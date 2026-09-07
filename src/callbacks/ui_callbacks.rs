use crate::state::*;
use crate::ui::*;
use crate::*;
use benchhub::models::HistorySortOrder;
use chrono::DateTime;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::process::Command;
use std::rc::Rc;

pub fn register_ui_callbacks(ui: &AppWindow, app_state: &AppState) {
    let db = app_state.service.db.clone();
    let engine = app_state.service.engine.clone();
    let profiles = app_state.service.profiles.clone();
    let data_dir = app_state.data_dir.clone();
    let history_filter_state = app_state.history_filter_state.clone();
    let custom_configs = app_state.custom_configs.clone();

    // Callback: Select Version
    let profiles_sel_ver = profiles.clone();
    let engine_sel_ver = engine.clone();
    let ui_weak_sel_ver = ui.as_weak();
    ui.on_select_version(move |id, version| {
        if let Some(ui) = ui_weak_sel_ver.upgrade() {
            let catalog = ui.get_catalog();
            for i in 0..catalog.row_count() {
                if let Some(mut item) = catalog.row_data(i) {
                    if item.id == id {
                        item.selected_version = version.clone();
                        item.display_version = version.clone();
                        if let Some(p) = profiles_sel_ver.iter().find(|p| p.id == id.as_str()) {
                            let idx = p
                                .versions
                                .iter()
                                .position(|v| v.version == version.as_str())
                                .unwrap_or(0) as i32;
                            item.selected_version_index = idx;
                            let effective = p.for_version(version.as_str());
                            item.download_size = effective.display_download_size().into();
                        }
                        item.installed =
                            engine_sel_ver.is_version_installed(id.as_str(), version.as_str());
                        catalog.set_row_data(i, item);
                    }
                }
            }
        }
    });

    // Callback: Open App Data Directory
    let data_dir_open = data_dir.clone();
    ui.on_open_data_dir(move || {
        let _ = Command::new("xdg-open").arg(&data_dir_open).spawn();
    });

    // Callback: Cancel Dialog
    let ui_weak_cancel = ui.as_weak();
    ui.on_cancel_dialog(move || {
        if let Some(ui) = ui_weak_cancel.upgrade() {
            ui.set_show_confirm_dialog(false);
        }
    });

    // Callback: Open Benchmark Custom Settings Dialog
    let profiles_open_cfg = profiles.clone();
    let engine_open_cfg = engine.clone();
    let custom_configs_open_cfg = custom_configs.clone();
    let ui_weak_open_cfg = ui.as_weak();
    ui.on_open_benchmark_settings(move |id, version| {
        let profile = match profiles_open_cfg.iter().find(|p| p.id == id.as_str()) {
            Some(p) => p.clone(),
            None => return,
        };
        if !engine_open_cfg.is_version_installed(id.as_str(), version.as_str()) {
            tracing::warn!(
                "Settings dialog rejected: benchmark '{}' (version '{}') is not installed",
                id,
                version
            );
            return;
        }
        let effective = profile.for_version(version.as_str());
        let (localized_name, _, _) = get_localized_profile_info(
            &profile.id,
            &effective.name,
            &effective.category,
            effective.description.as_deref().unwrap_or_default(),
        );

        if let Some(ui) = ui_weak_open_cfg.upgrade() {
            ui.set_bench_config_id(id.clone());
            ui.set_bench_config_version(version.clone());
            ui.set_bench_config_title(
                benchhub::i18n::t_fmt("bench_settings_title_format", &[&localized_name]).into(),
            );
            ui.set_bench_config_desc(benchhub::i18n::t("bench_settings_desc").into());

            let cfgs = custom_configs_open_cfg
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let current_timeout = cfgs.get(id.as_str()).and_then(|c| c.timeout_secs);
            let dur_display = benchhub::bench_config::format_bench_duration(
                current_timeout,
                &benchhub::i18n::get_language(),
            );
            ui.set_bench_config_selected_duration(dur_display.into());

            if id == "ffmpeg" {
                ui.set_bench_config_is_ffmpeg(true);
                ui.set_bench_config_is_7zip(false);
                ui.set_bench_config_is_unigine(false);
                ui.set_bench_config_has_preset(false);

                let ffmpeg_cfg = cfgs
                    .get("ffmpeg")
                    .and_then(|c| c.ffmpeg.clone())
                    .unwrap_or_else(|| {
                        benchhub::bench_config::FfmpegConfig::for_version(version.as_str())
                    });

                ui.set_bench_config_ffmpeg_selected_codec(ffmpeg_cfg.codec.clone().into());
                ui.set_bench_config_ffmpeg_crf(ffmpeg_cfg.crf as i32);
                ui.set_bench_config_ffmpeg_crf_str(ffmpeg_cfg.crf.to_string().into());
                ui.set_bench_config_ffmpeg_selected_preset(ffmpeg_cfg.preset.clone().into());
                let localized_source = if ffmpeg_cfg.input_source.to_lowercase().contains("bunny") {
                    benchhub::i18n::t("bench_settings_source_bunny")
                } else if ffmpeg_cfg.input_source.to_lowercase().contains("steel")
                    || ffmpeg_cfg.input_source.to_lowercase().contains("tears")
                {
                    benchhub::i18n::t("bench_settings_source_tears")
                } else {
                    benchhub::i18n::t("bench_settings_source_synthetic")
                };
                ui.set_bench_config_ffmpeg_selected_input_source(localized_source.into());
                let dur_str = if !ffmpeg_cfg.duration_enabled {
                    benchhub::i18n::t("bench_settings_dur_unlimited")
                } else if ffmpeg_cfg.duration_secs == 20 {
                    benchhub::i18n::t("bench_settings_dur_20s")
                } else {
                    format!("{}s", ffmpeg_cfg.duration_secs)
                };
                ui.set_bench_config_ffmpeg_selected_duration(dur_str.into());
                ui.set_bench_config_ffmpeg_threads(ffmpeg_cfg.threads as i32);
                ui.set_bench_config_ffmpeg_threads_str(ffmpeg_cfg.threads.to_string().into());
                ui.set_bench_config_custom_args(ffmpeg_cfg.custom_args.clone().into());

                let (_, cmd_preview, _) = ffmpeg_cfg.to_run_args();
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else if id == "7zip" {
                ui.set_bench_config_is_ffmpeg(false);
                ui.set_bench_config_is_7zip(true);
                ui.set_bench_config_is_unigine(false);
                ui.set_bench_config_has_preset(false);

                let zip_cfg = cfgs
                    .get("7zip")
                    .and_then(|c| c.seven_zip.clone())
                    .unwrap_or_default();

                let dict_size_str = if zip_cfg.dict_size.contains("32MB") {
                    benchhub::i18n::t("bench_settings_dict_32mb")
                } else {
                    zip_cfg.dict_size.clone()
                };

                ui.set_bench_config_7zip_threads(zip_cfg.threads as i32);
                ui.set_bench_config_7zip_threads_str(zip_cfg.threads.to_string().into());
                ui.set_bench_config_7zip_selected_dict_size(dict_size_str.into());
                ui.set_bench_config_7zip_passes(zip_cfg.passes as i32);
                ui.set_bench_config_7zip_passes_str(zip_cfg.passes.to_string().into());
                ui.set_bench_config_custom_args(zip_cfg.custom_args.clone().into());

                let (_, cmd_preview, _) = zip_cfg.to_run_args();
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else if id == "unigine" {
                ui.set_bench_config_is_ffmpeg(false);
                ui.set_bench_config_is_7zip(false);
                ui.set_bench_config_is_unigine(true);
                ui.set_bench_config_has_preset(false);

                if version.contains("Heaven") {
                    let presets: Vec<slint::SharedString> =
                        vec!["Extreme (1080p)".into(), "Basic (720p)".into()];
                    ui.set_bench_config_unigine_presets(slint::ModelRc::new(
                        slint::VecModel::from(presets),
                    ));
                } else {
                    let presets: Vec<slint::SharedString> = vec![
                        "1080p Extreme".into(),
                        "1080p High".into(),
                        "1080p Medium".into(),
                        "720p Low".into(),
                        "4K Optimized".into(),
                        "8K Optimized".into(),
                    ];
                    ui.set_bench_config_unigine_presets(slint::ModelRc::new(
                        slint::VecModel::from(presets),
                    ));
                }

                let unigine_cfg = cfgs
                    .get("unigine")
                    .and_then(|c| c.unigine.clone())
                    .unwrap_or_else(|| {
                        benchhub::bench_config::UnigineConfig::for_version(version.as_str())
                    });

                ui.set_bench_config_unigine_selected_preset(unigine_cfg.preset.clone().into());
                ui.set_bench_config_custom_args(unigine_cfg.custom_args.clone().into());

                let (_, cmd_preview, _) = unigine_cfg.to_run_args(version.as_str());
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else {
                ui.set_bench_config_is_ffmpeg(false);
                ui.set_bench_config_is_7zip(false);
                ui.set_bench_config_is_unigine(false);

                let (has_preset, preset_title, preset_desc, preset_options, default_preset) =
                    match id.as_str() {
                        "cray" => (
                            true,
                            benchhub::i18n::t("bench_settings_cray_resolution"),
                            benchhub::i18n::t("bench_settings_cray_res_desc"),
                            vec!["1080p", "4K", "720p"],
                            "1080p",
                        ),
                        "ycruncher" => (
                            true,
                            benchhub::i18n::t("bench_settings_problem_size"),
                            benchhub::i18n::t("bench_settings_ycruncher_desc"),
                            vec!["500M", "25M"],
                            "500M",
                        ),
                        "stream" => (
                            true,
                            benchhub::i18n::t("bench_settings_array_size"),
                            benchhub::i18n::t("bench_settings_stream_desc"),
                            vec!["Standard", "Compact"],
                            "Standard",
                        ),
                        "fio" => (
                            true,
                            benchhub::i18n::t("bench_settings_workload"),
                            benchhub::i18n::t("bench_settings_fio_desc"),
                            vec!["Seq-Read", "Seq-Write", "Rand-Read", "Rand-Write"],
                            "Seq-Read",
                        ),
                        "gravitymark" => (
                            true,
                            benchhub::i18n::t("bench_settings_graphics_api"),
                            benchhub::i18n::t("bench_settings_gravitymark_desc"),
                            vec!["vulkan", "opengl"],
                            "vulkan",
                        ),
                        "llama-bench" => (
                            true,
                            benchhub::i18n::t("bench_settings_model"),
                            benchhub::i18n::t("bench_settings_llama_desc"),
                            vec!["Fast (stories260K)", "Standard (stories15M)"],
                            "Fast (stories260K)",
                        ),
                        _ => (false, String::new(), String::new(), vec![], ""),
                    };

                ui.set_bench_config_has_preset(has_preset);
                if has_preset {
                    ui.set_bench_config_preset_title(preset_title.into());
                    ui.set_bench_config_preset_desc(preset_desc.into());
                    let opt_shared: Vec<slint::SharedString> =
                        preset_options.iter().map(|s| (*s).into()).collect();
                    ui.set_bench_config_preset_options(slint::ModelRc::new(slint::VecModel::from(
                        opt_shared,
                    )));
                    let current_preset = cfgs
                        .get(id.as_str())
                        .and_then(|c| c.custom_preset_name.clone())
                        .unwrap_or_else(|| default_preset.to_string());
                    ui.set_bench_config_selected_preset(current_preset.into());
                } else {
                    ui.set_bench_config_selected_preset("".into());
                }

                let custom_args = cfgs
                    .get(id.as_str())
                    .map(|c| c.custom_args.clone())
                    .unwrap_or_default();
                ui.set_bench_config_custom_args(custom_args.clone().into());

                let selected_preset = ui.get_bench_config_selected_preset().to_string();
                let effective = profile.for_version(if has_preset && !selected_preset.is_empty() {
                    &selected_preset
                } else {
                    version.as_str()
                });
                let preview = if custom_args.is_empty() {
                    format!("{} {:?}", effective.get_run_cmd(), effective.run_args)
                } else {
                    format!(
                        "{} {:?} {}",
                        effective.get_run_cmd(),
                        effective.run_args,
                        custom_args
                    )
                };
                ui.set_bench_config_command_preview(preview.into());
            }

            ui.set_show_bench_config_dialog(true);
        }
    });

    // Callback: Close Benchmark Custom Settings Dialog
    let ui_weak_close_cfg = ui.as_weak();
    ui.on_close_benchmark_settings(move || {
        if let Some(ui) = ui_weak_close_cfg.upgrade() {
            ui.set_show_bench_config_dialog(false);
        }
    });

    // Callback: Update Benchmark Config Preview
    let profiles_upd_cfg = profiles.clone();
    let ui_weak_upd_cfg = ui.as_weak();
    ui.on_trigger_update_preview(move || {
        if let Some(ui) = ui_weak_upd_cfg.upgrade() {
            let id = ui.get_bench_config_id();
            let version = ui.get_bench_config_version();

            if id == "ffmpeg" {
                let crf: u32 = ui
                    .get_bench_config_ffmpeg_crf_str()
                    .parse()
                    .unwrap_or(23)
                    .clamp(0, 51);
                ui.set_bench_config_ffmpeg_crf(crf as i32);
                let threads: u32 = ui
                    .get_bench_config_ffmpeg_threads_str()
                    .parse()
                    .unwrap_or(0)
                    .min(128);
                ui.set_bench_config_ffmpeg_threads(threads as i32);
                let duration_str = ui.get_bench_config_ffmpeg_selected_duration();
                let (duration_enabled, duration_secs) =
                    if duration_str.contains("Süresiz") || duration_str.contains("Tam") {
                        (false, 0)
                    } else {
                        let num: u32 = duration_str
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .unwrap_or(20);
                        (true, num)
                    };

                let ffmpeg_cfg = benchhub::bench_config::FfmpegConfig {
                    codec: ui.get_bench_config_ffmpeg_selected_codec().to_string(),
                    crf,
                    preset: ui.get_bench_config_ffmpeg_selected_preset().to_string(),
                    resolution: ui.get_bench_config_ffmpeg_selected_resolution().to_string(),
                    input_source: ui
                        .get_bench_config_ffmpeg_selected_input_source()
                        .to_string(),
                    duration_enabled,
                    duration_secs,
                    threads,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                };
                let (_, cmd_preview, _) = ffmpeg_cfg.to_run_args();
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else if id == "7zip" {
                let threads: u32 = ui
                    .get_bench_config_7zip_threads_str()
                    .parse()
                    .unwrap_or(0)
                    .min(128);
                ui.set_bench_config_7zip_threads(threads as i32);
                let passes: u32 = ui
                    .get_bench_config_7zip_passes_str()
                    .parse()
                    .unwrap_or(1)
                    .clamp(1, 50);
                ui.set_bench_config_7zip_passes(passes as i32);

                let seven_zip_cfg = benchhub::bench_config::SevenZipConfig {
                    threads,
                    dict_size: ui.get_bench_config_7zip_selected_dict_size().to_string(),
                    passes,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                };
                let (_, cmd_preview, _) = seven_zip_cfg.to_run_args();
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else if id == "unigine" {
                let unigine_cfg = benchhub::bench_config::UnigineConfig {
                    preset: ui.get_bench_config_unigine_selected_preset().to_string(),
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                };
                let (_, cmd_preview, _) = unigine_cfg.to_run_args(version.as_str());
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else {
                if let Some(p) = profiles_upd_cfg.iter().find(|p| p.id == id.as_str()) {
                    let has_preset = ui.get_bench_config_has_preset();
                    let selected_preset = ui.get_bench_config_selected_preset().to_string();
                    let effective = p.for_version(if has_preset && !selected_preset.is_empty() {
                        &selected_preset
                    } else {
                        version.as_str()
                    });
                    let custom_args = ui.get_bench_config_custom_args();
                    let preview = if custom_args.is_empty() {
                        format!("{} {:?}", effective.get_run_cmd(), effective.run_args)
                    } else {
                        format!(
                            "{} {:?} {}",
                            effective.get_run_cmd(),
                            effective.run_args,
                            custom_args
                        )
                    };
                    ui.set_bench_config_command_preview(preview.into());
                }
            }
        }
    });

    // Callback: Save Current Settings
    let custom_configs_save_cfg = custom_configs.clone();
    let profiles_save_cfg = profiles.clone();
    let ui_weak_save_cfg = ui.as_weak();
    ui.on_save_current_settings(move || {
        if let Some(ui) = ui_weak_save_cfg.upgrade() {
            let id = ui.get_bench_config_id();
            let version = ui.get_bench_config_version();

            let (localized_name, _, _) =
                if let Some(p) = profiles_save_cfg.iter().find(|p| p.id == id.as_str()) {
                    get_localized_profile_info(
                        &p.id,
                        &p.name,
                        &p.category,
                        p.description.as_deref().unwrap_or_default(),
                    )
                } else {
                    (id.to_string(), String::new(), String::new())
                };

            let duration_sel_str = ui.get_bench_config_selected_duration();
            let bench_timeout = benchhub::bench_config::parse_bench_duration(&duration_sel_str);

            if id == "ffmpeg" {
                let crf: u32 = ui
                    .get_bench_config_ffmpeg_crf_str()
                    .parse()
                    .unwrap_or(23)
                    .clamp(0, 51);
                let threads: u32 = ui
                    .get_bench_config_ffmpeg_threads_str()
                    .parse()
                    .unwrap_or(0)
                    .min(128);
                let duration_str = ui.get_bench_config_ffmpeg_selected_duration();
                let (duration_enabled, duration_secs) =
                    if duration_str.contains("Süresiz") || duration_str.contains("Tam") {
                        (false, 0)
                    } else {
                        let num: u32 = duration_str
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .unwrap_or(20);
                        (true, num)
                    };

                let ffmpeg_cfg = benchhub::bench_config::FfmpegConfig {
                    codec: ui.get_bench_config_ffmpeg_selected_codec().to_string(),
                    crf,
                    preset: ui.get_bench_config_ffmpeg_selected_preset().to_string(),
                    resolution: ui.get_bench_config_ffmpeg_selected_resolution().to_string(),
                    input_source: ui
                        .get_bench_config_ffmpeg_selected_input_source()
                        .to_string(),
                    duration_enabled,
                    duration_secs,
                    threads,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                };
                let (run_args, _, preset_name) = ffmpeg_cfg.to_run_args();
                let custom_cfg = benchhub::bench_config::BenchmarkCustomConfig {
                    benchmark_id: id.to_string(),
                    ffmpeg: Some(ffmpeg_cfg),
                    seven_zip: None,
                    unigine: None,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                    timeout_secs: bench_timeout,
                    effective_run_args: Some(run_args),
                    custom_preset_name: Some(preset_name),
                };
                custom_configs_save_cfg
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.to_string(), custom_cfg);
            } else if id == "7zip" {
                let threads: u32 = ui
                    .get_bench_config_7zip_threads_str()
                    .parse()
                    .unwrap_or(0)
                    .min(128);
                let passes: u32 = ui
                    .get_bench_config_7zip_passes_str()
                    .parse()
                    .unwrap_or(1)
                    .clamp(1, 50);
                let seven_zip_cfg = benchhub::bench_config::SevenZipConfig {
                    threads,
                    dict_size: ui.get_bench_config_7zip_selected_dict_size().to_string(),
                    passes,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                };
                let (run_args, _, preset_name) = seven_zip_cfg.to_run_args();
                let custom_cfg = benchhub::bench_config::BenchmarkCustomConfig {
                    benchmark_id: id.to_string(),
                    ffmpeg: None,
                    seven_zip: Some(seven_zip_cfg),
                    unigine: None,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                    timeout_secs: bench_timeout,
                    effective_run_args: Some(run_args),
                    custom_preset_name: Some(preset_name),
                };
                custom_configs_save_cfg
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.to_string(), custom_cfg);
            } else if id == "unigine" {
                let unigine_cfg = benchhub::bench_config::UnigineConfig {
                    preset: ui.get_bench_config_unigine_selected_preset().to_string(),
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                };
                let (run_args, _, preset_name) = unigine_cfg.to_run_args(version.as_str());
                let custom_cfg = benchhub::bench_config::BenchmarkCustomConfig {
                    benchmark_id: id.to_string(),
                    ffmpeg: None,
                    seven_zip: None,
                    unigine: Some(unigine_cfg),
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                    timeout_secs: bench_timeout,
                    effective_run_args: Some(run_args),
                    custom_preset_name: Some(preset_name),
                };
                custom_configs_save_cfg
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.to_string(), custom_cfg);
            } else {
                let has_preset = ui.get_bench_config_has_preset();
                let selected_preset = ui.get_bench_config_selected_preset().to_string();
                let effective = profiles_save_cfg
                    .iter()
                    .find(|p| p.id == id.as_str())
                    .map(|p| {
                        p.for_version(if has_preset && !selected_preset.is_empty() {
                            &selected_preset
                        } else {
                            version.as_str()
                        })
                    });

                let effective_run_args = effective.map(|e| {
                    let mut args = e.run_args.clone();
                    let custom_args = ui.get_bench_config_custom_args();
                    if !custom_args.trim().is_empty() {
                        for token in custom_args.split_whitespace() {
                            if benchhub::bench_config::is_safe_custom_arg(token) {
                                args.push(token.to_string());
                            }
                        }
                    }
                    args
                });

                let custom_cfg = benchhub::bench_config::BenchmarkCustomConfig {
                    benchmark_id: id.to_string(),
                    ffmpeg: None,
                    seven_zip: None,
                    unigine: None,
                    custom_args: ui.get_bench_config_custom_args().to_string(),
                    timeout_secs: bench_timeout,
                    effective_run_args,
                    custom_preset_name: if has_preset && !selected_preset.is_empty() {
                        Some(selected_preset)
                    } else {
                        None
                    },
                };
                custom_configs_save_cfg
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.to_string(), custom_cfg);
            }

            ui.set_show_bench_config_dialog(false);
            let msg =
                benchhub::i18n::t("status_bench_settings_saved").replace("{}", &localized_name);
            ui.set_status_text(msg.into());
        }
    });

    // Callback: Reset Benchmark Custom Settings
    let custom_configs_reset_cfg = custom_configs.clone();
    let profiles_reset_cfg = profiles.clone();
    let ui_weak_reset_cfg = ui.as_weak();
    ui.on_reset_benchmark_settings(move |id| {
        custom_configs_reset_cfg
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id.as_str());

        let (localized_name, _, _) =
            if let Some(p) = profiles_reset_cfg.iter().find(|p| p.id == id.as_str()) {
                get_localized_profile_info(
                    &p.id,
                    &p.name,
                    &p.category,
                    p.description.as_deref().unwrap_or_default(),
                )
            } else {
                (id.to_string(), String::new(), String::new())
            };

        if let Some(ui) = ui_weak_reset_cfg.upgrade() {
            let ver = ui.get_bench_config_version();
            if id == "ffmpeg" {
                let default_cfg = benchhub::bench_config::FfmpegConfig::for_version(ver.as_str());
                ui.set_bench_config_ffmpeg_selected_codec(default_cfg.codec.clone().into());
                ui.set_bench_config_ffmpeg_crf(default_cfg.crf as i32);
                ui.set_bench_config_ffmpeg_crf_str(default_cfg.crf.to_string().into());
                ui.set_bench_config_ffmpeg_selected_preset(default_cfg.preset.clone().into());
                ui.set_bench_config_ffmpeg_selected_resolution(
                    default_cfg.resolution.clone().into(),
                );
                ui.set_bench_config_ffmpeg_selected_input_source(
                    default_cfg.input_source.clone().into(),
                );
                let dur_str = if !default_cfg.duration_enabled {
                    "Süresiz (Tam Kaynak)".to_string()
                } else {
                    format!("{}s", default_cfg.duration_secs)
                };
                ui.set_bench_config_ffmpeg_selected_duration(dur_str.into());
                ui.set_bench_config_ffmpeg_threads(default_cfg.threads as i32);
                ui.set_bench_config_ffmpeg_threads_str(default_cfg.threads.to_string().into());
                ui.set_bench_config_custom_args(default_cfg.custom_args.clone().into());

                let (_, cmd_preview, _) = default_cfg.to_run_args();
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else if id == "7zip" {
                let default_cfg = benchhub::bench_config::SevenZipConfig::default();
                ui.set_bench_config_7zip_threads(default_cfg.threads as i32);
                ui.set_bench_config_7zip_threads_str(default_cfg.threads.to_string().into());
                ui.set_bench_config_7zip_selected_dict_size(default_cfg.dict_size.clone().into());
                ui.set_bench_config_7zip_passes(default_cfg.passes as i32);
                ui.set_bench_config_7zip_passes_str(default_cfg.passes.to_string().into());
                ui.set_bench_config_custom_args("".into());

                let (_, cmd_preview, _) = default_cfg.to_run_args();
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else if id == "unigine" {
                let default_cfg = benchhub::bench_config::UnigineConfig::for_version(ver.as_str());
                ui.set_bench_config_unigine_selected_preset(default_cfg.preset.clone().into());
                ui.set_bench_config_custom_args("".into());

                let (_, cmd_preview, _) = default_cfg.to_run_args(ver.as_str());
                ui.set_bench_config_command_preview(cmd_preview.into());
            } else {
                ui.set_bench_config_custom_args("".into());
                if let Some(p) = profiles_reset_cfg.iter().find(|p| p.id == id.as_str()) {
                    let effective = p.for_version(ver.as_str());
                    ui.set_bench_config_command_preview(
                        format!("{} {:?}", effective.get_run_cmd(), effective.run_args).into(),
                    );
                }
            }

            let default_dur_str = benchhub::bench_config::format_bench_duration(
                None,
                &benchhub::i18n::get_language(),
            );
            ui.set_bench_config_selected_duration(default_dur_str.into());

            let msg =
                benchhub::i18n::t("status_bench_settings_reset").replace("{}", &localized_name);
            ui.set_status_text(msg.into());
        }
    });

    // History Filter: Search Text
    let history_filter_search = history_filter_state.clone();
    let db_filter_search = db.clone();
    let ui_weak_search = ui.as_weak();
    ui.on_filter_history_search(move |query| {
        let mut filter = history_filter_search
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        filter.search = query.to_string();
        if let Some(ui) = ui_weak_search.upgrade() {
            refresh_history_view(&ui, &db_filter_search, &filter);
        }
    });

    // History Filter: Category
    let history_filter_cat = history_filter_state.clone();
    let db_filter_cat = db.clone();
    let ui_weak_cat = ui.as_weak();
    ui.on_filter_history_category(move |cat| {
        let mut filter = history_filter_cat.lock().unwrap_or_else(|e| e.into_inner());
        filter.category = cat.to_string();
        if let Some(ui) = ui_weak_cat.upgrade() {
            refresh_history_view(&ui, &db_filter_cat, &filter);
        }
    });

    // History Filter: GPU Mode
    let history_filter_gpu = history_filter_state.clone();
    let db_filter_gpu = db.clone();
    let ui_weak_gpu_filter = ui.as_weak();
    ui.on_filter_history_gpu_mode(move |mode| {
        let mut filter = history_filter_gpu.lock().unwrap_or_else(|e| e.into_inner());
        filter.gpu_mode = mode.to_string();
        if let Some(ui) = ui_weak_gpu_filter.upgrade() {
            refresh_history_view(&ui, &db_filter_gpu, &filter);
        }
    });

    // History Filter: Status
    let history_filter_st = history_filter_state.clone();
    let db_filter_st = db.clone();
    let ui_weak_st = ui.as_weak();
    ui.on_filter_history_status(move |status| {
        let mut filter = history_filter_st.lock().unwrap_or_else(|e| e.into_inner());
        filter.status = status.to_string();
        if let Some(ui) = ui_weak_st.upgrade() {
            refresh_history_view(&ui, &db_filter_st, &filter);
        }
    });

    // History Filter: Sort Order
    let history_filter_sort = history_filter_state.clone();
    let db_filter_sort = db.clone();
    let ui_weak_sort = ui.as_weak();
    ui.on_filter_history_sort(move |sort_str| {
        let mut filter = history_filter_sort
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        filter.sort_order = HistorySortOrder::from_display_str(sort_str.as_str());
        if let Some(ui) = ui_weak_sort.upgrade() {
            refresh_history_view(&ui, &db_filter_sort, &filter);
        }
    });

    // Callback: Toggle History Selection
    let history_filter_select = history_filter_state.clone();
    let db_filter_select = db.clone();
    let ui_weak_select = ui.as_weak();
    ui.on_toggle_history_selection(move |id| {
        let id_64 = id as i64;
        let mut filter = history_filter_select
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = filter.selected_ids.iter().position(|&x| x == id_64) {
            filter.selected_ids.remove(pos);
        } else {
            filter.selected_ids.push(id_64);
        }
        if let Some(ui) = ui_weak_select.upgrade() {
            refresh_history_view(&ui, &db_filter_select, &filter);
        }
    });

    // Callback: Create Methodology
    let history_filter_create_meth = history_filter_state.clone();
    let db_create_meth = db.clone();
    let app_state_create_meth = app_state.clone();
    let ui_weak_create_meth = ui.as_weak();
    ui.on_create_methodology(move || {
        let mut filter = history_filter_create_meth.lock().unwrap_or_else(|e| e.into_inner());
        let count = filter.selected_ids.len();
        if count < 2 {
            return;
        }
        
        match app_state_create_meth.service.create_methodology(&filter.selected_ids) {
            Ok(_new_run) => {
                // Varsayılanda metodolojinin alt testleri gizli (collapsed) kalsın
                filter.selected_ids.clear();
                if let Some(ui) = ui_weak_create_meth.upgrade() {
                    let msg = benchhub::i18n::t("methodology_created_success")
                        .replace("{}", &filter.selected_ids.len().to_string());
                    ui.set_status_text(msg.into());
                    refresh_history_view(&ui, &db_create_meth, &filter);
                }
            }
            Err(e) => {
                if let Some(ui) = ui_weak_create_meth.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_error")
                            .replace("{}", &e.to_string())
                            .into(),
                    );
                }
            }
        }
    });

    // Callback: Toggle Methodology Expand
    let history_filter_toggle_meth = history_filter_state.clone();
    let db_toggle_meth = db.clone();
    let ui_weak_toggle_meth = ui.as_weak();
    ui.on_toggle_methodology_expand(move |id| {
        let id_64 = id as i64;
        let mut filter = history_filter_toggle_meth.lock().unwrap_or_else(|e| e.into_inner());
        if filter.expanded_methodology_ids.contains(&id_64) {
            filter.expanded_methodology_ids.remove(&id_64);
        } else {
            filter.expanded_methodology_ids.insert(id_64);
        }
        if let Some(ui) = ui_weak_toggle_meth.upgrade() {
            refresh_history_view(&ui, &db_toggle_meth, &filter);
        }
    });

    // Callback: Open Comparison Modal
    let db_compare = db.clone();
    let history_filter_compare = history_filter_state.clone();
    let ui_weak_compare = ui.as_weak();
    ui.on_open_comparison_modal(move || {
        let filter = history_filter_compare
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if filter.selected_ids.len() >= 2 {
            let mut runs = Vec::new();
            for &id in &filter.selected_ids {
                if let Ok(Some(run)) = db_compare.get_run_by_id(id) {
                    runs.push(run);
                }
            }

            if runs.len() >= 2 {
                let first_cat = runs[0].category.clone();
                if let Some(diff_run) = runs.iter().find(|r| r.category != first_cat) {
                    if let Some(ui) = ui_weak_compare.upgrade() {
                        let warn_msg = benchhub::i18n::t("warn_different_categories")
                            .replace("{}", &diff_run.category);
                        ui.set_status_text(warn_msg.into());
                    }
                    return;
                }

                if let Some(ui) = ui_weak_compare.upgrade() {
                    let peak_lbl = benchhub::i18n::t("peak_prefix")
                        .trim_end_matches(':')
                        .to_string();
                    let columns: Vec<CompareRunColumn> = runs
                        .iter()
                        .enumerate()
                        .map(|(idx, r)| {
                            let date_str = DateTime::from_timestamp(r.timestamp, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_default();

                            let has_cpu_throttling = r.cpu_throttling != "Yok"
                                && r.cpu_throttling != "Hayır"
                                && r.cpu_throttling != "-"
                                && r.cpu_throttling != "None";
                            let has_gpu_throttling = r.gpu_throttling != "Yok"
                                && r.gpu_throttling != "Hayır"
                                && r.gpu_throttling != "-"
                                && r.gpu_throttling != "None";

                            let (localized_name, _, _) = get_localized_profile_info(
                                &r.benchmark_id,
                                &r.benchmark_id,
                                &r.category,
                                "",
                            );

                            let gpu_mode_display = benchhub::i18n::localize_gpu_mode(&r.gpu_mode);
                            let power_profile_display =
                                benchhub::i18n::localize_power_profile(&r.power_profile);
                            let status_display = benchhub::i18n::localize_status(&r.status);
                            let mut cpu_throttling_display =
                                benchhub::i18n::localize_throttling(&r.cpu_throttling);
                            let mut gpu_throttling_display =
                                benchhub::i18n::localize_throttling(&r.gpu_throttling);

                            let is_en = benchhub::i18n::get_language() == "en";
                            if is_en {
                                if cpu_throttling_display.contains("(%") {
                                    cpu_throttling_display = cpu_throttling_display
                                        .replace("(%", "(")
                                        .replace(')', "%)");
                                }
                                if gpu_throttling_display.contains("(%") {
                                    gpu_throttling_display = gpu_throttling_display
                                        .replace("(%", "(")
                                        .replace(')', "%)");
                                }
                            }

                            let cpu_usage_str = if is_en {
                                format!(
                                    "{:.0}% ({} {:.0}%)",
                                    r.avg_cpu_usage, peak_lbl, r.peak_cpu_usage
                                )
                            } else {
                                format!(
                                    "%{:.0} ({} %{:.0})",
                                    r.avg_cpu_usage, peak_lbl, r.peak_cpu_usage
                                )
                            };

                            let gpu_usage_str = if is_en {
                                format!(
                                    "{:.0}% ({} {:.0}%)",
                                    r.avg_gpu_usage, peak_lbl, r.peak_gpu_usage
                                )
                            } else {
                                format!(
                                    "%{:.0} ({} %{:.0})",
                                    r.avg_gpu_usage, peak_lbl, r.peak_gpu_usage
                                )
                            };

                            let sys_info_display = if r.system_info_summary.is_empty() {
                                "-".to_string()
                            } else {
                                r.system_info_summary.clone()
                            };

                            CompareRunColumn {
                                id: (idx + 1) as i32,
                                title: localized_name.into(),
                                preset: r.preset_or_version.clone().into(),
                                gpu_mode: gpu_mode_display.into(),
                                power_profile: power_profile_display.into(),
                                score: r
                                    .score
                                    .map(|s| format!("{:.1}", s))
                                    .unwrap_or_else(|| "-".to_string())
                                    .into(),
                                status: status_display.into(),
                                duration: format!("{:.1}s", r.duration_secs).into(),
                                date: date_str.into(),
                                system_info: sys_info_display.into(),

                                // CPU
                                cpu_usage: cpu_usage_str.into(),
                                cpu_temp: format!(
                                    "{:.1}°C / {:.1}°C",
                                    r.avg_cpu_temp, r.peak_cpu_temp
                                )
                                .into(),
                                cpu_freq: format!(
                                    "{} MHz / {} MHz",
                                    r.avg_cpu_freq_mhz, r.peak_cpu_freq_mhz
                                )
                                .into(),
                                cpu_power: format!(
                                    "{:.1}W ({} {:.1}W)",
                                    r.avg_power_w, peak_lbl, r.peak_power_w
                                )
                                .into(),
                                cpu_throttling: cpu_throttling_display.into(),
                                has_cpu_throttling,

                                // GPU
                                gpu_usage: gpu_usage_str.into(),
                                gpu_temp: format!(
                                    "{:.1}°C / {:.1}°C",
                                    r.avg_gpu_temp, r.peak_gpu_temp
                                )
                                .into(),
                                gpu_freq: format!(
                                    "{} MHz / {} MHz",
                                    r.avg_gpu_freq_mhz, r.peak_gpu_freq_mhz
                                )
                                .into(),
                                vram_freq: format!(
                                    "{} MHz / {} MHz",
                                    r.avg_vram_freq_mhz, r.peak_vram_freq_mhz
                                )
                                .into(),
                                gpu_power: format!(
                                    "{:.1}W ({} {:.1}W)",
                                    r.avg_gpu_power_w, peak_lbl, r.peak_gpu_power_w
                                )
                                .into(),
                                gpu_throttling: gpu_throttling_display.into(),
                                has_gpu_throttling,

                                // Memory
                                ram: format!(
                                    "{:.1} GB ({} {:.1} GB)",
                                    r.avg_ram_gb, peak_lbl, r.peak_ram_gb
                                )
                                .into(),
                                vram: format!(
                                    "{:.1} GB ({} {:.1} GB)",
                                    r.avg_vram_gb, peak_lbl, r.peak_vram_gb
                                )
                                .into(),

                                // AC Power
                                ac_power: format!(
                                    "{:.1}W ({} {:.1}W)",
                                    r.avg_ac_power_w, peak_lbl, r.peak_ac_power_w
                                )
                                .into(),
                            }
                        })
                        .collect();

                    ui.set_compare_runs(ModelRc::from(Rc::new(VecModel::from(columns))));
                    ui.set_show_compare_dialog(true);
                }
            }
        }
    });

    // Callback: Close Comparison Modal
    let ui_weak_close_compare = ui.as_weak();
    ui.on_close_compare_dialog(move || {
        if let Some(ui) = ui_weak_close_compare.upgrade() {
            ui.set_show_compare_dialog(false);
        }
    });

    // Callback: Show Run Details (Modal with 2 tabs)
    let app_state_details = app_state.clone();
    let ui_weak_details = ui.as_weak();
    ui.on_show_run_details(move |id| {
        let run_opt = match app_state_details.service.get_run_by_id(id as i64) {
            Ok(r) => r,
            Err(e) => {
                if let Some(ui) = ui_weak_details.upgrade() {
                    ui.set_status_text(
                        benchhub::i18n::t("status_error")
                            .replace("{}", &e.to_string())
                            .into(),
                    );
                }
                return;
            }
        };
        if let Some(run) = run_opt {
            let score_str = run
                .score
                .map(|s| {
                    if s.fract() == 0.0 {
                        format!("{:.0}", s)
                    } else {
                        format!("{:.1}", s)
                    }
                })
                .unwrap_or_else(|| "-".to_string());

            let duration_str = if run.duration_secs > 0.0 {
                format!("{:.1}s", run.duration_secs)
            } else {
                "-".to_string()
            };

            let date_str = DateTime::from_timestamp(run.timestamp, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "-".to_string());

            let avg_cpu_usage = format!("%{:.0}", run.avg_cpu_usage);
            let peak_cpu_usage = format!("%{:.0}", run.peak_cpu_usage);

            let avg_cpu_temp = if run.avg_cpu_temp > 0.0 {
                format!("{:.1} °C", run.avg_cpu_temp)
            } else {
                "-".to_string()
            };

            let peak_cpu_temp = if run.peak_cpu_temp > 0.0 {
                format!("{:.1} °C", run.peak_cpu_temp)
            } else {
                "-".to_string()
            };

            let avg_gpu_usage = format!("%{:.0}", run.avg_gpu_usage);
            let peak_gpu_usage = format!("%{:.0}", run.peak_gpu_usage);

            let avg_gpu_temp = if run.avg_gpu_temp > 0.0 {
                format!("{:.1} °C", run.avg_gpu_temp)
            } else {
                "-".to_string()
            };

            let peak_gpu_temp = if run.peak_gpu_temp > 0.0 {
                format!("{:.1} °C", run.peak_gpu_temp)
            } else {
                "-".to_string()
            };

            let avg_gpu_power_w = if run.avg_gpu_power_w > 0.0 {
                format!("{:.1} W", run.avg_gpu_power_w)
            } else {
                "-".to_string()
            };

            let peak_gpu_power_w = if run.peak_gpu_power_w > 0.0 {
                format!("{:.1} W", run.peak_gpu_power_w)
            } else {
                "-".to_string()
            };

            let avg_power_w = if run.avg_power_w > 0.0 {
                format!("{:.1} W", run.avg_power_w)
            } else {
                "-".to_string()
            };

            let peak_power_w = if run.peak_power_w > 0.0 {
                format!("{:.1} W", run.peak_power_w)
            } else {
                "-".to_string()
            };

            let avg_ac_power_w = if run.avg_ac_power_w > 0.0 {
                format!("{:.1} W", run.avg_ac_power_w)
            } else {
                "-".to_string()
            };

            let peak_ac_power_w = if run.peak_ac_power_w > 0.0 {
                format!("{:.1} W", run.peak_ac_power_w)
            } else {
                "-".to_string()
            };

            let avg_cpu_freq_mhz = if run.avg_cpu_freq_mhz > 0 {
                format!("{} MHz", run.avg_cpu_freq_mhz)
            } else {
                "-".to_string()
            };

            let peak_cpu_freq_mhz = if run.peak_cpu_freq_mhz > 0 {
                format!("{} MHz", run.peak_cpu_freq_mhz)
            } else {
                "-".to_string()
            };

            let avg_gpu_freq_mhz = if run.avg_gpu_freq_mhz > 0 {
                format!("{} MHz", run.avg_gpu_freq_mhz)
            } else {
                "-".to_string()
            };

            let peak_gpu_freq_mhz = if run.peak_gpu_freq_mhz > 0 {
                format!("{} MHz", run.peak_gpu_freq_mhz)
            } else {
                "-".to_string()
            };

            let avg_vram_freq_mhz = if run.avg_vram_freq_mhz > 0 {
                format!("{} MHz", run.avg_vram_freq_mhz)
            } else {
                "-".to_string()
            };

            let peak_vram_freq_mhz = if run.peak_vram_freq_mhz > 0 {
                format!("{} MHz", run.peak_vram_freq_mhz)
            } else {
                "-".to_string()
            };

            let avg_ram_gb = if run.avg_ram_gb > 0.0 {
                format!("{:.1} GB", run.avg_ram_gb)
            } else {
                "-".to_string()
            };

            let peak_ram_gb = if run.peak_ram_gb > 0.0 {
                format!("{:.1} GB", run.peak_ram_gb)
            } else {
                "-".to_string()
            };

            let avg_vram_gb = if run.avg_vram_gb > 0.0 {
                format!("{:.1} GB", run.avg_vram_gb)
            } else {
                "-".to_string()
            };

            let peak_vram_gb = if run.peak_vram_gb > 0.0 {
                format!("{:.1} GB", run.peak_vram_gb)
            } else {
                "-".to_string()
            };

            let gpu_mode_display = benchhub::i18n::localize_gpu_mode(&run.gpu_mode);
            let power_profile_display = benchhub::i18n::localize_power_profile(&run.power_profile);
            let status_display = benchhub::i18n::localize_status(&run.status);
            let cpu_throttling_display = benchhub::i18n::localize_throttling(&run.cpu_throttling);
            let gpu_throttling_display = benchhub::i18n::localize_throttling(&run.gpu_throttling);

            let (localized_name, localized_cat, _) =
                get_localized_profile_info(&run.benchmark_id, &run.benchmark_id, &run.category, "");

            if let Some(ui) = ui_weak_details.upgrade() {
                if run.is_methodology {
                    ui.set_details_title(
                        format!("🔍 [METODOLOJİ] {}: {}", localized_name, run.preset_or_version).into(),
                    );
                } else {
                    ui.set_details_title(
                        format!("🔍 {}: {}", localized_name, run.preset_or_version).into(),
                    );
                }
                ui.set_details_benchmark_name(localized_name.into());
                ui.set_details_category(localized_cat.into());
                ui.set_details_preset_version(run.preset_or_version.into());
                ui.set_details_gpu_mode(gpu_mode_display.into());
                ui.set_details_power_profile(power_profile_display.into());
                ui.set_details_score(score_str.into());
                ui.set_details_status(status_display.into());
                ui.set_details_duration(duration_str.into());
                ui.set_details_date(date_str.into());
                ui.set_details_avg_cpu_usage(avg_cpu_usage.into());
                ui.set_details_peak_cpu_usage(peak_cpu_usage.into());
                ui.set_details_avg_cpu_temp(avg_cpu_temp.into());
                ui.set_details_peak_cpu_temp(peak_cpu_temp.into());
                ui.set_details_avg_cpu_freq_mhz(avg_cpu_freq_mhz.into());
                ui.set_details_peak_cpu_freq_mhz(peak_cpu_freq_mhz.into());
                ui.set_details_avg_gpu_usage(avg_gpu_usage.into());
                ui.set_details_peak_gpu_usage(peak_gpu_usage.into());
                ui.set_details_avg_gpu_temp(avg_gpu_temp.into());
                ui.set_details_peak_gpu_temp(peak_gpu_temp.into());
                ui.set_details_avg_gpu_freq_mhz(avg_gpu_freq_mhz.into());
                ui.set_details_peak_gpu_freq_mhz(peak_gpu_freq_mhz.into());
                ui.set_details_avg_vram_freq_mhz(avg_vram_freq_mhz.into());
                ui.set_details_peak_vram_freq_mhz(peak_vram_freq_mhz.into());
                ui.set_details_avg_gpu_power_w(avg_gpu_power_w.into());
                ui.set_details_peak_gpu_power_w(peak_gpu_power_w.into());
                ui.set_details_avg_power_w(avg_power_w.into());
                ui.set_details_peak_power_w(peak_power_w.into());
                ui.set_details_avg_ac_power_w(avg_ac_power_w.into());
                ui.set_details_peak_ac_power_w(peak_ac_power_w.into());
                ui.set_details_avg_ram_gb(avg_ram_gb.into());
                ui.set_details_peak_ram_gb(peak_ram_gb.into());
                ui.set_details_avg_vram_gb(avg_vram_gb.into());
                ui.set_details_peak_vram_gb(peak_vram_gb.into());
                ui.set_details_cpu_throttling(cpu_throttling_display.into());
                ui.set_details_gpu_throttling(gpu_throttling_display.into());
                ui.set_details_system_info(run.system_info_summary.into());
                ui.set_details_log_path(run.log_path.into());
                ui.set_details_log_content("".into());
                ui.set_show_details_log(false);
                ui.set_details_active_tab(0);
                ui.set_show_details_dialog(true);
            }
        }
    });

    // Callback: Close Details Modal
    let ui_weak_close_details = ui.as_weak();
    ui.on_close_details_modal(move || {
        if let Some(ui) = ui_weak_close_details.upgrade() {
            ui.set_show_details_log(false);
            ui.set_show_details_dialog(false);
        }
    });

    // Callback: Open Group Dialog
    let ui_weak_open_group = ui.as_weak();
    let db_open_group = db.clone();
    ui.on_open_group_dialog(move || {
        if let Some(ui) = ui_weak_open_group.upgrade() {
            let default_name = get_next_default_group_name(&db_open_group);
            ui.set_group_dialog_name(default_name.into());
            ui.set_show_group_dialog(true);
        }
    });

    // Callback: Close Group Dialog
    let ui_weak_close_group = ui.as_weak();
    ui.on_close_group_dialog(move || {
        if let Some(ui) = ui_weak_close_group.upgrade() {
            ui.set_show_group_dialog(false);
        }
    });

    // Callback: Confirm Group Runs
    let ui_weak_confirm_group = ui.as_weak();
    let service_confirm_group = app_state.service.clone();
    let filter_confirm_group = history_filter_state.clone();
    ui.on_confirm_group_runs(move |group_name| {
        let mut name = group_name.trim().to_string();
        if name.is_empty() {
            name = get_next_default_group_name(&service_confirm_group.db);
        }
        let mut filter = filter_confirm_group.lock().unwrap_or_else(|e| e.into_inner());
        if !filter.selected_ids.is_empty() {
            let count_str = filter.selected_ids.len().to_string();
            if let Ok(_) = service_confirm_group.group_runs(&filter.selected_ids, &name) {
                // Ensure newly created group is expanded by default
                filter.collapsed_group_names.remove(&name);
                if let Some(ui) = ui_weak_confirm_group.upgrade() {
                    let msg = benchhub::i18n::t("group_created_success")
                        .replacen("{}", &count_str, 1)
                        .replacen("{}", &name, 1);
                    ui.set_status_text(msg.into());
                }
            }
            filter.selected_ids.clear();
        }
        if let Some(ui) = ui_weak_confirm_group.upgrade() {
            ui.set_show_group_dialog(false);
            refresh_history_view(&ui, &service_confirm_group.db, &filter);
        }
    });

    // Callback: Ungroup Selected
    let ui_weak_ungroup = ui.as_weak();
    let service_ungroup = app_state.service.clone();
    let filter_ungroup = history_filter_state.clone();
    ui.on_ungroup_selected(move || {
        let mut filter = filter_ungroup.lock().unwrap_or_else(|e| e.into_inner());
        if !filter.selected_ids.is_empty() {
            if let Ok(_) = service_ungroup.ungroup_runs(&filter.selected_ids) {
                if let Some(ui) = ui_weak_ungroup.upgrade() {
                    let msg = benchhub::i18n::t("runs_ungrouped_success");
                    ui.set_status_text(msg.into());
                }
            }
            filter.selected_ids.clear();
            if let Some(ui) = ui_weak_ungroup.upgrade() {
                refresh_history_view(&ui, &service_ungroup.db, &filter);
            }
        }
    });

    // Callback: Toggle Group Expand
    let ui_weak_toggle_g_exp = ui.as_weak();
    let filter_toggle_g_exp = history_filter_state.clone();
    let db_toggle_g_exp = db.clone();
    ui.on_toggle_group_expand(move |group_name| {
        let mut filter = filter_toggle_g_exp.lock().unwrap_or_else(|e| e.into_inner());
        let name = group_name.to_string();
        if filter.collapsed_group_names.contains(&name) {
            filter.collapsed_group_names.remove(&name);
        } else {
            filter.collapsed_group_names.insert(name);
        }
        if let Some(ui) = ui_weak_toggle_g_exp.upgrade() {
            refresh_history_view(&ui, &db_toggle_g_exp, &filter);
        }
    });

    // Callback: Delete Group
    let ui_weak_del_group = ui.as_weak();
    let service_del_group = app_state.service.clone();
    let filter_del_group = history_filter_state.clone();
    ui.on_delete_group(move |group_name| {
        let name = group_name.to_string();
        if let Ok(_) = service_del_group.delete_group(&name) {
            if let Some(ui) = ui_weak_del_group.upgrade() {
                let msg = benchhub::i18n::t("group_deleted_success")
                    .replace("{}", &name);
                ui.set_status_text(msg.into());
            }
        }
        let mut filter = filter_del_group.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(g) = &filter.selected_group {
            if g == &name {
                filter.selected_group = None;
            }
        }
        if let Some(ui) = ui_weak_del_group.upgrade() {
            refresh_history_view(&ui, &service_del_group.db, &filter);
        }
    });

    // Callback: Filter History Group
    let ui_weak_filter_group = ui.as_weak();
    let filter_group = history_filter_state.clone();
    let db_filter_group = db.clone();
    ui.on_filter_history_group(move |group_name| {
        let mut filter = filter_group.lock().unwrap_or_else(|e| e.into_inner());
        let name = group_name.to_string();
        if name == benchhub::i18n::t("group_filter_all") {
            filter.selected_group = None;
        } else if name == benchhub::i18n::t("group_filter_ungrouped") {
            filter.selected_group = Some("".to_string());
        } else {
            filter.selected_group = Some(name);
        }
        if let Some(ui) = ui_weak_filter_group.upgrade() {
            refresh_history_view(&ui, &db_filter_group, &filter);
        }
    });
}

fn get_next_default_group_name(db: &benchhub::db::Db) -> String {
    let existing = db.get_distinct_groups().unwrap_or_default();
    for i in 1..=999 {
        let candidate = format!("grup-{:02}", i);
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    "grup-01".to_string()
}
