use benchhub_core::db::Db;
use benchhub_core::telemetry;
use benchhub_daemon::{load_daemon_profiles, DaemonRuntime};
use benchhub_types::{
    AppSettings, DaemonPayload, DaemonRequest, GpuMode, HistorySortOrder, QueueItem,
    RunBenchmarkParams, RunResult,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

fn assert_finite_telemetry(sample: &benchhub_types::TelemetryData, label: &str) {
    assert!(
        sample.cpu_temp.is_finite(),
        "{}: cpu_temp not finite",
        label
    );
    assert!(
        sample.gpu_temp.is_finite(),
        "{}: gpu_temp not finite",
        label
    );
    assert!(
        sample.cpu_usage.is_finite(),
        "{}: cpu_usage not finite",
        label
    );
    assert!(
        sample.gpu_usage.is_finite(),
        "{}: gpu_usage not finite",
        label
    );
    assert!(
        sample.cpu_freq.is_finite(),
        "{}: cpu_freq not finite",
        label
    );
    assert!(
        sample.gpu_freq_mhz.is_finite(),
        "{}: gpu_freq_mhz not finite",
        label
    );
    assert!(
        sample.vram_freq_mhz.is_finite(),
        "{}: vram_freq_mhz not finite",
        label
    );
    assert!(sample.power_w.is_finite(), "{}: power_w not finite", label);
    assert!(
        sample.gpu_power_w.is_finite(),
        "{}: gpu_power_w not finite",
        label
    );
    assert!(
        sample.ac_power_w.is_finite(),
        "{}: ac_power_w not finite",
        label
    );
    assert!(
        sample.ram_usage_mb.is_finite(),
        "{}: ram_usage_mb not finite",
        label
    );
    assert!(
        sample.vram_usage_mb.is_finite(),
        "{}: vram_usage_mb not finite",
        label
    );
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if ty.is_symlink() {
            let link_target = std::fs::read_link(entry.path())?;
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(link_target, target);
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn copy_cached_runner(runners_dir: &Path, bench_id: &str) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let src = PathBuf::from(home)
        .join(".local/share/benchhub/runners")
        .join(bench_id);
    if src.exists() {
        let dst = runners_dir.join(bench_id);
        if !dst.exists() {
            let _ = copy_dir_recursive(&src, &dst);
        }
    }
}

fn verify_run_and_print(
    label: &str,
    settings_desc: &str,
    run: &RunResult,
    points: &[benchhub_types::TelemetryData],
    log_text: &str,
) {
    println!(
        "RUN_ROW | {} | {} | {} | {} | score={:?} | dur={:.2}s | samples={} | cpu={:.1}/{:.1}% | cpu_temp={:.1}/{:.1}C | cpu_mhz={}/{} | ram={:.2}/{:.2}GB | gpu={:.1}/{:.1}% temp={:.1}/{:.1}C mhz={}/{} vram={:.2}/{:.2}GB gpow={:.1}/{:.1}W | pow={:.1}/{:.1}W ac={:.1}/{:.1}W | throttle={}/{}",
        label,
        run.benchmark_id,
        run.preset_or_version,
        settings_desc,
        run.score,
        run.duration_secs,
        points.len(),
        run.avg_cpu_usage,
        run.peak_cpu_usage,
        run.avg_cpu_temp,
        run.peak_cpu_temp,
        run.avg_cpu_freq_mhz,
        run.peak_cpu_freq_mhz,
        run.avg_ram_gb,
        run.peak_ram_gb,
        run.avg_gpu_usage,
        run.peak_gpu_usage,
        run.avg_gpu_temp,
        run.peak_gpu_temp,
        run.avg_gpu_freq_mhz,
        run.peak_gpu_freq_mhz,
        run.avg_vram_gb,
        run.peak_vram_gb,
        run.avg_gpu_power_w,
        run.peak_gpu_power_w,
        run.avg_power_w,
        run.peak_power_w,
        run.avg_ac_power_w,
        run.peak_ac_power_w,
        run.cpu_throttling,
        run.gpu_throttling,
    );

    assert_eq!(run.status, "completed", "{}: status != completed", label);
    let score = run.score.expect("score should be Some");
    assert!(score > 0.0, "{}: score <= 0: {}", label, score);
    assert!(run.duration_secs > 0.0, "{}: duration_secs <= 0", label);
    assert!(run.avg_cpu_usage > 0.0, "{}: avg_cpu_usage == 0", label);
    assert!(
        run.peak_cpu_usage >= run.avg_cpu_usage,
        "{}: peak_cpu_usage < avg_cpu_usage",
        label
    );
    assert!(run.avg_cpu_freq_mhz > 0, "{}: avg_cpu_freq_mhz == 0", label);
    assert!(
        run.peak_cpu_freq_mhz >= run.avg_cpu_freq_mhz,
        "{}: peak_cpu_freq_mhz < avg_cpu_freq_mhz",
        label
    );
    assert!(run.avg_cpu_temp > 0.0, "{}: avg_cpu_temp == 0", label);
    assert!(
        run.peak_cpu_temp >= run.avg_cpu_temp,
        "{}: peak_cpu_temp < avg_cpu_temp",
        label
    );
    assert!(run.avg_ram_gb > 0.0, "{}: avg_ram_gb == 0", label);
    assert!(
        run.peak_ram_gb >= run.avg_ram_gb,
        "{}: peak_ram_gb < avg_ram_gb",
        label
    );
    assert!(
        !run.system_info_summary.trim().is_empty(),
        "{}: empty system_info_summary",
        label
    );
    assert!(
        !run.power_profile.trim().is_empty(),
        "{}: empty power_profile",
        label
    );
    assert!(
        Path::new(&run.log_path).exists(),
        "{}: log_path does not exist: {}",
        label,
        run.log_path
    );
    assert!(!log_text.trim().is_empty(), "{}: log_text is empty", label);
    assert!(
        !points.is_empty(),
        "{}: telemetry_points is empty for run_id {}",
        label,
        run.id
    );
    for (idx, pt) in points.iter().enumerate() {
        assert!(pt.timestamp > 0, "{}: point {} timestamp <= 0", label, idx);
        assert_finite_telemetry(pt, &format!("{} pt {}", label, idx));
    }
}

#[tokio::test]
#[ignore = "live hardware telemetry and benchmark execution test"]
async fn live_e2e_telemetry_and_benchmarks() {
    println!("=== STEP 1: HARDWARE SENSOR & TELEMETRY BASELINE ===");
    let diagnosis = telemetry::get_sensor_diagnosis();
    let sys_info = telemetry::get_system_info_summary();
    let power_prof = telemetry::get_power_profile();
    println!("SENSOR_DIAGNOSIS:\n{}", diagnosis);
    println!("SYSTEM_INFO_SUMMARY: {}", sys_info);
    println!("POWER_PROFILE: {}", power_prof);

    assert!(!diagnosis.trim().is_empty());
    assert!(!sys_info.trim().is_empty());
    assert!(!power_prof.trim().is_empty());

    let mut engine = telemetry::TelemetryEngine::new();
    for mode in [GpuMode::Auto, GpuMode::Integrated, GpuMode::NvidiaDgpu] {
        for tick in 0..3 {
            let sample = engine.read_current_for_mode(mode);
            assert_finite_telemetry(&sample, &format!("{:?} tick {}", mode, tick));
            if tick == 2 {
                println!(
                    "LIVE_SAMPLE {:?}: cpu={:.1}% temp={:.1}C freq={:.0}MHz ram={:.0}MB | gpu={:.1}% temp={:.1}C freq={:.0}MHz vram={:.0}MB gpow={:.1}W | pow={:.1}W ac={:.1}W | throttle={}/{}",
                    mode,
                    sample.cpu_usage,
                    sample.cpu_temp,
                    sample.cpu_freq,
                    sample.ram_usage_mb,
                    sample.gpu_usage,
                    sample.gpu_temp,
                    sample.gpu_freq_mhz,
                    sample.vram_usage_mb,
                    sample.gpu_power_w,
                    sample.power_w,
                    sample.ac_power_w,
                    sample.cpu_throttle,
                    sample.gpu_throttle,
                );
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    println!("\n=== STEP 2 & 3: DAEMON RUNTIME E2E BENCHMARKS ===");
    let temp = TempDir::new().expect("tempdir");
    let data_dir = temp.path().join("data");
    let runners_dir = data_dir.join("runners");
    std::fs::create_dir_all(&runners_dir).expect("create runners dir");

    // Copy heavy/already-cached runners; leave `cray` and `stream` to be installed fresh via DaemonRequest::InstallBenchmark.
    for id in ["7zip", "ycruncher", "fio"] {
        copy_cached_runner(&runners_dir, id);
    }

    let profiles = load_daemon_profiles();
    let runtime = DaemonRuntime::new_with_config(data_dir.clone(), profiles)
        .await
        .expect("DaemonRuntime init");
    let db = Db::new(data_dir.join("benchhub.db")).expect("open db");

    // Save initial settings with fast telemetry polling (250ms)
    let settings = AppSettings {
        telemetry_interval_ms: 250,
        selected_gpu_mode: GpuMode::Auto,
        save_stopped_runs: true,
        disable_telemetry_during_runs: false,
        ..AppSettings::default()
    };
    let resp = runtime
        .handle_request(DaemonRequest::SaveSettings(settings))
        .await;
    assert!(resp.ok, "SaveSettings failed: {:?}", resp.error);

    // Install cray 720p & 1080p fresh
    for ver in ["720p", "1080p"] {
        let resp = runtime
            .handle_request(DaemonRequest::InstallBenchmark {
                id: "cray".to_string(),
                version: ver.to_string(),
                eula_accepted: true,
            })
            .await;
        assert!(resp.ok, "Install cray {} failed: {:?}", ver, resp.error);
    }

    // Ensure stream Compact/Standard, 7zip, ycruncher, fio are installed (will be instant if symlinked, or install if missing)
    for (id, ver) in [
        ("stream", "Compact"),
        ("stream", "Standard"),
        ("7zip", "24.08"),
        ("ycruncher", "25M"),
        ("fio", "Seq-Read"),
        ("fio", "Rand-Read-4K"),
    ] {
        let resp = runtime
            .handle_request(DaemonRequest::InstallBenchmark {
                id: id.to_string(),
                version: ver.to_string(),
                eula_accepted: true,
            })
            .await;
        assert!(resp.ok, "Install {} {} failed: {:?}", id, ver, resp.error);
    }

    let cases: Vec<(&str, &str, RunBenchmarkParams)> = vec![
        (
            "cray_720p_default",
            "default, GpuMode::Auto, interval=250ms",
            RunBenchmarkParams {
                id: "cray".to_string(),
                version: "720p".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: None,
            },
        ),
        (
            "cray_1080p_custom_args",
            "custom_args='-i sphfract -s 640x480 -r 4', GpuMode::Auto",
            RunBenchmarkParams {
                id: "cray".to_string(),
                version: "1080p".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: Some("-i sphfract -s 640x480 -r 4".to_string()),
                duration_secs_override: None,
            },
        ),
        (
            "stream_compact",
            "default, GpuMode::Auto",
            RunBenchmarkParams {
                id: "stream".to_string(),
                version: "Compact".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: None,
            },
        ),
        (
            "stream_standard_igpu",
            "default, GpuMode::Integrated",
            RunBenchmarkParams {
                id: "stream".to_string(),
                version: "Standard".to_string(),
                gpu_mode: GpuMode::Integrated,
                custom_args: None,
                duration_secs_override: None,
            },
        ),
        (
            "7zip_default",
            "default, GpuMode::Auto",
            RunBenchmarkParams {
                id: "7zip".to_string(),
                version: "24.08".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: None,
            },
        ),
        (
            "7zip_custom_2threads",
            "custom_args='b -mmt=2', GpuMode::Auto",
            RunBenchmarkParams {
                id: "7zip".to_string(),
                version: "24.08".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: Some("b -mmt=2".to_string()),
                duration_secs_override: None,
            },
        ),
        (
            "ycruncher_25m",
            "default, GpuMode::Auto",
            RunBenchmarkParams {
                id: "ycruncher".to_string(),
                version: "25M".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: None,
            },
        ),
        (
            "fio_seq_read_3s",
            "duration_override=3s, GpuMode::Auto",
            RunBenchmarkParams {
                id: "fio".to_string(),
                version: "Seq-Read".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: Some(3),
            },
        ),
        (
            "fio_rand_read_4k_3s",
            "duration_override=3s, GpuMode::Auto",
            RunBenchmarkParams {
                id: "fio".to_string(),
                version: "Rand-Read-4K".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: Some(3),
            },
        ),
    ];

    for (label, settings_desc, params) in cases {
        let resp = runtime
            .handle_request(DaemonRequest::RunBenchmark(params))
            .await;
        let payload = resp
            .into_result()
            .unwrap_or_else(|e| panic!("{} RunBenchmark failed: {}", label, e));
        let DaemonPayload::RunOutcome(outcome) = payload else {
            panic!("Expected RunOutcome");
        };
        let run_id = outcome.run_id.expect("run_id should be Some");

        let DaemonPayload::RunOpt(Some(run)) = runtime
            .handle_request(DaemonRequest::GetRunById { id: run_id })
            .await
            .into_result()
            .expect("GetRunById")
        else {
            panic!("Run not found for id {}", run_id);
        };

        let DaemonPayload::Text(log_text) = runtime
            .handle_request(DaemonRequest::GetRunLog { id: run_id })
            .await
            .into_result()
            .expect("GetRunLog")
        else {
            panic!("Expected Text log");
        };

        let points = db
            .get_telemetry_points(run_id)
            .expect("get_telemetry_points");
        verify_run_and_print(label, settings_desc, &run, &points, &log_text);
    }

    // Queue execution test with updated settings
    let settings2 = AppSettings {
        telemetry_interval_ms: 250,
        selected_gpu_mode: GpuMode::Integrated,
        save_stopped_runs: true,
        disable_telemetry_during_runs: false,
        ..AppSettings::default()
    };
    assert!(
        runtime
            .handle_request(DaemonRequest::SaveSettings(settings2))
            .await
            .ok
    );

    let q1 = QueueItem {
        id: "cray".to_string(),
        version: "720p".to_string(),
        gpu_mode: GpuMode::Integrated,
        custom_args: None,
        duration_secs_override: None,
    };
    let q2 = QueueItem {
        id: "stream".to_string(),
        version: "Compact".to_string(),
        gpu_mode: GpuMode::Integrated,
        custom_args: None,
        duration_secs_override: None,
    };
    assert!(
        runtime
            .handle_request(DaemonRequest::EnqueueItem(q1))
            .await
            .ok
    );
    assert!(
        runtime
            .handle_request(DaemonRequest::EnqueueItem(q2))
            .await
            .ok
    );
    assert!(runtime.handle_request(DaemonRequest::RunQueue).await.ok);

    // Wait for queue completion
    for _ in 0..240 {
        let DaemonPayload::Snapshot(snap) = runtime
            .handle_request(DaemonRequest::GetStateSnapshot)
            .await
            .into_result()
            .expect("GetStateSnapshot")
        else {
            panic!("Expected Snapshot");
        };
        if !snap.is_queue_running && snap.active_task.is_none() && snap.queue.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let DaemonPayload::History(history) = runtime
        .handle_request(DaemonRequest::GetFilteredHistory {
            search: String::new(),
            category: "all".to_string(),
            gpu_mode: "all".to_string(),
            status: "all".to_string(),
            sort_order: HistorySortOrder::DateDesc,
        })
        .await
        .into_result()
        .expect("GetFilteredHistory")
    else {
        panic!("Expected History");
    };
    assert_eq!(history.len(), 11, "Expected 9 direct runs + 2 queued runs");

    for (idx, run) in history.iter().take(2).enumerate() {
        let DaemonPayload::Text(log_text) = runtime
            .handle_request(DaemonRequest::GetRunLog { id: run.id })
            .await
            .into_result()
            .expect("GetRunLog")
        else {
            panic!("Expected Text log");
        };
        let points = db
            .get_telemetry_points(run.id)
            .expect("get_telemetry_points");
        verify_run_and_print(
            &format!("queue_item_{}", 2 - idx),
            "queued, GpuMode::Integrated, interval=250ms",
            run,
            &points,
            &log_text,
        );
    }
}
