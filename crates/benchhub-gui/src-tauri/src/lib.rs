//! BenchHub Desktop Shell
//!
//! Thin Tauri v2 application shell hosting the Leptos CSR frontend and
//! communicating with `benchhubd` over a local Unix domain socket.

pub mod commands;
pub mod daemon_client;
pub mod state;
pub mod theme_detect;

use daemon_client::{resolve_socket_path, DaemonClient};
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("BenchHub desktop shell panicked: {info}");
    }));

    let socket_path = resolve_socket_path();
    let daemon = DaemonClient::new(socket_path);
    let app_state = state::AppState::new(Arc::clone(&daemon));
    let exit_daemon = Arc::clone(&daemon);

    tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let daemon_client = Arc::clone(&daemon);
            tauri::async_runtime::spawn(async move {
                daemon_client.run_event_forwarder(app_handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_system_locale,
            commands::get_system_theme_info,
            commands::get_daemon_snapshot,
            commands::get_catalog,
            commands::get_settings,
            commands::save_settings,
            commands::get_history,
            commands::delete_history,
            commands::clear_history,
            commands::update_run_score,
            commands::get_run_log,
            commands::open_external_url,
            commands::compare_runs,
            commands::create_methodology,
            commands::get_methodology_children,
            commands::group_runs,
            commands::ungroup_runs,
            commands::install_benchmark,
            commands::uninstall_benchmark,
            commands::run_benchmark,
            commands::cancel_active_run,
            commands::clear_queue,
            commands::set_queue,
            commands::run_queue,
            commands::get_eula_text,
            commands::export_history_csv,
            commands::export_comparison_png,
            commands::get_hardware_capabilities,
            commands::get_power_profile,
            commands::clear_terminal_logs,
            commands::set_monitor_active,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(exit_daemon.shutdown_daemon());
            }
        });
}
