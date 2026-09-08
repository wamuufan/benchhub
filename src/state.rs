use benchhub::models::{AppSettings, GpuMode, HistorySortOrder, TelemetryData};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<benchhub::service::BenchHubService>,
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub config_path: PathBuf,
    pub settings: Arc<std::sync::Mutex<AppSettings>>,
    pub history_filter_state: Arc<std::sync::Mutex<HistoryFilterState>>,
    pub active_cancel_tx: Arc<Mutex<Option<watch::Sender<bool>>>>,
    pub terminal_tx: mpsc::Sender<TerminalMsg>,
    pub telemetry_interval_tx: watch::Sender<u64>,
    pub terminal_limit_tx: watch::Sender<usize>,
    pub current_samples: Arc<Mutex<Vec<TelemetryData>>>,
    pub is_recording_samples: Arc<AtomicBool>,
    pub active_run_id: Arc<AtomicI64>,
    pub save_stopped_runs_flag: Arc<AtomicBool>,
    pub selected_gpu_mode: Arc<std::sync::Mutex<GpuMode>>,
    pub max_test_duration_secs: Arc<std::sync::atomic::AtomicU64>,
    pub custom_configs: Arc<
        std::sync::Mutex<
            std::collections::HashMap<String, benchhub::bench_config::BenchmarkCustomConfig>,
        >,
    >,
    pub is_queue_running: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub enum TerminalMsg {
    Append(String),
    Clear,
}

pub struct HistoryFilterState {
    pub search: String,
    pub category: String,
    pub gpu_mode: String,
    pub status: String,
    pub sort_order: HistorySortOrder,
    pub selected_ids: Vec<i64>,
    pub expanded_methodology_ids: std::collections::HashSet<i64>,
    pub selected_group: Option<String>,
    pub collapsed_group_names: std::collections::HashSet<String>,
}
