slint::include_modules!();

pub mod callbacks;
pub mod commands;
pub mod state;
pub mod ui;

use benchhub::db::Db;
use benchhub::engine::BenchmarkEngine;
use benchhub::models::{GpuMode, HistorySortOrder, TelemetryData};
use benchhub::telemetry;
use std::sync::{atomic::AtomicBool, atomic::AtomicI64, Arc, Mutex};
use tokio::sync::{mpsc, watch};

use crate::commands::{
    get_data_dir, load_profiles, load_settings, spawn_log_forwarder, spawn_telemetry_worker,
    spawn_terminal_flusher,
};
use crate::state::{AppState, HistoryFilterState, TerminalMsg};
use crate::ui::setup_initial_ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing logging and channel
    let (log_tx, log_rx) = mpsc::channel::<String>(1024);
    benchhub::logging::init_tracing(log_tx);

    // Initialize application directories
    let data_dir = get_data_dir()?;
    let logs_dir = data_dir.join("logs");
    let runners_dir = data_dir.join("runners");
    std::fs::create_dir_all(&logs_dir)?;
    std::fs::create_dir_all(&runners_dir)?;

    // Load configuration & persistent logs
    let config_path = data_dir.join("config.json");
    let settings = load_settings(&config_path);
    benchhub::i18n::set_language(&settings.language);
    let console_log_path = logs_dir.join("benchhub_console.log");

    // Initialize database, engine, and service layer
    let db = Db::new(data_dir.join("benchhub.sqlite"))?;
    let engine = BenchmarkEngine::new(data_dir.clone());
    let profiles = match load_profiles().await {
        Ok(profs) => {
            tracing::info!("{} benchmark profili başarıyla yüklendi", profs.len());
            Arc::new(profs)
        }
        Err(err) => {
            tracing::error!("Profil yükleme hatası: {:?}", err);
            Arc::new(Vec::new())
        }
    };

    let service = Arc::new(benchhub::service::BenchHubService::new(
        db,
        engine,
        profiles.clone(),
        data_dir.clone(),
    ));

    // Shared state primitives
    let initial_gpu_mode = GpuMode::from_display_str(&settings.selected_gpu_mode);
    let selected_gpu_mode = Arc::new(Mutex::new(initial_gpu_mode));
    let save_stopped_runs_flag = Arc::new(AtomicBool::new(settings.save_stopped_runs));

    let history_filter_state = Arc::new(Mutex::new(HistoryFilterState {
        search: String::new(),
        category: benchhub::i18n::t("cat_all"),
        gpu_mode: benchhub::i18n::t("gpu_filter_all"),
        status: benchhub::i18n::t("status_all"),
        sort_order: HistorySortOrder::DateDesc,
        selected_ids: Vec::new(),
        expanded_methodology_ids: std::collections::HashSet::new(),
        selected_group: None,
        collapsed_group_names: std::collections::HashSet::new(),
    }));

    let active_cancel_tx = Arc::new(tokio::sync::Mutex::new(None));
    let (telemetry_interval_tx, telemetry_interval_rx) =
        watch::channel(settings.telemetry_interval_ms);
    let (terminal_limit_tx, terminal_limit_rx) = watch::channel(settings.terminal_buffer_lines);
    let (terminal_tx, terminal_rx) = mpsc::channel::<TerminalMsg>(4096);

    let current_samples = Arc::new(tokio::sync::Mutex::new(Vec::<TelemetryData>::new()));
    let is_recording_samples = Arc::new(AtomicBool::new(false));
    let active_run_id = Arc::new(AtomicI64::new(0));

    let app_state = AppState {
        service,
        data_dir: data_dir.clone(),
        logs_dir: logs_dir.clone(),
        config_path: config_path.clone(),
        settings: Arc::new(Mutex::new(settings.clone())),
        history_filter_state,
        active_cancel_tx,
        terminal_tx: terminal_tx.clone(),
        telemetry_interval_tx,
        terminal_limit_tx,
        current_samples,
        is_recording_samples,
        active_run_id,
        save_stopped_runs_flag,
        selected_gpu_mode,
        max_test_duration_secs: Arc::new(std::sync::atomic::AtomicU64::new(
            settings.max_test_duration_secs,
        )),
        custom_configs: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        is_queue_running: Arc::new(AtomicBool::new(false)),
    };

    // Bootstrap Slint UI with a clean, fresh console
    let ui = AppWindow::new()?;

    let _ = std::fs::write(&console_log_path, "");
    let initial_console_log = String::new();

    setup_initial_ui(&ui, &app_state, &initial_console_log);

    // Spawn async background tasks with JoinHandles for graceful shutdown
    let (telemetry_tx, telemetry_rx) = mpsc::channel::<TelemetryData>(64);
    let telemetry_loop_handle = tokio::spawn(async move {
        telemetry::start_telemetry_loop(telemetry_tx, telemetry_interval_rx).await;
    });

    let telemetry_worker_handle =
        spawn_telemetry_worker(ui.as_weak(), app_state.clone(), telemetry_rx);
    let log_forwarder_handle = spawn_log_forwarder(log_rx, terminal_tx);
    let terminal_flusher_handle = spawn_terminal_flusher(
        ui.as_weak(),
        console_log_path,
        initial_console_log,
        terminal_limit_rx,
        terminal_rx,
    );

    // Setup SIGINT listener to perform graceful shutdown if Ctrl+C is received
    let ui_weak = ui.as_weak();
    let sigint_handle = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("SIGINT alındı, kapatılıyor...");
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = ui_weak.upgrade() {
                    let _ = w.hide();
                }
                let _ = slint::quit_event_loop();
            });
        }
    });

    // Register all Slint event callbacks
    crate::callbacks::register_callbacks(&ui, app_state.clone());

    // Run Slint Event Loop
    let run_res = ui.run();

    // Graceful Shutdown:
    tracing::info!("BenchHub kapatılıyor, arka plan görevleri sonlandırılıyor...");

    // 1. Cancel running benchmark child process if active to prevent orphan processes
    if let Some(cancel_tx) = app_state.active_cancel_tx.lock().await.take() {
        tracing::info!("Aktif benchmark işlemi iptal ediliyor...");
        let _ = cancel_tx.send(true);
    }

    // 2. Abort background task JoinHandles
    telemetry_loop_handle.abort();
    telemetry_worker_handle.abort();
    log_forwarder_handle.abort();
    terminal_flusher_handle.abort();
    sigint_handle.abort();

    tracing::info!("Tüm arka plan görevleri güvenle kapatıldı.");

    run_res?;
    Ok(())
}
