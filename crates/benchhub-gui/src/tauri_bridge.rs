//! WASM IPC Bridge to Tauri Desktop Shell (Proxied to `benchhubd`)
//!
//! Provides typed asynchronous wrappers for desktop theme/locale commands
//! and daemon-proxied RPC operations + event listeners.

use benchhub_types::{
    AppSettings, BenchmarkProfile, DaemonNotification, DaemonStateSnapshot, GpuMode,
    HistorySortOrder, QueueItem, RunComparisonDiff, RunOutcome, RunResult, SystemThemeInfo,
    TelemetryData,
};
use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen, catch)]
    async fn tauri_listen(event: &str, handler: &js_sys::Function) -> Result<JsValue, JsValue>;
}

fn format_invoke_err(err: JsValue) -> String {
    err.as_string().unwrap_or_else(|| format!("{:?}", err))
}

fn to_js_args<A: Serialize>(args: &A) -> Result<JsValue, String> {
    serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())
}

async fn invoke_no_args<R: DeserializeOwned>(cmd: &str) -> Result<R, String> {
    let res = tauri_invoke(cmd, JsValue::NULL)
        .await
        .map_err(format_invoke_err)?;
    serde_wasm_bindgen::from_value(res).map_err(|e| e.to_string())
}

async fn invoke_with_args<A: Serialize, R: DeserializeOwned>(
    cmd: &str,
    args: &A,
) -> Result<R, String> {
    let js_args = to_js_args(args)?;
    let res = tauri_invoke(cmd, js_args)
        .await
        .map_err(format_invoke_err)?;
    serde_wasm_bindgen::from_value(res).map_err(|e| e.to_string())
}

async fn invoke_unit_no_args(cmd: &str) -> Result<(), String> {
    tauri_invoke(cmd, JsValue::NULL)
        .await
        .map_err(format_invoke_err)?;
    Ok(())
}

async fn invoke_unit_with_args<A: Serialize>(cmd: &str, args: &A) -> Result<(), String> {
    let js_args = to_js_args(args)?;
    tauri_invoke(cmd, js_args)
        .await
        .map_err(format_invoke_err)?;
    Ok(())
}

async fn listen_raw<F>(event_name: &str, handler: F) -> Result<JsValue, String>
where
    F: FnMut(JsValue) + 'static,
{
    let closure = Closure::wrap(Box::new(handler) as Box<dyn FnMut(JsValue)>);
    let unlisten = tauri_listen(event_name, closure.as_ref().unchecked_ref())
        .await
        .map_err(format_invoke_err)?;
    closure.forget();
    Ok(unlisten)
}

async fn listen_typed<T, F>(event_name: &str, mut callback: F) -> Result<JsValue, String>
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    listen_raw(event_name, move |event_obj: JsValue| {
        if let Ok(payload) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload")) {
            if let Ok(data) = serde_wasm_bindgen::from_value::<T>(payload) {
                callback(data);
            }
        }
    })
    .await
}

/// Queries the host system locale string from the desktop shell.
pub async fn get_system_locale() -> Result<String, String> {
    match invoke_no_args("get_system_locale").await {
        Ok(locale) => Ok(locale),
        Err(_) => Ok("en".to_string()),
    }
}

/// Queries the desktop system theme palette (KDE Plasma Breeze, GNOME/GTK, or Portal).
pub async fn get_system_theme_info() -> Result<SystemThemeInfo, String> {
    match invoke_no_args("get_system_theme_info").await {
        Ok(info) => Ok(info),
        Err(_) => Ok(SystemThemeInfo::default()),
    }
}

/// Fetches a full synchronized snapshot of daemon state.
pub async fn get_daemon_snapshot() -> Result<DaemonStateSnapshot, String> {
    invoke_no_args("get_daemon_snapshot").await
}

/// Fetches all benchmark and stress profiles from `benchhubd`.
pub async fn get_catalog() -> Result<Vec<BenchmarkProfile>, String> {
    invoke_no_args("get_catalog").await
}

/// Loads current application settings from `benchhubd`.
pub async fn get_settings() -> Result<AppSettings, String> {
    invoke_no_args("get_settings").await
}

/// Saves sanitized application settings in `benchhubd`.
pub async fn save_settings(settings: &AppSettings) -> Result<(), String> {
    invoke_unit_with_args(
        "save_settings",
        &serde_json::json!({ "settings": settings }),
    )
    .await
}

/// Queries filtered benchmark history runs from `benchhubd`.
pub async fn get_history(
    search: &str,
    category: &str,
    gpu_mode: &str,
    status: &str,
    sort: HistorySortOrder,
) -> Result<Vec<RunResult>, String> {
    invoke_with_args(
        "get_history",
        &serde_json::json!({
            "search": search,
            "category": category,
            "gpuMode": gpu_mode,
            "status": status,
            "sort": sort,
        }),
    )
    .await
}

/// Deletes a single history run by ID.
pub async fn delete_history(id: i64) -> Result<(), String> {
    invoke_unit_with_args("delete_history", &serde_json::json!({ "id": id })).await
}

/// Clears all benchmark history records.
pub async fn clear_history() -> Result<(), String> {
    invoke_unit_no_args("clear_history").await
}

/// Updates (or clears) a run's benchmark score manually inside `benchhubd`.
pub async fn update_run_score(id: i64, score: Option<f64>) -> Result<RunResult, String> {
    invoke_with_args(
        "update_run_score",
        &serde_json::json!({ "id": id, "score": score }),
    )
    .await
}

/// Loads the saved execution log file for a history run via `benchhubd`.
pub async fn get_run_log(id: i64) -> Result<String, String> {
    invoke_with_args("get_run_log", &serde_json::json!({ "id": id })).await
}

/// Opens a verified HTTP(S) URL in the desktop default web browser.
pub async fn open_external_url(url: &str) -> Result<(), String> {
    invoke_unit_with_args("open_external_url", &serde_json::json!({ "url": url })).await
}

/// Fetches child runs aggregated under a methodology record.
pub async fn get_methodology_children(methodology_id: i64) -> Result<Vec<RunResult>, String> {
    invoke_with_args(
        "get_methodology_children",
        &serde_json::json!({ "methodologyId": methodology_id }),
    )
    .await
}

/// Computes side-by-side comparison diff inside `benchhubd`.
pub async fn compare_runs(id_a: i64, id_b: i64) -> Result<Option<RunComparisonDiff>, String> {
    invoke_with_args(
        "compare_runs",
        &serde_json::json!({ "idA": id_a, "idB": id_b }),
    )
    .await
}

/// Aggregates multiple runs into a methodology record.
pub async fn create_methodology(run_ids: &[i64]) -> Result<RunResult, String> {
    invoke_with_args(
        "create_methodology",
        &serde_json::json!({ "runIds": run_ids }),
    )
    .await
}

/// Assigns selected runs to a group name.
pub async fn group_runs(run_ids: &[i64], group_name: &str) -> Result<(), String> {
    invoke_unit_with_args(
        "group_runs",
        &serde_json::json!({ "runIds": run_ids, "groupName": group_name }),
    )
    .await
}

/// Removes selected runs from any group.
pub async fn ungroup_runs(run_ids: &[i64]) -> Result<(), String> {
    invoke_unit_with_args("ungroup_runs", &serde_json::json!({ "runIds": run_ids })).await
}

/// Downloads and installs a benchmark package via `benchhubd` after explicit EULA consent.
pub async fn install_benchmark(id: &str, version: &str, eula_accepted: bool) -> Result<(), String> {
    invoke_unit_with_args(
        "install_benchmark",
        &serde_json::json!({
            "id": id,
            "version": version,
            "eulaAccepted": eula_accepted,
        }),
    )
    .await
}

/// Uninstalls an installed benchmark version via `benchhubd`.
pub async fn uninstall_benchmark(id: &str, version: &str) -> Result<(), String> {
    invoke_unit_with_args(
        "uninstall_benchmark",
        &serde_json::json!({ "id": id, "version": version }),
    )
    .await
}

/// Executes a benchmark or stress test via `benchhubd`.
pub async fn run_benchmark(
    id: &str,
    version: &str,
    gpu_mode: GpuMode,
    custom_args: Option<String>,
    duration_secs_override: Option<u64>,
) -> Result<RunOutcome, String> {
    invoke_with_args(
        "run_benchmark",
        &serde_json::json!({
            "id": id,
            "version": version,
            "gpuMode": gpu_mode,
            "customArgs": custom_args,
            "durationSecsOverride": duration_secs_override,
        }),
    )
    .await
}

/// Signals cancellation of the currently active benchmark in `benchhubd`.
pub async fn cancel_active_run() -> Result<(), String> {
    invoke_unit_no_args("cancel_active_run").await
}

/// Clears all items from the daemon's execution queue.
pub async fn clear_queue() -> Result<Vec<QueueItem>, String> {
    invoke_no_args("clear_queue").await
}

/// Replaces the daemon's execution queue atomically with `items`.
pub async fn set_queue(items: &[QueueItem]) -> Result<Vec<QueueItem>, String> {
    invoke_with_args("set_queue", &serde_json::json!({ "items": items })).await
}

/// Triggers sequential queue execution inside `benchhubd`.
pub async fn run_queue() -> Result<(), String> {
    invoke_unit_no_args("run_queue").await
}

/// Fetches official EULA / license notice text for a benchmark package.
pub async fn get_eula_text(id: &str) -> Result<String, String> {
    invoke_with_args("get_eula_text", &serde_json::json!({ "id": id })).await
}

/// Exports all history runs into an RFC 4180 CSV file via `benchhubd`.
pub async fn export_history_csv() -> Result<String, String> {
    invoke_no_args("export_history_csv").await
}

/// Exports a comparison chart PNG for the given run IDs via `benchhubd`.
pub async fn export_comparison_png(run_ids: &[i64]) -> Result<String, String> {
    invoke_with_args(
        "export_comparison_png",
        &serde_json::json!({ "runIds": run_ids }),
    )
    .await
}

/// Queries detected hardware sensor capabilities from `benchhubd`.
pub async fn get_hardware_capabilities() -> Result<String, String> {
    invoke_no_args("get_hardware_capabilities").await
}

/// Queries active Linux power profile from `benchhubd`.
pub async fn get_power_profile() -> Result<String, String> {
    invoke_no_args("get_power_profile").await
}

/// Clears the daemon's ring terminal buffer and broadcasts `terminal-clear`.
pub async fn clear_terminal_logs() -> Result<(), String> {
    invoke_unit_no_args("clear_terminal_logs").await
}

/// Notifies the desktop shell and `benchhubd` whether the Monitor tab is active.
pub async fn set_monitor_active(active: bool) -> Result<(), String> {
    invoke_unit_with_args(
        "set_monitor_active",
        &serde_json::json!({ "active": active }),
    )
    .await
}

/// Subscribes to live `telemetry-sample` events forwarded from `benchhubd`.
pub async fn listen_telemetry<F>(callback: F) -> Result<JsValue, String>
where
    F: FnMut(TelemetryData) + 'static,
{
    listen_typed("telemetry-sample", callback).await
}

/// Subscribes to live `terminal-log` events forwarded from `benchhubd`.
pub async fn listen_terminal_log<F>(callback: F) -> Result<JsValue, String>
where
    F: FnMut(String) + 'static,
{
    listen_typed("terminal-log", callback).await
}

/// Subscribes to `terminal-clear` events emitted when a new task starts or logs are cleared.
pub async fn listen_terminal_clear<F>(mut callback: F) -> Result<JsValue, String>
where
    F: FnMut() + 'static,
{
    listen_raw("terminal-clear", move |_event_obj| callback()).await
}

/// Subscribes to full `daemon-state` snapshots emitted by `benchhubd`.
pub async fn listen_daemon_state<F>(callback: F) -> Result<JsValue, String>
where
    F: FnMut(DaemonStateSnapshot) + 'static,
{
    listen_typed("daemon-state", callback).await
}

/// Subscribes to structured `daemon-notification` events emitted by `benchhubd`.
pub async fn listen_daemon_notification<F>(callback: F) -> Result<JsValue, String>
where
    F: FnMut(DaemonNotification) + 'static,
{
    listen_typed("daemon-notification", callback).await
}
