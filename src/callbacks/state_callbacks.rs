use crate::state::*;
use crate::ui::*;
use crate::*;
use benchhub::models::GpuMode;
use slint::{ComponentHandle, Model};

pub fn register_state_callbacks(ui: &AppWindow, app_state: &AppState) {
    let db = app_state.service.db.clone();
    let engine = app_state.service.engine.clone();
    let profiles = app_state.service.profiles.clone();
    let data_dir = app_state.data_dir.clone();
    let config_path = app_state.config_path.clone();
    let history_filter_state = app_state.history_filter_state.clone();
    let telemetry_interval_tx = app_state.telemetry_interval_tx.clone();
    let terminal_limit_tx = app_state.terminal_limit_tx.clone();
    let save_stopped_runs_flag = app_state.save_stopped_runs_flag.clone();
    let selected_gpu_mode = app_state.selected_gpu_mode.clone();
    let max_test_duration_secs = app_state.max_test_duration_secs.clone();

    // Callback: Request Clear History Dialog
    let ui_weak_req_clear = ui.as_weak();
    ui.on_request_clear_history(move || {
        if let Some(ui) = ui_weak_req_clear.upgrade() {
            ui.set_dialog_title(benchhub::i18n::t("dialog_clear_history_title").into());
            ui.set_dialog_name(benchhub::i18n::t("dialog_clear_history_name").into());
            ui.set_dialog_category(benchhub::i18n::t("dialog_clear_history_category").into());
            ui.set_dialog_size(benchhub::i18n::t("dialog_clear_history_size").into());
            ui.set_dialog_desc(benchhub::i18n::t("dialog_clear_history_desc").into());
            ui.set_dialog_action_type("clear_history".into());
            ui.set_dialog_target_id("".into());
            ui.set_dialog_target_version("".into());
            ui.set_dialog_confirm_btn_text(
                benchhub::i18n::t("dialog_clear_history_confirm").into(),
            );
            ui.set_show_confirm_dialog(true);
        }
    });

    // Callback: Change Telemetry Interval
    let telemetry_interval_tx_change = telemetry_interval_tx.clone();
    let ui_weak_change_interval = ui.as_weak();
    let config_path_interval = config_path.clone();
    ui.on_change_telemetry_interval(move |val| {
        let val_str = val.as_str();
        let ms: u64 = if val_str.contains("250") {
            250
        } else if val_str.contains("1000") || val_str.contains("1 ") || val_str.contains("1s") {
            1000
        } else if val_str.contains("2000") || val_str.contains("2 ") || val_str.contains("2s") {
            2000
        } else {
            500
        };
        let int_idx = match ms {
            250 => 0,
            1000 => 2,
            2000 => 3,
            _ => 1,
        };
        let _ = telemetry_interval_tx_change.send(ms);
        if let Some(ui) = ui_weak_change_interval.upgrade() {
            ui.set_selected_telemetry_interval(val);
            ui.set_selected_telemetry_interval_index(int_idx);
            let msg = benchhub::i18n::t("status_telemetry_interval").replace("{}", &ms.to_string());
            ui.set_status_text(msg.into());
        }
        if let Err(e) = benchhub::config::update_settings(&config_path_interval, |s| {
            s.telemetry_interval_ms = ms;
        }) {
            tracing::error!("Telemetri aralığı kaydedilemedi: {}", e);
        }
    });

    // Callback: Change Terminal Buffer Limit
    let terminal_limit_tx_change = terminal_limit_tx.clone();
    let ui_weak_change_limit = ui.as_weak();
    let config_path_limit = config_path.clone();
    ui.on_change_terminal_limit(move |val| {
        let val_str = val.as_str();
        let lines: usize = if val_str.contains("80") {
            80
        } else if val_str.contains("300") {
            300
        } else if val_str.contains("500") {
            500
        } else {
            150
        };
        let lim_idx = match lines {
            80 => 0,
            300 => 2,
            500 => 3,
            _ => 1,
        };
        let _ = terminal_limit_tx_change.send(lines);
        if let Some(ui) = ui_weak_change_limit.upgrade() {
            ui.set_selected_terminal_limit(val);
            ui.set_selected_terminal_limit_index(lim_idx);
            let msg = benchhub::i18n::t("status_terminal_limit").replace("{}", &lines.to_string());
            ui.set_status_text(msg.into());
        }
        if let Err(e) = benchhub::config::update_settings(&config_path_limit, |s| {
            s.terminal_buffer_lines = lines;
        }) {
            tracing::error!("Konsol arabellek sınırı kaydedilemedi: {}", e);
        }
    });

    // Callback: Change Save Stopped Runs Option
    let save_stopped_flag_change = save_stopped_runs_flag.clone();
    let ui_weak_change_stopped = ui.as_weak();
    let config_path_stopped = config_path.clone();
    ui.on_change_save_stopped_runs(move |val| {
        let should_save = !val.as_str().contains("Kaydetme")
            && !val.as_str().contains("Do Not Save")
            && !val.as_str().contains("Ignore");
        let stop_idx = if should_save { 0 } else { 1 };
        save_stopped_flag_change.store(should_save, std::sync::atomic::Ordering::Relaxed);
        if let Some(ui) = ui_weak_change_stopped.upgrade() {
            ui.set_selected_save_stopped(val);
            ui.set_selected_save_stopped_index(stop_idx);
            if should_save {
                ui.set_status_text(benchhub::i18n::t("status_save_stopped_on").into());
            } else {
                ui.set_status_text(benchhub::i18n::t("status_save_stopped_off").into());
            }
        }
        if let Err(e) = benchhub::config::update_settings(&config_path_stopped, |s| {
            s.save_stopped_runs = should_save;
        }) {
            tracing::error!("Durdurulan test kaydetme ayarı kaydedilemedi: {}", e);
        }
    });

    // Callback: Change Maximum Test Duration Limit
    let max_test_duration_cb = max_test_duration_secs.clone();
    let ui_weak_change_duration = ui.as_weak();
    let config_path_duration = config_path.clone();
    ui.on_change_max_test_duration(move |val| {
        let secs = benchhub::config::parse_max_test_duration(val.as_str());
        let dur_idx = match secs {
            0 => 0,
            60 => 1,
            120 => 2,
            180 => 3,
            300 => 4,
            600 => 5,
            _ => 4,
        };
        max_test_duration_cb.store(secs, std::sync::atomic::Ordering::Relaxed);
        if let Some(ui) = ui_weak_change_duration.upgrade() {
            ui.set_selected_max_duration(val);
            ui.set_selected_max_duration_index(dur_idx);
            let msg = if secs > 0 {
                format!(
                    "{}: {} sn",
                    benchhub::i18n::t("status_max_duration_set"),
                    secs
                )
            } else {
                benchhub::i18n::t("status_max_duration_unlimited")
            };
            ui.set_status_text(msg.into());
        }
        if let Err(e) = benchhub::config::update_settings(&config_path_duration, |s| {
            s.max_test_duration_secs = secs;
        }) {
            tracing::error!("Maksimum test süresi ayarı kaydedilemedi: {}", e);
        }
    });

    // Callback: Change GPU Mode
    let selected_gpu_mode_cb = selected_gpu_mode.clone();
    let ui_weak_gpu = ui.as_weak();
    let config_path_gpu = config_path.clone();
    ui.on_change_gpu_mode(move |val| {
        let mode = GpuMode::from_display_str(val.as_str());
        let gpu_idx = match mode {
            GpuMode::NvidiaDgpu => 0,
            GpuMode::Integrated => 1,
            GpuMode::Auto => 2,
        };
        *selected_gpu_mode_cb
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = mode;
        if let Some(ui) = ui_weak_gpu.upgrade() {
            ui.set_selected_gpu_mode(val.clone());
            ui.set_selected_gpu_mode_index(gpu_idx);
            let msg = benchhub::i18n::t("status_gpu_mode").replace("{}", &val);
            ui.set_status_text(msg.into());
        }
        let mode_str = mode.to_display_str().to_string();
        if let Err(e) = benchhub::config::update_settings(&config_path_gpu, |s| {
            s.selected_gpu_mode = mode_str;
        }) {
            tracing::error!("GPU modu ayarı kaydedilemedi: {}", e);
        }
    });

    // Callback: Change Language
    let ui_weak_lang = ui.as_weak();
    let app_state_lang = app_state.clone();
    ui.on_change_language(move |val| {
        if let Some(ui) = ui_weak_lang.upgrade() {
            let lang_code = benchhub::i18n::normalize_lang(&val);
            benchhub::i18n::set_language(lang_code);

            let settings_snapshot = if let Ok(mut settings) = app_state_lang.settings.lock() {
                settings.language = lang_code.to_string();
                if let Err(e) =
                    benchhub::config::update_settings(&app_state_lang.config_path, |s| {
                        s.language = lang_code.to_string();
                    })
                {
                    tracing::error!("Dil ayarı kaydedilemedi: {}", e);
                }
                settings.clone()
            } else {
                return;
            };

            update_ui_language(&ui, &settings_snapshot);
            populate_initial_catalog(&ui, &app_state_lang);

            if let Ok(filter) = app_state_lang.history_filter_state.lock() {
                refresh_history_view(&ui, &app_state_lang.service.db, &filter);
            }

            ui.set_status_text(benchhub::i18n::t("status_lang_changed").into());
        }
    });

    // Callback: Confirm Dialog Action
    let ui_weak_confirm = ui.as_weak();
    let engine_confirm = engine.clone();
    let profiles_confirm = profiles.clone();
    let db_confirm = db.clone();
    let data_dir_confirm = data_dir.clone();
    let history_filter_confirm = history_filter_state.clone();

    ui.on_confirm_dialog(move |action_type, target_id, target_version| {
        if let Some(ui) = ui_weak_confirm.upgrade() {
            ui.set_show_confirm_dialog(false);
        }

        match action_type.as_str() {
            "clean_all_runners" => {
                let ui_weak = ui_weak_confirm.clone();
                let data_dir = data_dir_confirm.clone();
                tokio::spawn(async move {
                    let runners_dir = data_dir.join("runners");
                    if runners_dir.exists() {
                        if let Ok(safe_runners_dir) =
                            benchhub::utils::ensure_path_within(&data_dir, &runners_dir)
                        {
                            let _ = tokio::fs::remove_dir_all(&safe_runners_dir).await;
                            let _ = tokio::fs::create_dir_all(&safe_runners_dir).await;
                        }
                    }
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            let catalog = ui.get_catalog();
                            for i in 0..catalog.row_count() {
                                if let Some(mut item) = catalog.row_data(i) {
                                    item.installed = false;
                                    catalog.set_row_data(i, item);
                                }
                            }
                            ui.set_runners_size_text("0 B".into());
                            ui.set_status_text(
                                benchhub::i18n::t("status_all_runners_cleaned").into(),
                            );
                        }
                    });
                });
            }
            "uninstall" => {
                let profile = match profiles_confirm.iter().find(|p| p.id == target_id.as_str()) {
                    Some(p) => p.clone(),
                    None => return,
                };
                let effective = profile.for_version(target_version.as_str());

                let engine = engine_confirm.clone();
                let ui_weak = ui_weak_confirm.clone();
                let bench_id = target_id.to_string();
                let ver_str = target_version.to_string();

                tokio::spawn(async move {
                    let res = engine.uninstall_version(&bench_id, &ver_str).await;
                    let is_success = res.is_ok();

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            let catalog = ui.get_catalog();
                            for i in 0..catalog.row_count() {
                                if let Some(mut item) = catalog.row_data(i) {
                                    if item.id == bench_id.as_str() {
                                        item.running = false;
                                        if is_success && item.selected_version == ver_str.as_str() {
                                            item.installed = false;
                                        }
                                        catalog.set_row_data(i, item);
                                    }
                                }
                            }

                            if is_success {
                                let (localized_name, _, _) = get_localized_profile_info(
                                    &bench_id,
                                    &effective.name,
                                    &effective.category,
                                    effective.description.as_deref().unwrap_or_default(),
                                );
                                let display_name = if ver_str.is_empty() {
                                    localized_name
                                } else {
                                    format!("{} ({})", localized_name, ver_str)
                                };
                                ui.set_status_text(
                                    benchhub::i18n::t("status_uninstall_success")
                                        .replace("{}", &display_name)
                                        .into(),
                                );
                            } else {
                                ui.set_status_text(
                                    benchhub::i18n::t("status_uninstall_error").into(),
                                );
                            }
                        }
                    });
                });
            }
            "clear_history" => {
                let _ = db_confirm.clear_history();
                if let Some(ui) = ui_weak_confirm.upgrade() {
                    let mut filter = history_filter_confirm
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    filter.selected_ids.clear();
                    refresh_history_view(&ui, &db_confirm, &filter);
                    ui.set_status_text(benchhub::i18n::t("status_history_cleared").into());
                }
            }
            _ => {}
        }
    });

    // Callback: Delete Run Item
    let db_delete = db.clone();
    let history_filter_del = history_filter_state.clone();
    let ui_weak_delete = ui.as_weak();
    ui.on_delete_run_item(move |id| {
        let id_64 = id as i64;
        let _ = db_delete.delete_run(id_64);
        if let Some(ui) = ui_weak_delete.upgrade() {
            let mut filter = history_filter_del.lock().unwrap_or_else(|e| e.into_inner());
            filter.selected_ids.retain(|&x| x != id_64);
            refresh_history_view(&ui, &db_delete, &filter);
            ui.set_status_text(benchhub::i18n::t("status_test_record_deleted").into());
        }
    });
}
