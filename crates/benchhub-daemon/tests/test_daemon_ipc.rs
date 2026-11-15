use benchhub_core::db::Db;
use benchhub_daemon::{load_daemon_profiles, serve_unix_socket, DaemonRuntime};
use benchhub_types::{
    AppSettings, DaemonEvent, DaemonPayload, DaemonRequest, DaemonResponse, GpuMode,
    HistorySortOrder, QueueItem, RunResult,
};
use std::path::Path;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::watch;

async fn rpc_call(socket_path: &Path, req: DaemonRequest) -> DaemonResponse {
    let stream = UnixStream::connect(socket_path)
        .await
        .expect("Failed to connect to test daemon socket");
    let (read_half, mut write_half) = stream.into_split();
    let encoded = serde_json::to_string(&req).unwrap() + "\n";
    write_half.write_all(encoded.as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).expect("Invalid DaemonResponse JSON")
}

async fn start_test_daemon(
    temp_dir: &TempDir,
) -> (
    std::path::PathBuf,
    watch::Sender<bool>,
    tokio::task::JoinHandle<()>,
) {
    let data_dir = temp_dir.path().join("data");
    let socket_path = temp_dir.path().join("benchhubd.sock");
    let profiles = load_daemon_profiles();
    let runtime = DaemonRuntime::new_with_config(data_dir, profiles)
        .await
        .expect("DaemonRuntime init failed");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let sock_clone = socket_path.clone();
    let handle = tokio::spawn(async move {
        let _ = serve_unix_socket(runtime, sock_clone, shutdown_rx).await;
    });

    for _ in 0..50 {
        if socket_path.exists() && UnixStream::connect(&socket_path).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    (socket_path, shutdown_tx, handle)
}

#[tokio::test]
async fn test_daemon_ipc_basic_rpcs_and_settings_and_queue() {
    let temp = TempDir::new().unwrap();
    let (sock, shutdown_tx, handle) = start_test_daemon(&temp).await;

    // 1. Ping
    let resp = rpc_call(&sock, DaemonRequest::Ping).await;
    assert_eq!(resp.into_result().unwrap(), DaemonPayload::Pong);

    // 2. GetCatalog
    let catalog_resp = rpc_call(&sock, DaemonRequest::GetCatalog)
        .await
        .into_result()
        .unwrap();
    let DaemonPayload::Catalog(profiles) = catalog_resp else {
        panic!("Expected Catalog payload");
    };
    assert_eq!(profiles.len(), 12);
    assert!(profiles
        .iter()
        .any(|p| p.id == "furmark" && p.test_type.as_deref() == Some("stress")));

    // 3. GetSettings / SaveSettings persistence
    let mut settings = AppSettings {
        telemetry_interval_ms: 250,
        language: "en".to_string(),
        terminal_buffer_lines: 500,
        ..AppSettings::default()
    };
    settings.theme.mode = "dark".to_string();
    settings.theme.ui_scale = 1.1;
    let save_resp = rpc_call(&sock, DaemonRequest::SaveSettings(settings.clone())).await;
    assert!(save_resp.ok);

    let loaded_resp = rpc_call(&sock, DaemonRequest::GetSettings)
        .await
        .into_result()
        .unwrap();
    let DaemonPayload::Settings(loaded) = loaded_resp else {
        panic!("Expected Settings payload");
    };
    assert_eq!(loaded.telemetry_interval_ms, 250);
    assert_eq!(loaded.language, "en");
    assert_eq!(loaded.terminal_buffer_lines, 500);
    assert_eq!(loaded.theme.mode, "dark");

    // 4. Enqueue / GetQueue / RemoveQueueItem / ClearQueue
    let item1 = QueueItem {
        id: "7zip".to_string(),
        version: "24.08".to_string(),
        gpu_mode: GpuMode::Auto,
        custom_args: None,
        duration_secs_override: None,
    };
    let item2 = QueueItem {
        id: "furmark".to_string(),
        version: "vulkan".to_string(),
        gpu_mode: GpuMode::NvidiaDgpu,
        custom_args: None,
        duration_secs_override: Some(60),
    };
    let q_resp1 = rpc_call(&sock, DaemonRequest::EnqueueItem(item1.clone()))
        .await
        .into_result()
        .unwrap();
    assert!(matches!(q_resp1, DaemonPayload::Queue(ref q) if q.len() == 1));

    let _ = rpc_call(&sock, DaemonRequest::EnqueueItem(item2.clone())).await;
    let q_get = rpc_call(&sock, DaemonRequest::GetQueue)
        .await
        .into_result()
        .unwrap();
    assert!(matches!(q_get, DaemonPayload::Queue(ref q) if q.len() == 2));

    let q_rm = rpc_call(&sock, DaemonRequest::RemoveQueueItem { index: 0 })
        .await
        .into_result()
        .unwrap();
    assert!(matches!(q_rm, DaemonPayload::Queue(ref q) if q.len() == 1 && q[0].id == "furmark"));

    let q_clr = rpc_call(&sock, DaemonRequest::ClearQueue)
        .await
        .into_result()
        .unwrap();
    assert!(matches!(q_clr, DaemonPayload::Queue(ref q) if q.is_empty()));

    // SetQueue replaces queue atomically and validates every item
    let batch = vec![item1.clone(), item2.clone()];
    let q_set = rpc_call(&sock, DaemonRequest::SetQueue(batch))
        .await
        .into_result()
        .unwrap();
    assert!(matches!(
        q_set,
        DaemonPayload::Queue(ref q) if q.len() == 2 && q[0].id == "7zip" && q[1].id == "furmark"
    ));

    let bad_batch_item = QueueItem {
        id: "7zip".to_string(),
        version: "24.08".to_string(),
        gpu_mode: GpuMode::Auto,
        custom_args: Some("-y ; rm -rf /".to_string()),
        duration_secs_override: None,
    };
    let bad_set_resp = rpc_call(&sock, DaemonRequest::SetQueue(vec![bad_batch_item])).await;
    assert!(!bad_set_resp.ok);

    let _ = rpc_call(&sock, DaemonRequest::ClearQueue).await;

    // 5. Reject unsafe custom args in EnqueueItem
    let bad_item = QueueItem {
        id: "7zip".to_string(),
        version: "24.08".to_string(),
        gpu_mode: GpuMode::Auto,
        custom_args: Some("-y ; rm -rf /".to_string()),
        duration_secs_override: None,
    };
    let bad_resp = rpc_call(&sock, DaemonRequest::EnqueueItem(bad_item)).await;
    assert!(!bad_resp.ok);

    // 6. GetEulaText
    let eula_resp = rpc_call(
        &sock,
        DaemonRequest::GetEulaText {
            id: "7zip".to_string(),
        },
    )
    .await
    .into_result()
    .unwrap();
    let DaemonPayload::Text(eula_text) = eula_resp else {
        panic!("Expected Text payload");
    };
    assert!(!eula_text.trim().is_empty());

    // 7. Reject installation when EULA is not explicitly accepted
    let rejected_install = rpc_call(
        &sock,
        DaemonRequest::InstallBenchmark {
            id: "7zip".to_string(),
            version: "24.08".to_string(),
            eula_accepted: false,
        },
    )
    .await;
    assert!(!rejected_install.ok);
    assert!(rejected_install.error.unwrap_or_default().contains("EULA"));

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
}

#[tokio::test]
async fn test_daemon_ipc_history_comparison_and_event_subscription() {
    let temp = TempDir::new().unwrap();
    let (sock, shutdown_tx, handle) = start_test_daemon(&temp).await;

    // Seed two runs in SQLite directly inside data_dir/benchhub.db
    let db = Db::new(temp.path().join("data/benchhub.db")).unwrap();
    let run_a = RunResult {
        benchmark_id: "7zip".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "24.08".to_string(),
        gpu_mode: "auto".to_string(),
        score: Some(100_000.0),
        status: "completed".to_string(),
        timestamp: 1_700_000_000,
        duration_secs: 20.0,
        ..RunResult::default()
    };
    let run_b = RunResult {
        benchmark_id: "7zip".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "24.08".to_string(),
        gpu_mode: "auto".to_string(),
        score: Some(110_000.0),
        status: "completed".to_string(),
        timestamp: 1_700_000_100,
        duration_secs: 18.0,
        ..RunResult::default()
    };
    let id_a = db.insert_run(&run_a).unwrap();
    let id_b = db.insert_run(&run_b).unwrap();

    // Query filtered history via IPC
    let hist_resp = rpc_call(
        &sock,
        DaemonRequest::GetFilteredHistory {
            search: String::new(),
            category: "all".to_string(),
            gpu_mode: "all".to_string(),
            status: "all".to_string(),
            sort_order: HistorySortOrder::DateDesc,
        },
    )
    .await
    .into_result()
    .unwrap();
    let DaemonPayload::History(runs) = hist_resp else {
        panic!("Expected History payload");
    };
    assert_eq!(runs.len(), 2);

    // CompareRuns via IPC
    let cmp_resp = rpc_call(&sock, DaemonRequest::CompareRuns { id_a, id_b })
        .await
        .into_result()
        .unwrap();
    let DaemonPayload::Comparison(Some(diff)) = cmp_resp else {
        panic!("Expected Comparison(Some) payload");
    };
    assert_eq!(diff.score_diff, Some(10_000.0));

    // UpdateRunScore updates score in SQLite and GetRunLog reads run log safely
    let logs_dir = temp.path().join("data/logs");
    std::fs::create_dir_all(&logs_dir).unwrap();
    let run_log_path = logs_dir.join("7zip_test.log");
    std::fs::write(
        &run_log_path,
        "\x1b[32m7-Zip score: 123456\x1b[0m\nhttps://example.com/run/1\n",
    )
    .unwrap();
    let run_with_log = RunResult {
        benchmark_id: "7zip".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "24.08".to_string(),
        gpu_mode: "auto".to_string(),
        score: None,
        status: "completed".to_string(),
        timestamp: 1_700_000_200,
        duration_secs: 15.0,
        log_path: run_log_path.to_string_lossy().to_string(),
        ..RunResult::default()
    };
    let id_c = db.insert_run(&run_with_log).unwrap();

    let updated_resp = rpc_call(
        &sock,
        DaemonRequest::UpdateRunScore {
            id: id_c,
            score: Some(123_456.0),
        },
    )
    .await
    .into_result()
    .unwrap();
    let DaemonPayload::Run(updated_run) = updated_resp else {
        panic!("Expected Run payload from UpdateRunScore");
    };
    assert_eq!(updated_run.score, Some(123_456.0));

    let log_resp = rpc_call(&sock, DaemonRequest::GetRunLog { id: id_c })
        .await
        .into_result()
        .unwrap();
    let DaemonPayload::Text(log_txt) = log_resp else {
        panic!("Expected Text payload from GetRunLog");
    };
    assert!(log_txt.contains("7-Zip score: 123456"));
    assert!(log_txt.contains("https://example.com/run/1"));
    assert!(!log_txt.contains("\x1b["));

    let outside_file = temp.path().join("secret.txt");
    std::fs::write(&outside_file, "top secret").unwrap();
    let outside_run = RunResult {
        benchmark_id: "7zip".to_string(),
        log_path: outside_file.to_string_lossy().to_string(),
        timestamp: 1_700_000_300,
        ..RunResult::default()
    };
    let id_outside = db.insert_run(&outside_run).unwrap();
    let outside_resp = rpc_call(&sock, DaemonRequest::GetRunLog { id: id_outside }).await;
    assert!(
        !outside_resp.ok,
        "GetRunLog must reject log paths outside data/logs"
    );

    // SubscribeEvents initial state snapshot
    let stream = UnixStream::connect(&sock).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let sub_req = serde_json::to_string(&DaemonRequest::SubscribeEvents).unwrap() + "\n";
    write_half.write_all(sub_req.as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let event: DaemonEvent = serde_json::from_str(&line).expect("Valid DaemonEvent JSON");
    assert!(matches!(event, DaemonEvent::StateChanged(_)));

    // ClearTerminalLogs broadcasts DaemonEvent::ClearTerminal cleanly over event stream
    let clr_resp = rpc_call(&sock, DaemonRequest::ClearTerminalLogs).await;
    assert!(clr_resp.ok);

    let mut saw_clear_event = false;
    for _ in 0..8 {
        line.clear();
        let read_fut = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            reader.read_line(&mut line),
        )
        .await;
        match read_fut {
            Ok(Ok(n)) if n > 0 => {
                if let Ok(DaemonEvent::ClearTerminal) = serde_json::from_str::<DaemonEvent>(&line) {
                    saw_clear_event = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_clear_event,
        "Expected DaemonEvent::ClearTerminal over event stream"
    );

    // GetDaemonInfo returns current process & binary metadata
    let info_resp = rpc_call(&sock, DaemonRequest::GetDaemonInfo)
        .await
        .into_result()
        .unwrap();
    let DaemonPayload::DaemonInfo(info) = info_resp else {
        panic!("Expected DaemonInfo payload");
    };
    assert_eq!(info.pid, std::process::id());
    assert!(!info.exe_path.is_empty());
    assert!(!info.exe_deleted);
    assert!(!info.has_active_task);

    // SetMonitorActive(true) switches cadence and prompts immediate telemetry over IPC
    let mon_resp = rpc_call(&sock, DaemonRequest::SetMonitorActive(true)).await;
    assert_eq!(mon_resp.into_result().unwrap(), DaemonPayload::Unit);
    let mut saw_tele_event = false;
    for _ in 0..8 {
        line.clear();
        let read_fut = tokio::time::timeout(
            std::time::Duration::from_millis(800),
            reader.read_line(&mut line),
        )
        .await;
        match read_fut {
            Ok(Ok(n)) if n > 0 => {
                if let Ok(DaemonEvent::TelemetrySample(_)) =
                    serde_json::from_str::<DaemonEvent>(&line)
                {
                    saw_tele_event = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_tele_event,
        "Expected DaemonEvent::TelemetrySample after SetMonitorActive(true)"
    );

    // Shutdown cleanly terminates serve_unix_socket and removes the IPC socket
    let shut_resp = rpc_call(&sock, DaemonRequest::Shutdown).await;
    assert!(shut_resp.ok);
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("Daemon did not shut down cleanly within timeout")
        .unwrap();
    assert!(
        !sock.exists(),
        "Socket file should be removed after clean Shutdown"
    );
    drop(shutdown_tx);
}
