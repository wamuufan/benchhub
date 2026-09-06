use crate::state::HistoryFilterState;
use crate::*;
use benchhub::db::Db;
use benchhub::i18n;
use benchhub::models::RunResult;
use chrono::DateTime;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;

pub fn format_history_item(
    run: &RunResult,
    is_selected: bool,
    is_methodology: bool,
    is_child: bool,
    is_expanded: bool,
    has_children: bool,
    child_count: usize,
    depth: i32,
) -> HistoryItem {
    let date_str = DateTime::from_timestamp(run.timestamp, 0)
        .map(|dt| dt.format("%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string());

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

    let avg_pfx = i18n::t("avg_prefix");
    let peak_pfx = i18n::t("peak_prefix");

    // Formatted lines for 2-row table cells
    let cpu_avg_line = format!(
        "{} %{:.0} • {:.1}°C • {:.1} GHz",
        avg_pfx,
        run.avg_cpu_usage,
        run.avg_cpu_temp,
        (run.avg_cpu_freq_mhz as f32) / 1000.0
    );
    let cpu_peak_line = format!(
        "{} %{:.0} • {:.1}°C • {:.1} GHz",
        peak_pfx,
        run.peak_cpu_usage,
        run.peak_cpu_temp,
        (run.peak_cpu_freq_mhz as f32) / 1000.0
    );

    let gpu_avg_line = format!(
        "{} %{:.0} • {:.1}°C",
        avg_pfx, run.avg_gpu_usage, run.avg_gpu_temp
    );
    let gpu_peak_line = format!(
        "{} %{:.0} • {:.1}°C",
        peak_pfx, run.peak_gpu_usage, run.peak_gpu_temp
    );

    let power_ram_avg_line = format!(
        "{} {:.1} W • {:.1} GB",
        avg_pfx, run.avg_power_w, run.avg_ram_gb
    );
    let power_ram_peak_line = format!(
        "{} {:.1} W • {:.1} GB",
        peak_pfx, run.peak_power_w, run.peak_ram_gb
    );

    // Detailed metrics
    let avg_cpu_usage = format!("%{:.0}", run.avg_cpu_usage);
    let peak_cpu_usage = format!("%{:.0}", run.peak_cpu_usage);
    let avg_cpu_temp = if run.avg_cpu_temp > 0.0 {
        format!("{:.1}°C", run.avg_cpu_temp)
    } else {
        "-".to_string()
    };
    let peak_cpu_temp = if run.peak_cpu_temp > 0.0 {
        format!("{:.1}°C", run.peak_cpu_temp)
    } else {
        "-".to_string()
    };
    let avg_cpu_freq_mhz = if run.avg_cpu_freq_mhz > 0 {
        format!("{:.1} GHz", (run.avg_cpu_freq_mhz as f32) / 1000.0)
    } else {
        "-".to_string()
    };
    let peak_cpu_freq_mhz = if run.peak_cpu_freq_mhz > 0 {
        format!("{:.1} GHz", (run.peak_cpu_freq_mhz as f32) / 1000.0)
    } else {
        "-".to_string()
    };

    let avg_gpu_usage = format!("%{:.0}", run.avg_gpu_usage);
    let peak_gpu_usage = format!("%{:.0}", run.peak_gpu_usage);
    let avg_gpu_temp = if run.avg_gpu_temp > 0.0 {
        format!("{:.1}°C", run.avg_gpu_temp)
    } else {
        "-".to_string()
    };
    let peak_gpu_temp = if run.peak_gpu_temp > 0.0 {
        format!("{:.1}°C", run.peak_gpu_temp)
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

    let (localized_name, localized_cat, _) =
        get_localized_profile_info(&run.benchmark_id, &run.benchmark_id, &run.category, "");

    let gpu_mode_display = match benchhub::models::GpuMode::from_display_str(&run.gpu_mode) {
        benchhub::models::GpuMode::NvidiaDgpu => i18n::t("gpu_mode_nvidia"),
        benchhub::models::GpuMode::Integrated => i18n::t("gpu_mode_igpu"),
        benchhub::models::GpuMode::Auto => i18n::t("gpu_mode_default"),
    };

    let status_display = if run.status.contains("TAMAM") || run.status.contains("COMPLETED") {
        i18n::t("status_completed")
    } else if run.status.contains("HATA") || run.status.contains("ERROR") {
        i18n::t("status_error")
    } else if run.status.contains("İPTAL")
        || run.status.contains("Durduruldu")
        || run.status.contains("CANCEL")
    {
        i18n::t("status_cancelled")
    } else {
        run.status.clone()
    };

    let preset_display = if run.is_methodology {
        String::new()
    } else {
        run.preset_or_version.clone()
    };

    HistoryItem {
        id: run.id as i32,
        benchmark_id: localized_name.into(),
        category: localized_cat.into(),
        preset_or_version: preset_display.into(),
        gpu_mode: gpu_mode_display.into(),
        score: score_str.into(),
        status: status_display.into(),
        duration: duration_str.into(),
        date: date_str.into(),
        cpu_avg_line: cpu_avg_line.into(),
        cpu_peak_line: cpu_peak_line.into(),
        gpu_avg_line: gpu_avg_line.into(),
        gpu_peak_line: gpu_peak_line.into(),
        power_ram_avg_line: power_ram_avg_line.into(),
        power_ram_peak_line: power_ram_peak_line.into(),
        avg_cpu_usage: avg_cpu_usage.into(),
        peak_cpu_usage: peak_cpu_usage.into(),
        avg_cpu_temp: avg_cpu_temp.into(),
        peak_cpu_temp: peak_cpu_temp.into(),
        avg_cpu_freq_mhz: avg_cpu_freq_mhz.into(),
        peak_cpu_freq_mhz: peak_cpu_freq_mhz.into(),
        avg_gpu_usage: avg_gpu_usage.into(),
        peak_gpu_usage: peak_gpu_usage.into(),
        avg_gpu_temp: avg_gpu_temp.into(),
        peak_gpu_temp: peak_gpu_temp.into(),
        avg_power_w: avg_power_w.into(),
        peak_power_w: peak_power_w.into(),
        avg_ram_gb: avg_ram_gb.into(),
        peak_ram_gb: peak_ram_gb.into(),
        system_info_summary: run.system_info_summary.clone().into(),
        log_path: run.log_path.clone().into(),
        selected: is_selected,
        is_methodology,
        is_child,
        is_expanded,
        has_children,
        child_count: child_count as i32,
        depth,
    }
}

pub fn refresh_history_view(ui: &AppWindow, db: &Db, filter: &HistoryFilterState) {
    let mut can_create = false;
    if filter.selected_ids.len() >= 2 {
        let mut first_bench_id = None;
        can_create = true;
        for &id in &filter.selected_ids {
            if let Ok(Some(r)) = db.get_run_by_id(id) {
                if r.is_methodology { can_create = false; break; }
                if let Some(ref f_id) = first_bench_id {
                    if *f_id != r.benchmark_id {
                        can_create = false;
                        break;
                    }
                } else {
                    first_bench_id = Some(r.benchmark_id.clone());
                }
            } else {
                can_create = false;
                break;
            }
        }
    }
    ui.set_can_create_methodology(can_create);

    match db.get_filtered_history(
        &filter.search,
        &filter.category,
        &filter.gpu_mode,
        &filter.status,
        filter.sort_order,
    ) {
        Ok(runs) => {
            let mut history_items = Vec::new();
            for run in &runs {
                if run.methodology_parent_id.is_some() {
                    continue;
                }
                
                if run.is_methodology {
                    let children = db.get_methodology_children(run.id).unwrap_or_default();
                    let is_expanded = filter.expanded_methodology_ids.contains(&run.id);
                    history_items.push(format_history_item(
                        run,
                        filter.selected_ids.contains(&run.id),
                        true,
                        false,
                        is_expanded,
                        true,
                        children.len(),
                        0,
                    ));
                    if is_expanded {
                        for child in &children {
                            history_items.push(format_history_item(
                                child,
                                filter.selected_ids.contains(&child.id),
                                false,
                                true,
                                false,
                                false,
                                0,
                                1,
                            ));
                        }
                    }
                } else {
                    history_items.push(format_history_item(
                        run,
                        filter.selected_ids.contains(&run.id),
                        false,
                        false,
                        false,
                        false,
                        0,
                        0,
                    ));
                }
            }
            ui.set_history(ModelRc::from(Rc::new(VecModel::from(history_items))));
            ui.set_selected_history_count(filter.selected_ids.len() as i32);
        }
        Err(e) => {
            tracing::error!("Geçmiş sorgulanırken hata oluştu: {:#}", e);
            ui.set_status_text(
                benchhub::i18n::t("error_history_load")
                    .replace("{}", &e.to_string())
                    .into(),
            );
        }
    }
}

pub fn update_localized_models(ui: &AppWindow, lang: &str) {
    let _ = lang; // Uses i18n global state

    // GPU Modes with hardware detection and dGPU/iGPU abbreviations
    let (dgpu_opt, igpu_opt) = benchhub::telemetry::get_detected_gpu_names();
    let dgpu_label = if let Some(name) = dgpu_opt {
        format!("dGPU ({})", name)
    } else {
        "dGPU (NVIDIA)".to_string()
    };
    let igpu_label = if let Some(name) = igpu_opt {
        format!("iGPU ({})", name)
    } else {
        "iGPU".to_string()
    };
    let auto_label = "Auto".to_string();

    let gpu_mode_strings: Vec<slint::SharedString> =
        vec![dgpu_label.into(), igpu_label.into(), auto_label.into()];
    ui.set_gpu_modes(ModelRc::from(Rc::new(VecModel::from(gpu_mode_strings))));

    // History Filters
    let history_categories: Vec<slint::SharedString> = vec![
        i18n::t("cat_all").into(),
        i18n::t("cat_cpu").into(),
        i18n::t("cat_gpu").into(),
    ];
    ui.set_history_categories(ModelRc::from(Rc::new(VecModel::from(history_categories))));

    let history_gpu_modes: Vec<slint::SharedString> = vec![
        i18n::t("gpu_filter_all").into(),
        i18n::t("gpu_filter_nvidia").into(),
        i18n::t("gpu_filter_igpu").into(),
        i18n::t("gpu_filter_default").into(),
    ];
    ui.set_history_gpu_modes(ModelRc::from(Rc::new(VecModel::from(history_gpu_modes))));

    let history_statuses: Vec<slint::SharedString> = vec![
        i18n::t("status_all").into(),
        i18n::t("status_completed").into(),
        i18n::t("status_error").into(),
        i18n::t("status_cancelled").into(),
    ];
    ui.set_history_statuses(ModelRc::from(Rc::new(VecModel::from(history_statuses))));

    let history_sort_options: Vec<slint::SharedString> = vec![
        i18n::t("sort_date_desc").into(),
        i18n::t("sort_date_asc").into(),
        i18n::t("sort_score_desc").into(),
        i18n::t("sort_score_asc").into(),
        i18n::t("sort_duration_asc").into(),
    ];
    ui.set_history_sort_options(ModelRc::from(Rc::new(VecModel::from(history_sort_options))));

    // Settings Dropdowns
    let telemetry_intervals: Vec<slint::SharedString> = vec![
        i18n::t("interval_250").into(),
        i18n::t("interval_500").into(),
        i18n::t("interval_1000").into(),
        i18n::t("interval_2000").into(),
    ];
    ui.set_telemetry_intervals(ModelRc::from(Rc::new(VecModel::from(telemetry_intervals))));

    let terminal_limits: Vec<slint::SharedString> = vec![
        i18n::t("limit_80").into(),
        i18n::t("limit_150").into(),
        i18n::t("limit_300").into(),
        i18n::t("limit_500").into(),
    ];
    ui.set_terminal_limits(ModelRc::from(Rc::new(VecModel::from(terminal_limits))));

    let save_stopped_options: Vec<slint::SharedString> = vec![
        i18n::t("save_stopped_yes").into(),
        i18n::t("save_stopped_no").into(),
    ];
    ui.set_save_stopped_options(ModelRc::from(Rc::new(VecModel::from(save_stopped_options))));

    let max_durations = [0, 60, 120, 180, 300, 600];
    let max_duration_options: Vec<slint::SharedString> = max_durations
        .iter()
        .map(|&secs| benchhub::config::format_max_test_duration(secs, &i18n::get_language()).into())
        .collect();
    ui.set_max_duration_options(ModelRc::from(Rc::new(VecModel::from(max_duration_options))));

    let languages: Vec<slint::SharedString> = vec!["Türkçe".into(), "English".into()];
    ui.set_available_languages(ModelRc::from(Rc::new(VecModel::from(languages))));

    let bench_durations = [
        None,
        Some(0),
        Some(30),
        Some(60),
        Some(120),
        Some(180),
        Some(300),
        Some(600),
    ];
    let bench_duration_options: Vec<slint::SharedString> = bench_durations
        .iter()
        .map(|&opt| {
            benchhub::bench_config::format_bench_duration(opt, &i18n::get_language()).into()
        })
        .collect();
    ui.set_bench_config_duration_options(ModelRc::from(Rc::new(VecModel::from(
        bench_duration_options,
    ))));

    let ffmpeg_sources: Vec<slint::SharedString> = vec![
        i18n::t("bench_settings_source_bunny").into(),
        i18n::t("bench_settings_source_tears").into(),
        i18n::t("bench_settings_source_synthetic").into(),
    ];
    ui.set_bench_config_ffmpeg_input_sources(ModelRc::from(Rc::new(VecModel::from(
        ffmpeg_sources,
    ))));

    let ffmpeg_durations: Vec<slint::SharedString> = vec![
        i18n::t("bench_settings_dur_20s").into(),
        "10s".into(),
        "15s".into(),
        "30s".into(),
        "60s".into(),
        i18n::t("bench_settings_dur_unlimited").into(),
    ];
    ui.set_bench_config_ffmpeg_durations(ModelRc::from(Rc::new(VecModel::from(ffmpeg_durations))));

    let zip_dict_sizes: Vec<slint::SharedString> = vec![
        i18n::t("bench_settings_dict_32mb").into(),
        "64MB".into(),
        "128MB".into(),
        "256MB".into(),
        "512MB".into(),
        "1024MB (1 GB)".into(),
    ];
    ui.set_bench_config_7zip_dict_sizes(ModelRc::from(Rc::new(VecModel::from(zip_dict_sizes))));
}

pub type Settings = benchhub::config::AppSettings;

/// Updates all localized UI elements, dropdown selections, and active language bindings.
pub fn update_ui_language(ui: &AppWindow, settings: &Settings) {
    let lang_code = i18n::normalize_lang(&settings.language);
    let lang_global = ui.global::<Lang>();
    lang_global.set_current(lang_code.into());

    let (lang_display, lang_idx) = match lang_code {
        "en" => ("English", 1),
        _ => ("Türkçe", 0),
    };
    ui.set_selected_language(lang_display.into());
    ui.set_selected_language_index(lang_idx);

    update_localized_models(ui, lang_code);

    let (interval_str, interval_idx) = match settings.telemetry_interval_ms {
        250 => (i18n::t("interval_250"), 0),
        1000 => (i18n::t("interval_1000"), 2),
        2000 => (i18n::t("interval_2000"), 3),
        _ => (i18n::t("interval_500"), 1),
    };
    ui.set_selected_telemetry_interval(interval_str.into());
    ui.set_selected_telemetry_interval_index(interval_idx);

    let (limit_str, limit_idx) = match settings.terminal_buffer_lines {
        80 => (i18n::t("limit_80"), 0),
        300 => (i18n::t("limit_300"), 2),
        500 => (i18n::t("limit_500"), 3),
        _ => (i18n::t("limit_150"), 1),
    };
    ui.set_selected_terminal_limit(limit_str.into());
    ui.set_selected_terminal_limit_index(limit_idx);

    let (stopped_str, stopped_idx) = if settings.save_stopped_runs {
        (i18n::t("save_stopped_yes"), 0)
    } else {
        (i18n::t("save_stopped_no"), 1)
    };
    ui.set_selected_save_stopped(stopped_str.into());
    ui.set_selected_save_stopped_index(stopped_idx);

    let max_dur_str =
        benchhub::config::format_max_test_duration(settings.max_test_duration_secs, lang_code);
    let max_dur_idx = match settings.max_test_duration_secs {
        0 => 0,
        60 => 1,
        120 => 2,
        180 => 3,
        300 => 4,
        600 => 5,
        _ => 4,
    };
    ui.set_selected_max_duration(max_dur_str.into());
    ui.set_selected_max_duration_index(max_dur_idx);

    let initial_gpu_mode = benchhub::models::GpuMode::from_display_str(&settings.selected_gpu_mode);
    let (gpu_mode_display, gpu_mode_idx) = match initial_gpu_mode {
        benchhub::models::GpuMode::NvidiaDgpu => (i18n::t("gpu_mode_nvidia"), 0),
        benchhub::models::GpuMode::Integrated => (i18n::t("gpu_mode_igpu"), 1),
        benchhub::models::GpuMode::Auto => (i18n::t("gpu_mode_default"), 2),
    };
    ui.set_selected_gpu_mode(gpu_mode_display.into());
    ui.set_selected_gpu_mode_index(gpu_mode_idx);
}

/// Initializes and populates the main Slint UI window with stored settings, system info,
/// benchmark catalog, and history.
pub fn setup_initial_ui(
    ui: &AppWindow,
    app_state: &crate::state::AppState,
    initial_console_log: &str,
) {
    ui.window().set_maximized(true);

    let settings = app_state.settings.lock().unwrap_or_else(|e| e.into_inner());

    // Initialize Lang global
    let lang_global = ui.global::<Lang>();
    lang_global.on_t(|key, current| i18n::t_lang(&key, &current).into());

    update_ui_language(ui, &settings);

    // Populate Settings UI
    ui.set_data_dir_path(app_state.data_dir.to_string_lossy().to_string().into());
    ui.set_logs_dir_path(app_state.logs_dir.to_string_lossy().to_string().into());
    ui.set_runners_size_text(i18n::t("calculating").into());
    ui.set_sensor_diagnosis_text(i18n::t("scanning_sensors").into());

    let ui_weak_init_diag = ui.as_weak();
    let runners_dir_init = app_state.data_dir.join("runners");
    tokio::spawn(async move {
        let runners_size = crate::commands::calculate_dir_size(&runners_dir_init).await;
        let diag = benchhub::telemetry::get_sensor_diagnosis();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak_init_diag.upgrade() {
                ui.set_runners_size_text(crate::commands::format_bytes(runners_size).into());
                ui.set_sensor_diagnosis_text(diag.into());
            }
        });
    });

    // Load persistent console log
    ui.set_terminal_output(initial_console_log.into());

    // Populate catalog
    populate_initial_catalog(ui, app_state);

    // Populate history
    {
        let filter = app_state
            .history_filter_state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        refresh_history_view(ui, &app_state.service.db, &filter);
    }
}

/// Helper to retrieve localized name, category, and description for a benchmark profile.
pub fn get_localized_profile_info(
    id: &str,
    default_name: &str,
    default_category: &str,
    default_desc: &str,
) -> (String, String, String) {
    let name = match id {
        "7zip" => i18n::t("bench_7zip_name"),
        "cray" => i18n::t("bench_cray_name"),
        "geekbench" => i18n::t("bench_geekbench_name"),
        "unigine" => i18n::t("bench_unigine_name"),
        "unigine_heaven" => i18n::t("bench_unigine_heaven_name"),
        "unigine_superposition" => i18n::t("bench_unigine_superposition_name"),
        "ffmpeg" => i18n::t("bench_ffmpeg_name"),
        "blender" => i18n::t("bench_blender_name"),
        "llama-bench" => i18n::t("bench_llama_bench_name"),
        "gravitymark" => i18n::t("bench_gravitymark_name"),
        "fio" => i18n::t("bench_fio_name"),
        "stream" => i18n::t("bench_stream_name"),
        "ycruncher" => i18n::t("bench_ycruncher_name"),
        _ => default_name.to_string(),
    };

    let desc = match id {
        "7zip" => i18n::t("bench_7zip_desc"),
        "cray" => i18n::t("bench_cray_desc"),
        "geekbench" => i18n::t("bench_geekbench_desc"),
        "unigine" => i18n::t("bench_unigine_desc"),
        "unigine_heaven" => i18n::t("bench_unigine_heaven_desc"),
        "unigine_superposition" => i18n::t("bench_unigine_superposition_desc"),
        "ffmpeg" => i18n::t("bench_ffmpeg_desc"),
        "blender" => i18n::t("bench_blender_desc"),
        "llama-bench" => i18n::t("bench_llama_bench_desc"),
        "gravitymark" => i18n::t("bench_gravitymark_desc"),
        "fio" => i18n::t("bench_fio_desc"),
        "stream" => i18n::t("bench_stream_desc"),
        "ycruncher" => i18n::t("bench_ycruncher_desc"),
        _ => default_desc.to_string(),
    };

    let category = if default_category.contains("Render") {
        i18n::t("cat_render_full")
    } else if default_category.contains("Sistem") || default_category.contains("System") {
        i18n::t("cat_system_full")
    } else if default_category.contains("GPU")
        || default_category.contains("Grafik")
        || default_category.contains("Graphics")
    {
        i18n::t("cat_gpu_full")
    } else if default_category.contains("CPU")
        || default_category.contains("İşlemci")
        || default_category.contains("Processor")
    {
        i18n::t("cat_cpu_full")
    } else if default_category.contains("LLM")
        || default_category.contains("Yapay Zeka")
        || default_category.contains("AI")
    {
        i18n::t("cat_ai_full")
    } else if default_category.contains("Bellek")
        || default_category.contains("RAM")
        || default_category.contains("Memory")
    {
        i18n::t("cat_ram_full")
    } else if default_category.contains("Depolama")
        || default_category.contains("Disk")
        || default_category.contains("Storage")
    {
        i18n::t("cat_disk_full")
    } else {
        default_category.to_string()
    };

    (name, category, desc)
}

/// Populates the benchmark catalog list in Slint UI with available profiles and installed status.
pub fn populate_initial_catalog(ui: &AppWindow, app_state: &crate::state::AppState) {
    let catalog_items: Vec<BenchmarkItem> = app_state
        .service
        .profiles
        .iter()
        .map(|p| {
            let ver_strings: Vec<slint::SharedString> = p
                .versions
                .iter()
                .map(|v| v.version.clone().into())
                .collect();
            let selected_ver = p
                .default_version
                .clone()
                .or_else(|| p.versions.first().map(|v| v.version.clone()))
                .or_else(|| p.version.clone())
                .unwrap_or_default();
            let is_installed = app_state
                .service
                .engine
                .is_version_installed(&p.id, &selected_ver);
            let effective = p.for_version(&selected_ver);

            let (localized_name, localized_cat, localized_desc) = get_localized_profile_info(
                &p.id,
                &effective.name,
                &effective.category,
                effective.description.as_deref().unwrap_or_default(),
            );

            let selected_ver_idx = p
                .versions
                .iter()
                .position(|v| v.version == selected_ver)
                .unwrap_or(0) as i32;

            let has_versions = p.versions.len() > 1
                && (p.id == "7zip"
                    || p.id == "unigine"
                    || p.id == "geekbench"
                    || p.id == "blender"
                    || p.id == "ffmpeg");

            let display_ver = match p.id.as_str() {
                "cray" => "v1.1".to_string(),
                "ycruncher" => "v0.8.7".to_string(),
                "stream" => "v5.10".to_string(),
                "fio" => "v3.38".to_string(),
                "gravitymark" => "v1.89".to_string(),
                "llama-bench" => "b10729".to_string(),
                _ => {
                    if has_versions {
                        selected_ver.clone()
                    } else if selected_ver.is_empty() {
                        "v1.0".to_string()
                    } else if selected_ver.starts_with('v') || selected_ver.starts_with('b') {
                        selected_ver.clone()
                    } else if selected_ver
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
                    {
                        format!("v{}", selected_ver)
                    } else {
                        selected_ver.clone()
                    }
                }
            };

            BenchmarkItem {
                id: p.id.clone().into(),
                name: localized_name.into(),
                category: localized_cat.into(),
                download_size: effective.display_download_size().into(),
                description: localized_desc.into(),
                installed: is_installed,
                running: false,
                has_versions,
                versions: ModelRc::from(Rc::new(VecModel::from(ver_strings))),
                selected_version: selected_ver.into(),
                selected_version_index: selected_ver_idx,
                display_version: display_ver.into(),
            }
        })
        .collect();
    ui.set_catalog(ModelRc::from(Rc::new(VecModel::from(catalog_items))));
}
