use crate::state::*;
use crate::ui::*;
use crate::*;
use benchhub::models::{GpuMode, RunResult};
use slint::{ComponentHandle, Model};
use std::time::Instant;

pub fn register_engine_callbacks(ui: &AppWindow, app_state: &AppState) {
    let engine = app_state.service.engine.clone();
    let profiles = app_state.service.profiles.clone();

    // Callback: Request Install Dialog
    let profiles_req_inst = profiles.clone();
    let ui_weak_req_inst = ui.as_weak();
    ui.on_request_install(move |id, version| {
        if let Some(p) = profiles_req_inst.iter().find(|p| p.id == id.as_str()) {
            if let Some(ui) = ui_weak_req_inst.upgrade() {
                let effective = p.for_version(version.as_str());
                let (localized_name, localized_cat, localized_desc) = get_localized_profile_info(
                    &p.id,
                    &effective.name,
                    &effective.category,
                    effective.description.as_deref().unwrap_or_default(),
                );
                let size_str = effective.display_download_size();
                let display_title = if version.is_empty() {
                    localized_name.clone()
                } else {
                    format!("{} {}", localized_name, version)
                };
                ui.set_install_dialog_title(display_title.into());
                ui.set_install_dialog_name(localized_name.into());
                ui.set_install_dialog_category(localized_cat.into());
                ui.set_install_dialog_size(size_str.into());
                ui.set_install_dialog_desc(localized_desc.into());
                ui.set_install_target_id(id);
                ui.set_install_target_version(version);
                ui.set_install_dialog_log("".into());
                ui.set_install_dialog_status(benchhub::i18n::t("ready").into());
                let eula_text = benchhub::eula::get_eula_for_benchmark(&p.id);
                ui.set_install_dialog_eula(eula_text.into());
                ui.set_install_eula_accepted(false);
                ui.set_install_liability_accepted(false);
                ui.set_show_install_eula(false);
                ui.set_is_installing(false);
                ui.set_install_finished(false);
                ui.set_install_success(false);
                ui.set_show_install_dialog(true);
            }
        }
    });

    // Callback: Start Install Dialog Action
    let profiles_start_inst = profiles.clone();
    let engine_start_inst = engine.clone();
    let ui_weak_start_inst = ui.as_weak();
    ui.on_start_install(move |id, version| {
        let profile = match profiles_start_inst.iter().find(|p| p.id == id.as_str()) {
            Some(p) => p.clone(),
            None => return,
        };
        let effective = profile.for_version(version.as_str());
        let (localized_name, _, _) = get_localized_profile_info(
            &profile.id,
            &effective.name,
            &effective.category,
            effective.description.as_deref().unwrap_or_default(),
        );

        if let Some(ui) = ui_weak_start_inst.upgrade() {
            if !ui.get_install_eula_accepted() || !ui.get_install_liability_accepted() {
                tracing::warn!("Installation rejected: EULA or Liability not accepted for benchmark '{}'", id);
                return;
            }
            let catalog = ui.get_catalog();
            for i in 0..catalog.row_count() {
                if let Some(mut item) = catalog.row_data(i) {
                    if item.id == id {
                        item.running = true;
                        catalog.set_row_data(i, item);
                    }
                }
            }
            ui.set_is_installing(true);
            ui.set_install_finished(false);
            ui.set_install_success(false);
            ui.set_install_dialog_status(benchhub::i18n::t("loading").into());
            let initial_log = format!(
                "[BenchHub] {}\n",
                benchhub::i18n::t("status_installing").replace("{}", &localized_name)
            );
            ui.set_install_dialog_log(initial_log.clone().into());
            ui.set_status_text(
                benchhub::i18n::t("status_installing")
                    .replace("{}", &localized_name)
                    .into(),
            );
        }

        let engine = engine_start_inst.clone();
        let ui_weak = ui_weak_start_inst.clone();
        let bench_id = id.to_string();
        let ver_str = version.to_string();
        let name_str = localized_name.clone();

        tokio::spawn(async move {
            let (inst_tx, mut inst_rx) = tokio::sync::mpsc::channel::<String>(512);
            let ui_weak_stream = ui_weak.clone();
            let initial_msg = format!("[BenchHub] Kurulum başlatılıyor: {}\n", name_str);

            tokio::spawn(async move {
                let mut log_ring_buf = benchhub::logging::TerminalRingBuffer::new(200);
                log_ring_buf.push_str(&initial_msg);
                let mut dirty = true;
                let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
                loop {
                    tokio::select! {
                        maybe_line = inst_rx.recv() => {
                            match maybe_line {
                                Some(line) => {
                                    log_ring_buf.push_str(&line);
                                    dirty = true;
                                }
                                None => {
                                    let text = log_ring_buf.to_string();
                                    let ui_w = ui_weak_stream.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(ui) = ui_w.upgrade() {
                                            ui.set_install_dialog_log(text.into());
                                        }
                                    });
                                    break;
                                }
                            }
                        }
                        _ = interval.tick() => {
                            if dirty {
                                dirty = false;
                                let text = log_ring_buf.to_string();
                                let ui_w = ui_weak_stream.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_w.upgrade() {
                                        ui.set_install_dialog_log(text.into());
                                    }
                                });
                            }
                        }
                    }
                }
            });

            let result = engine
                .install_version(&profile, &ver_str, move |line| {
                    let _ = inst_tx.try_send(line);
                })
                .await;

            let is_success = result.is_ok();
            let err_msg = result.err().map(|e| e.to_string());

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    let catalog = ui.get_catalog();
                    for i in 0..catalog.row_count() {
                        if let Some(mut item) = catalog.row_data(i) {
                            if item.id == bench_id.as_str() {
                                item.running = false;
                                if is_success && item.selected_version == ver_str.as_str() {
                                    item.installed = true;
                                }
                                catalog.set_row_data(i, item);
                            }
                        }
                    }

                    ui.set_is_installing(false);
                    ui.set_install_finished(true);
                    ui.set_install_success(is_success);

                    if is_success {
                        ui.set_install_dialog_status(
                            benchhub::i18n::t("status_install_success").into(),
                        );
                        ui.set_status_text(benchhub::i18n::t("status_install_success").into());
                    } else if let Some(ref err) = err_msg {
                        let err_str = benchhub::i18n::t("status_install_error").replace("{}", err);
                        ui.set_install_dialog_status(err_str.clone().into());
                        ui.set_status_text(err_str.into());
                    }
                }
            });
        });
    });

    // Callback: Close Install Dialog
    let ui_weak_close_inst = ui.as_weak();
    ui.on_close_install_dialog(move || {
        if let Some(ui) = ui_weak_close_inst.upgrade() {
            ui.set_show_install_dialog(false);
            ui.set_install_eula_accepted(false);
            ui.set_install_liability_accepted(false);
            ui.set_show_install_eula(false);
        }
    });

    // Callback: Request Uninstall Dialog
    let profiles_req_uninst = profiles.clone();
    let ui_weak_req_uninst = ui.as_weak();
    ui.on_request_uninstall(move |id, version| {
        if let Some(p) = profiles_req_uninst.iter().find(|p| p.id == id.as_str()) {
            if let Some(ui) = ui_weak_req_uninst.upgrade() {
                let effective = p.for_version(version.as_str());
                let (localized_name, localized_cat, _) = get_localized_profile_info(
                    &p.id,
                    &effective.name,
                    &effective.category,
                    effective.description.as_deref().unwrap_or_default(),
                );
                ui.set_dialog_title(
                    format!("{}: {}", benchhub::i18n::t("uninstall"), version).into(),
                );
                ui.set_dialog_name(localized_name.into());
                ui.set_dialog_category(localized_cat.into());
                ui.set_dialog_size(format!("runners/{}/{}", p.id, version).into());
                ui.set_dialog_desc(benchhub::i18n::t("dialog_uninstall_desc").into());
                ui.set_dialog_action_type("uninstall".into());
                ui.set_dialog_target_id(id);
                ui.set_dialog_target_version(version);
                ui.set_dialog_confirm_btn_text(
                    benchhub::i18n::t("dialog_uninstall_confirm").into(),
                );
                ui.set_show_confirm_dialog(true);
            }
        }
    });

    // Callback: Request Clean All Runners Dialog
    let ui_weak_clean_runners = ui.as_weak();
    ui.on_request_clean_all_runners(move || {
        if let Some(ui) = ui_weak_clean_runners.upgrade() {
            ui.set_dialog_title(benchhub::i18n::t("dialog_clean_runners_title").into());
            ui.set_dialog_name(benchhub::i18n::t("dialog_clean_runners_name").into());
            ui.set_dialog_category(benchhub::i18n::t("dialog_clean_runners_category").into());
            ui.set_dialog_size(ui.get_runners_size_text());
            ui.set_dialog_desc(benchhub::i18n::t("dialog_clean_runners_desc").into());
            ui.set_dialog_action_type("clean_all_runners".into());
            ui.set_dialog_target_id("".into());
            ui.set_dialog_target_version("".into());
            ui.set_dialog_confirm_btn_text(
                benchhub::i18n::t("dialog_clean_runners_confirm").into(),
            );
            ui.set_show_confirm_dialog(true);
        }
    });

    // Callback: Stop Benchmark
    let active_cancel_tx_stop = app_state.active_cancel_tx.clone();
    let is_queue_running_stop = app_state.is_queue_running.clone();
    let ui_weak_stop = ui.as_weak();
    let term_tx_stop = app_state.terminal_tx.clone();
    ui.on_stop_benchmark(move || {
        is_queue_running_stop.store(false, std::sync::atomic::Ordering::Relaxed);
        let active_cancel_tx_stop = active_cancel_tx_stop.clone();
        let term_tx_stop = term_tx_stop.clone();
        let ui_weak_stop = ui_weak_stop.clone();
        tokio::spawn(async move {
            let opt_tx = active_cancel_tx_stop.lock().await.take();
            if let Some(tx) = opt_tx {
                let _ = tx.send(true);
                let _ = term_tx_stop.try_send(TerminalMsg::Append(
                    "\n[BenchHub] ⏹ Test durdurma sinyali gönderildi...\n".to_string(),
                ));
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak_stop.upgrade() {
                        ui.set_is_queue_running(false);
                        ui.set_queue_status_text("".into());
                        ui.set_status_text(benchhub::i18n::t("status_stopping_test").into());
                    }
                });
            }
        });
    });

    // Callback: Stop All Benchmarks (Queue Stop)
    let active_cancel_tx_stop_all = app_state.active_cancel_tx.clone();
    let is_queue_running_stop_all = app_state.is_queue_running.clone();
    let ui_weak_stop_all = ui.as_weak();
    let term_tx_stop_all = app_state.terminal_tx.clone();
    ui.on_stop_all_benchmarks(move || {
        is_queue_running_stop_all.store(false, std::sync::atomic::Ordering::Relaxed);
        let active_cancel_tx_stop_all = active_cancel_tx_stop_all.clone();
        let term_tx_stop_all = term_tx_stop_all.clone();
        let ui_weak_stop_all = ui_weak_stop_all.clone();
        tokio::spawn(async move {
            let opt_tx = active_cancel_tx_stop_all.lock().await.take();
            if let Some(tx) = opt_tx {
                let _ = tx.send(true);
                let _ = term_tx_stop_all.try_send(TerminalMsg::Append(
                    "\n[BenchHub] ⏹ Tüm test kuyruğu durduruluyor...\n".to_string(),
                ));
            }
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak_stop_all.upgrade() {
                    ui.set_is_queue_running(false);
                    ui.set_queue_status_text("".into());
                    ui.set_status_text(benchhub::i18n::t("status_stopping_test").into());
                }
            });
        });
    });

    // Callback: Run With Current Settings
    let ui_weak_run_with_cfg = ui.as_weak();
    ui.on_run_with_current_settings(move || {
        if let Some(ui) = ui_weak_run_with_cfg.upgrade() {
            let id = ui.get_bench_config_id();
            let version = ui.get_bench_config_version();
            ui.invoke_save_current_settings();
            ui.invoke_run_benchmark(id, version);
        }
    });

    // Callback: Run Benchmark
    let ui_weak_run = ui.as_weak();
    let app_state_run = app_state.clone();
    let selected_gpu_mode_run = app_state.selected_gpu_mode.clone();

    ui.on_run_benchmark(move |id, version| {
        let profile = match app_state_run
            .service
            .profiles
            .iter()
            .find(|p| p.id == id.as_str())
        {
            Some(p) => p.clone(),
            None => return,
        };
        let current_gpu_mode = *selected_gpu_mode_run
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let ui_weak = ui_weak_run.clone();
        let bench_id = id.to_string();
        let ver_str = version.to_string();
        let app_state_task = app_state_run.clone();

        tokio::spawn(async move {
            execute_single_benchmark_flow(
                bench_id,
                ver_str,
                &profile,
                current_gpu_mode,
                ui_weak,
                &app_state_task,
            )
            .await;
        });
    });

    // Callback: Run All Benchmarks (Sequential Queue)
    let ui_weak_run_all = ui.as_weak();
    let is_queue_running_run_all = app_state.is_queue_running.clone();
    let app_state_run_all = app_state.clone();
    let selected_gpu_mode_run_all = app_state.selected_gpu_mode.clone();

    ui.on_run_all_benchmarks(move || {
        let ui = match ui_weak_run_all.upgrade() {
            Some(u) => u,
            None => return,
        };

        if is_queue_running_run_all.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // Collect installed benchmarks with their currently selected version from catalog
        let mut queue: Vec<(String, String, benchhub::models::BenchmarkProfile)> = Vec::new();
        let catalog = ui.get_catalog();
        for i in 0..catalog.row_count() {
            if let Some(item) = catalog.row_data(i) {
                let id_str = item.id.to_string();
                let ver_str = item.selected_version.to_string();
                if app_state_run_all
                    .service
                    .engine
                    .is_version_installed(&id_str, &ver_str)
                {
                    if let Some(p) = app_state_run_all
                        .service
                        .profiles
                        .iter()
                        .find(|p| p.id == id_str)
                    {
                        queue.push((id_str, ver_str, p.clone()));
                    }
                }
            }
        }

        if queue.is_empty() {
            ui.set_status_text(benchhub::i18n::t("queue_no_installed").into());
            return;
        }

        let total_count = queue.len();
        is_queue_running_run_all.store(true, std::sync::atomic::Ordering::Relaxed);
        ui.set_is_queue_running(true);
        ui.set_queue_total_count(total_count as i32);
        ui.set_queue_current_index(0);

        let ui_weak_task = ui_weak_run_all.clone();
        let is_queue_task = is_queue_running_run_all.clone();
        let app_state_task = app_state_run_all.clone();
        let selected_gpu_mode_task = selected_gpu_mode_run_all.clone();

        tokio::spawn(async move {
            for (idx, (bench_id, ver_str, profile)) in queue.into_iter().enumerate() {
                if !is_queue_task.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                let effective = profile.for_version(&ver_str);
                let (loc_name, _, _) = get_localized_profile_info(
                    &profile.id,
                    &effective.name,
                    &effective.category,
                    effective.description.as_deref().unwrap_or_default(),
                );

                let cur_idx = idx as i32;
                let tot = total_count as i32;
                let q_msg = benchhub::i18n::t_fmt(
                    "queue_running_fmt",
                    &[&(cur_idx + 1).to_string(), &tot.to_string(), &loc_name],
                );

                let _ = slint::invoke_from_event_loop({
                    let ui_weak = ui_weak_task.clone();
                    let q_msg = q_msg.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_queue_current_index(cur_idx);
                            ui.set_queue_total_count(tot);
                            ui.set_queue_status_text(q_msg.into());
                        }
                    }
                });

                let current_gpu_mode = *selected_gpu_mode_task
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());

                let can_continue = execute_single_benchmark_flow(
                    bench_id,
                    ver_str,
                    &profile,
                    current_gpu_mode,
                    ui_weak_task.clone(),
                    &app_state_task,
                )
                .await;

                if !can_continue || !is_queue_task.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                // Brief 2s pause between tests for hardware cooldown
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            is_queue_task.store(false, std::sync::atomic::Ordering::Relaxed);
            let _ = slint::invoke_from_event_loop({
                let ui_weak = ui_weak_task.clone();
                move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_is_queue_running(false);
                        ui.set_queue_status_text("".into());
                        ui.set_status_text(benchhub::i18n::t("queue_completed").into());
                    }
                }
            });
        });
    });
}

// Helper: Execute a single benchmark run flow
async fn execute_single_benchmark_flow(
    bench_id: String,
    ver_str: String,
    profile: &benchhub::models::BenchmarkProfile,
    current_gpu_mode: GpuMode,
    ui_weak: slint::Weak<AppWindow>,
    app_state: &AppState,
) -> bool {
    let effective = profile.for_version(&ver_str);
    let (localized_name, _, _) = get_localized_profile_info(
        &profile.id,
        &effective.name,
        &effective.category,
        effective.description.as_deref().unwrap_or_default(),
    );

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *app_state.active_cancel_tx.lock().await = Some(cancel_tx);

    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        let bench_id = bench_id.clone();
        let localized_name = localized_name.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                let catalog = ui.get_catalog();
                for i in 0..catalog.row_count() {
                    if let Some(mut item) = catalog.row_data(i) {
                        if item.id == bench_id.as_str() {
                            item.running = true;
                            catalog.set_row_data(i, item);
                        }
                    }
                }
                ui.set_is_benchmark_running(true);
                ui.set_active_benchmark_name(localized_name.clone().into());
                let status_msg = benchhub::i18n::t("status_running").replace("{}", &localized_name);
                ui.set_status_text(status_msg.into());
                ui.set_active_tab(1); // Switch to monitor tab
            }
        }
    });

    // Fresh console for the new test run
    let _ = app_state.terminal_tx.try_send(TerminalMsg::Clear);
    let _ = app_state.terminal_tx.try_send(TerminalMsg::Append(format!(
        "=== [BenchHub] Test: {} ===\n",
        localized_name
    )));

    // Reset telemetry samples
    app_state.current_samples.lock().await.clear();
    app_state
        .is_recording_samples
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let (custom_run_args, effective_preset_str, custom_timeout) = {
        let cfgs = app_state
            .custom_configs
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cfg) = cfgs.get(bench_id.as_str()) {
            (
                cfg.effective_run_args.clone(),
                cfg.custom_preset_name
                    .clone()
                    .unwrap_or_else(|| ver_str.clone()),
                cfg.timeout_secs,
            )
        } else {
            (None, ver_str.clone(), None)
        }
    };

    let start_ts = chrono::Utc::now().timestamp();
    let timer = Instant::now();

    let ver_suffix = if ver_str.is_empty() {
        String::new()
    } else {
        format!("_{}", ver_str)
    };
    let log_file_name = format!("{}_{}{}.log", start_ts, profile.id, ver_suffix);
    let log_file_path = app_state.logs_dir.join(&log_file_name);
    let log_path_str = log_file_path.to_string_lossy().to_string();

    let sys_info = benchhub::telemetry::get_system_info_summary();
    let power_prof = benchhub::telemetry::get_power_profile();

    let bench_id_with_ver = if ver_str.is_empty() {
        profile.id.clone()
    } else {
        format!("{}-{}", profile.id, ver_str)
    };

    let mut run_res = RunResult {
        id: 0,
        benchmark_id: bench_id_with_ver.clone(),
        category: profile.category.clone(),
        preset_or_version: effective_preset_str.clone(),
        gpu_mode: current_gpu_mode.to_display_str().to_string(),
        score: None,
        status: "Çalışıyor".to_string(),
        timestamp: start_ts,
        duration_secs: 0.0,
        avg_cpu_usage: 0.0,
        peak_cpu_usage: 0.0,
        avg_cpu_temp: 0.0,
        peak_cpu_temp: 0.0,
        avg_cpu_freq_mhz: 0,
        peak_cpu_freq_mhz: 0,
        avg_gpu_usage: 0.0,
        peak_gpu_usage: 0.0,
        avg_gpu_temp: 0.0,
        peak_gpu_temp: 0.0,
        avg_gpu_freq_mhz: 0,
        peak_gpu_freq_mhz: 0,
        avg_vram_freq_mhz: 0,
        peak_vram_freq_mhz: 0,
        avg_gpu_power_w: 0.0,
        peak_gpu_power_w: 0.0,
        avg_power_w: 0.0,
        peak_power_w: 0.0,
        avg_ac_power_w: 0.0,
        peak_ac_power_w: 0.0,
        avg_ram_gb: 0.0,
        peak_ram_gb: 0.0,
        avg_vram_gb: 0.0,
        peak_vram_gb: 0.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: sys_info.clone(),
        power_profile: power_prof.clone(),
        log_path: log_path_str.clone(),
    };

    if let Ok(inserted_id) = app_state.service.db.insert_run(&run_res) {
        run_res.id = inserted_id;
        app_state
            .active_run_id
            .store(inserted_id, std::sync::atomic::Ordering::Relaxed);
    }

    let global_dur_limit = app_state
        .max_test_duration_secs
        .load(std::sync::atomic::Ordering::Relaxed);
    let effective_timeout = match custom_timeout {
        Some(0) => None,
        Some(secs) => Some(std::time::Duration::from_secs(secs)),
        None => {
            if global_dur_limit > 0 {
                Some(std::time::Duration::from_secs(global_dur_limit))
            } else {
                None
            }
        }
    };

    let term_tx_cb = app_state.terminal_tx.clone();
    let run_fut = app_state.service.engine.run_version_custom(
        profile,
        &ver_str,
        custom_run_args.as_deref(),
        effective_timeout.map(|d| d.as_secs()),
        cancel_rx,
        Some(log_file_path.clone()),
        current_gpu_mode,
        move |line| {
            let _ = term_tx_cb.try_send(TerminalMsg::Append(line));
        },
    );

    let (score, status) = if let Some(timeout_dur) = effective_timeout {
        match tokio::time::timeout(timeout_dur, run_fut).await {
            Ok(Ok(output)) => {
                let s = output.status.clone();
                let sc = output.score;
                (sc, s)
            }
            Ok(Err(e)) => {
                let _ = app_state
                    .terminal_tx
                    .try_send(TerminalMsg::Append(format!("\n[BenchHub] Hata: {}\n", e)));
                (None, "Hata".to_string())
            }
            Err(_) => {
                let timeout_msg = format!(
                    "\n[BenchHub] ⏱️ Test belirlenen maksimum süreyi ({} sn) aştı ve zaman aşımı korumasıyla durduruldu.\n",
                    timeout_dur.as_secs()
                );
                let _ = app_state
                    .terminal_tx
                    .try_send(TerminalMsg::Append(timeout_msg));
                let opt_tx = app_state.active_cancel_tx.lock().await.take();
                if let Some(tx) = opt_tx {
                    let _ = tx.send(true);
                }
                (None, benchhub::i18n::t("status_timeout"))
            }
        }
    } else {
        match run_fut.await {
            Ok(output) => {
                let s = output.status.clone();
                let sc = output.score;
                (sc, s)
            }
            Err(e) => {
                let _ = app_state
                    .terminal_tx
                    .try_send(TerminalMsg::Append(format!("\n[BenchHub] Hata: {}\n", e)));
                (None, "Hata".to_string())
            }
        }
    };

    let duration_secs = timer.elapsed().as_secs_f32();
    app_state
        .is_recording_samples
        .store(false, std::sync::atomic::Ordering::Relaxed);
    app_state
        .active_run_id
        .store(0, std::sync::atomic::Ordering::Relaxed);

    let captured = {
        let s = app_state.current_samples.lock().await;
        s.clone()
    };
    let summary = benchhub::telemetry::calculate_telemetry_summary(&captured);

    run_res.score = score;
    run_res.status = status.clone();
    run_res.duration_secs = duration_secs;
    run_res.avg_cpu_usage = summary.avg_cpu_usage;
    run_res.peak_cpu_usage = summary.peak_cpu_usage;
    run_res.avg_cpu_temp = summary.avg_cpu_temp;
    run_res.peak_cpu_temp = summary.peak_cpu_temp;
    run_res.avg_cpu_freq_mhz = summary.avg_cpu_freq_mhz;
    run_res.peak_cpu_freq_mhz = summary.peak_cpu_freq_mhz;
    run_res.avg_gpu_usage = summary.avg_gpu_usage;
    run_res.peak_gpu_usage = summary.peak_gpu_usage;
    run_res.avg_gpu_temp = summary.avg_gpu_temp;
    run_res.peak_gpu_temp = summary.peak_gpu_temp;
    run_res.avg_gpu_freq_mhz = summary.avg_gpu_freq_mhz;
    run_res.peak_gpu_freq_mhz = summary.peak_gpu_freq_mhz;
    run_res.avg_vram_freq_mhz = summary.avg_vram_freq_mhz;
    run_res.peak_vram_freq_mhz = summary.peak_vram_freq_mhz;
    run_res.avg_gpu_power_w = summary.avg_gpu_power_w;
    run_res.peak_gpu_power_w = summary.peak_gpu_power_w;
    run_res.avg_power_w = summary.avg_power_w;
    run_res.peak_power_w = summary.peak_power_w;
    run_res.avg_ac_power_w = summary.avg_ac_power_w;
    run_res.peak_ac_power_w = summary.peak_ac_power_w;
    run_res.avg_ram_gb = summary.avg_ram_gb;
    run_res.peak_ram_gb = summary.peak_ram_gb;
    run_res.avg_vram_gb = summary.avg_vram_gb;
    run_res.peak_vram_gb = summary.peak_vram_gb;
    run_res.cpu_throttling = summary.cpu_throttling;
    run_res.gpu_throttling = summary.gpu_throttling;
    run_res.system_info_summary = sys_info;

    let should_record = status != "Durduruldu"
        || app_state
            .save_stopped_runs_flag
            .load(std::sync::atomic::Ordering::Relaxed);

    if should_record {
        if run_res.id > 0 {
            let _ = app_state.service.db.update_run(&run_res);
        } else {
            let _ = app_state.service.db.insert_run(&run_res);
        }
    } else if run_res.id > 0 {
        let _ = app_state.service.db.delete_run(run_res.id);
    }

    let db_refresh = app_state.service.db.clone();
    let filter_refresh = app_state.history_filter_state.clone();

    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        let bench_id = bench_id.clone();
        let status = status.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                let catalog = ui.get_catalog();
                for i in 0..catalog.row_count() {
                    if let Some(mut item) = catalog.row_data(i) {
                        if item.id == bench_id.as_str() {
                            item.running = false;
                            catalog.set_row_data(i, item);
                        }
                    }
                }
                ui.set_is_benchmark_running(false);
                ui.set_active_benchmark_name("".into());

                // Refresh history table with current filter
                let filter = filter_refresh.lock().unwrap_or_else(|e| e.into_inner());
                refresh_history_view(&ui, &db_refresh, &filter);

                if status == "Durduruldu" {
                    if should_record {
                        ui.set_status_text(
                            benchhub::i18n::t("status_test_stopped_recorded").into(),
                        );
                    } else {
                        ui.set_status_text(
                            benchhub::i18n::t("status_test_stopped_not_recorded").into(),
                        );
                    }
                } else if let Some(s) = score {
                    ui.set_status_text(
                        benchhub::i18n::t("status_test_completed_score")
                            .replace("{}", &s.to_string())
                            .into(),
                    );
                } else {
                    let localized_status = benchhub::i18n::localize_status(&status);
                    ui.set_status_text(
                        benchhub::i18n::t("status_test_completed_no_score")
                            .replace("{}", &localized_status)
                            .into(),
                    );
                }
            }
        }
    });

    let was_stopped = status == "Durduruldu" || status == "İptal Edildi";
    !was_stopped
}
