use crate::commands::copy_to_clipboard;
use crate::state::{AppState, TerminalMsg};
use crate::AppWindow;
use chrono::Utc;
use slint::ComponentHandle;
use std::process::Command;

pub fn register_log_callbacks(ui: &AppWindow, app_state: &AppState) {
    let terminal_tx = app_state.terminal_tx.clone();
    let logs_dir = app_state.logs_dir.clone();
    let history_filter_state = app_state.history_filter_state.clone();
    let service = app_state.service.clone();
    let data_dir = app_state.data_dir.clone();

    // Callback: Clear Terminal
    let term_tx_clear = terminal_tx.clone();
    ui.on_clear_terminal(move || {
        let _ = term_tx_clear.try_send(TerminalMsg::Clear);
    });

    // Callback: Open Logs Dir
    let logs_dir_open = logs_dir.clone();
    ui.on_open_logs_dir(move || {
        let _ = Command::new("xdg-open").arg(&logs_dir_open).spawn();
    });

    // Callback: Export History to CSV
    let history_filter_csv = history_filter_state.clone();
    let logs_dir_csv = logs_dir.clone();
    let ui_weak_csv = ui.as_weak();
    let service_csv = service.clone();
    ui.on_export_history_csv(move || {
        let filter = history_filter_csv.lock().unwrap_or_else(|e| e.into_inner());
        let runs = match service_csv.get_history(
            &filter.search,
            &filter.category,
            &filter.gpu_mode,
            &filter.status,
            filter.sort_order,
        ) {
            Ok(r) => r,
            Err(e) => {
                if let Some(ui) = ui_weak_csv.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_error")
                            .replace("{}", &e.to_string())
                            .into(),
                    );
                }
                return;
            }
        };

        let ts = Utc::now().timestamp();
        let csv_filename = format!("benchhub_history_{}.csv", ts);
        let csv_path = logs_dir_csv.join(&csv_filename);

        match benchhub::export::export_history_csv(&runs, &csv_path) {
            Ok(_) => {
                if let Some(ui) = ui_weak_csv.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_csv_export_success")
                            .replace("{}", &csv_filename)
                            .into(),
                    );
                }
            }
            Err(e) => {
                if let Some(ui) = ui_weak_csv.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_csv_export_error")
                            .replace("{}", &e.to_string())
                            .into(),
                    );
                }
            }
        }
    });

    // Callback: Export Comparison Image
    let ui_weak_svg = ui.as_weak();
    let history_filter_svg = history_filter_state.clone();
    let service_svg = service.clone();
    ui.on_export_comparison_image(move || {
        let filter = history_filter_svg.lock().unwrap_or_else(|e| e.into_inner());
        let runs = match service_svg.get_history(
            &filter.search,
            &filter.category,
            &filter.gpu_mode,
            &filter.status,
            filter.sort_order,
        ) {
            Ok(r) => r,
            Err(e) => {
                if let Some(ui) = ui_weak_svg.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_error")
                            .replace("{}", &e.to_string())
                            .into(),
                    );
                }
                return;
            }
        };

        let selected: Vec<_> = runs
            .into_iter()
            .filter(|r| filter.selected_ids.contains(&r.id))
            .collect();

        if selected.is_empty() {
            if let Some(ui) = ui_weak_svg.upgrade() {
                ui.set_status_text(benchhub::i18n::t("status_error_no_selection").into());
            }
            return;
        }

        match benchhub::export::export_comparison_png(&selected) {
            Ok(path) => {
                if let Some(ui) = ui_weak_svg.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_svg_export_success")
                            .replace("{}", path.to_string_lossy().as_ref())
                            .into(),
                    );
                }
            }
            Err(e) => {
                if let Some(ui) = ui_weak_svg.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_svg_export_error")
                            .replace("{}", &e.to_string())
                            .into(),
                    );
                }
            }
        }
    });

    // Callback: Load Run Details Log
    let data_dir_load_log = data_dir.clone();
    let ui_weak_load_log = ui.as_weak();
    ui.on_load_details_log(move || {
        if let Some(ui) = ui_weak_load_log.upgrade() {
            let log_path_raw = ui.get_details_log_path().to_string();
            let data_dir = data_dir_load_log.clone();
            let ui_weak = ui_weak_load_log.clone();
            tokio::spawn(async move {
                let resolved_path = if log_path_raw.is_empty() {
                    None
                } else {
                    let logs_base = data_dir.join("logs");
                    let p = std::path::PathBuf::from(&log_path_raw);

                    let mut found = None;
                    if let Ok(safe_path) = benchhub::utils::ensure_path_within(&logs_base, &p) {
                        if safe_path.exists() {
                            found = Some(safe_path);
                        }
                    }

                    if found.is_none() {
                        if let Some(filename) = p.file_name() {
                            let candidate = logs_base.join(filename);
                            if let Ok(safe_path) =
                                benchhub::utils::ensure_path_within(&logs_base, &candidate)
                            {
                                if safe_path.exists() {
                                    found = Some(safe_path);
                                }
                            }
                        }
                    }
                    found
                };

                let log_content = match resolved_path {
                    None => benchhub::i18n::t("no_log_file_found"),
                    Some(path) => match tokio::fs::read(&path).await {
                        Ok(bytes) => {
                            if bytes.is_empty() {
                                benchhub::i18n::t("log_file_empty")
                            } else {
                                let max_len = 65536;
                                if bytes.len() > max_len {
                                    let start = bytes.len() - max_len;
                                    String::from_utf8_lossy(&bytes[start..]).to_string()
                                } else {
                                    String::from_utf8_lossy(&bytes).to_string()
                                }
                            }
                        }
                        Err(e) => format!(
                            "{} {}\n{}: {}",
                            benchhub::i18n::t("log_file_read_error"),
                            e,
                            benchhub::i18n::t("file"),
                            path.display()
                        ),
                    },
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_details_log_content(log_content.into());
                    }
                });
            });
        }
    });

    // Callback: Copy Log to Clipboard
    ui.on_copy_log_to_clipboard(move |text| {
        let text_to_copy = text.to_string();
        tokio::spawn(async move {
            if let Err(e) = copy_to_clipboard(&text_to_copy) {
                tracing::error!("Failed to copy to clipboard: {}", e);
            }
        });
    });
}
