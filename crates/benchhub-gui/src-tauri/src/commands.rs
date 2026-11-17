//! Tauri Thin IPC Commands
//!
//! Exposes desktop locale/theme inspection directly and forwards all benchmark,
//! telemetry, queue, history, and configuration operations to `benchhubd`.

use crate::state::AppState;
use crate::theme_detect;
use benchhub_types::{
    AppSettings, BenchmarkProfile, DaemonPayload, DaemonRequest, DaemonStateSnapshot, GpuMode,
    HistorySortOrder, QueueItem, RunBenchmarkParams, RunComparisonDiff, RunOutcome, RunResult,
    SystemThemeInfo,
};

async fn call_unit(state: &tauri::State<'_, AppState>, req: &DaemonRequest) -> Result<(), String> {
    state.daemon.call(req).await?;
    Ok(())
}

macro_rules! daemon_call {
    ($state:expr, $req:expr, $variant:ident) => {
        match $state.daemon.call(&$req).await? {
            DaemonPayload::$variant(val) => Ok(val),
            other => Err(format!("Unexpected daemon response: {:?}", other)),
        }
    };
    ($state:expr, $req:expr, *$variant:ident) => {
        daemon_call!($state, $req, $variant).map(|boxed| *boxed)
    };
}

#[tauri::command]
pub fn get_system_locale() -> String {
    theme_detect::detect_system_locale()
}

#[tauri::command]
pub fn get_system_theme_info() -> SystemThemeInfo {
    theme_detect::detect_system_theme_info()
}

#[tauri::command]
pub async fn get_daemon_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<DaemonStateSnapshot, String> {
    daemon_call!(state, DaemonRequest::GetStateSnapshot, *Snapshot)
}

#[tauri::command]
pub async fn get_catalog(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<BenchmarkProfile>, String> {
    daemon_call!(state, DaemonRequest::GetCatalog, Catalog)
}

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    daemon_call!(state, DaemonRequest::GetSettings, *Settings)
}

#[tauri::command]
pub async fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::SaveSettings(settings)).await
}

#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, AppState>,
    search: String,
    category: String,
    gpu_mode: String,
    status: String,
    sort: HistorySortOrder,
) -> Result<Vec<RunResult>, String> {
    daemon_call!(
        state,
        DaemonRequest::GetFilteredHistory {
            search,
            category,
            gpu_mode,
            status,
            sort_order: sort,
        },
        History
    )
}

#[tauri::command]
pub async fn delete_history(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::DeleteHistory { id }).await
}

#[tauri::command]
pub async fn clear_history(state: tauri::State<'_, AppState>) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::ClearHistory).await
}

#[tauri::command]
pub async fn update_run_score(
    state: tauri::State<'_, AppState>,
    id: i64,
    score: Option<f64>,
) -> Result<RunResult, String> {
    daemon_call!(state, DaemonRequest::UpdateRunScore { id, score }, *Run)
}

#[tauri::command]
pub async fn get_run_log(state: tauri::State<'_, AppState>, id: i64) -> Result<String, String> {
    daemon_call!(state, DaemonRequest::GetRunLog { id }, Text)
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://"))
        || trimmed.len() > 2048
        || trimmed.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("Refusing to open invalid or non-HTTP(S) URL".to_string());
    }
    let mut child = std::process::Command::new("xdg-open")
        .arg(trimmed)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch browser: {}", e))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[tauri::command]
pub async fn compare_runs(
    state: tauri::State<'_, AppState>,
    id_a: i64,
    id_b: i64,
) -> Result<Option<RunComparisonDiff>, String> {
    daemon_call!(state, DaemonRequest::CompareRuns { id_a, id_b }, Comparison)
        .map(|opt| opt.map(|b| *b))
}

#[tauri::command]
pub async fn create_methodology(
    state: tauri::State<'_, AppState>,
    run_ids: Vec<i64>,
) -> Result<RunResult, String> {
    daemon_call!(
        state,
        DaemonRequest::CreateMethodology { child_ids: run_ids },
        *Run
    )
}

#[tauri::command]
pub async fn get_methodology_children(
    state: tauri::State<'_, AppState>,
    methodology_id: i64,
) -> Result<Vec<RunResult>, String> {
    daemon_call!(
        state,
        DaemonRequest::GetMethodologyChildren { methodology_id },
        History
    )
}

#[tauri::command]
pub async fn group_runs(
    state: tauri::State<'_, AppState>,
    run_ids: Vec<i64>,
    group_name: String,
) -> Result<(), String> {
    call_unit(
        &state,
        &DaemonRequest::SetRunsGroup {
            ids: run_ids,
            group_name,
        },
    )
    .await
}

#[tauri::command]
pub async fn ungroup_runs(
    state: tauri::State<'_, AppState>,
    run_ids: Vec<i64>,
) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::RemoveRunsFromGroup { ids: run_ids }).await
}

#[tauri::command]
pub async fn install_benchmark(
    state: tauri::State<'_, AppState>,
    id: String,
    version: String,
    eula_accepted: bool,
) -> Result<(), String> {
    call_unit(
        &state,
        &DaemonRequest::InstallBenchmark {
            id,
            version,
            eula_accepted,
        },
    )
    .await
}

#[tauri::command]
pub async fn uninstall_benchmark(
    state: tauri::State<'_, AppState>,
    id: String,
    version: String,
) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::UninstallBenchmark { id, version }).await
}

#[tauri::command]
pub async fn run_benchmark(
    state: tauri::State<'_, AppState>,
    id: String,
    version: String,
    gpu_mode: GpuMode,
    custom_args: Option<String>,
    duration_secs_override: Option<u64>,
) -> Result<RunOutcome, String> {
    daemon_call!(
        state,
        DaemonRequest::RunBenchmark(RunBenchmarkParams {
            id,
            version,
            gpu_mode,
            custom_args,
            duration_secs_override,
        }),
        RunOutcome
    )
}

#[tauri::command]
pub async fn cancel_active_run(state: tauri::State<'_, AppState>) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::CancelActiveRun).await
}

#[tauri::command]
pub async fn clear_queue(state: tauri::State<'_, AppState>) -> Result<Vec<QueueItem>, String> {
    daemon_call!(state, DaemonRequest::ClearQueue, Queue)
}

#[tauri::command]
pub async fn set_queue(
    state: tauri::State<'_, AppState>,
    items: Vec<QueueItem>,
) -> Result<Vec<QueueItem>, String> {
    daemon_call!(state, DaemonRequest::SetQueue(items), Queue)
}

#[tauri::command]
pub async fn run_queue(state: tauri::State<'_, AppState>) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::RunQueue).await
}

#[tauri::command]
pub async fn get_eula_text(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    daemon_call!(state, DaemonRequest::GetEulaText { id }, Text)
}

#[tauri::command]
pub async fn export_history_csv(state: tauri::State<'_, AppState>) -> Result<String, String> {
    daemon_call!(state, DaemonRequest::ExportHistoryCsv, Text)
}

#[tauri::command]
pub async fn export_comparison_png(
    state: tauri::State<'_, AppState>,
    run_ids: Vec<i64>,
) -> Result<String, String> {
    daemon_call!(state, DaemonRequest::ExportComparisonPng { run_ids }, Text)
}

#[tauri::command]
pub async fn get_hardware_capabilities(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    daemon_call!(state, DaemonRequest::GetHardwareCapabilities, Text)
}

#[tauri::command]
pub async fn get_power_profile(state: tauri::State<'_, AppState>) -> Result<String, String> {
    daemon_call!(state, DaemonRequest::GetPowerProfile, Text)
}

#[tauri::command]
pub async fn clear_terminal_logs(state: tauri::State<'_, AppState>) -> Result<(), String> {
    call_unit(&state, &DaemonRequest::ClearTerminalLogs).await
}

#[tauri::command]
pub async fn set_monitor_active(
    state: tauri::State<'_, AppState>,
    active: bool,
) -> Result<(), String> {
    state.daemon.set_monitor_active(active).await
}
