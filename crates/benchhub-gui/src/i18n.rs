//! Bilingual Internationalization (i18n) Dictionary
//!
//! Provides complete UI translation lookups for English and Turkish across
//! all tabs, table headers, benchmark metadata, daemon notifications,
//! filters, settings, statuses, telemetry metrics, and modal dialogs.

use benchhub_types::{DaemonNotification, GpuMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    En,
    Tr,
}

/// Matches a language identifier or locale string (e.g. "tr_TR.UTF-8", "tr-TR", "en_US")
/// to a supported `Language`.
pub fn match_supported_language(s: &str) -> Option<Language> {
    let clean = s.trim().to_lowercase();
    let code = clean.split(['_', '-', '.', '@']).next().unwrap_or("");

    match code {
        "tr" => Some(Language::Tr),
        "en" => Some(Language::En),
        _ => {
            if clean.contains("türk") || clean.contains("turkish") {
                Some(Language::Tr)
            } else if clean.contains("english") {
                Some(Language::En)
            } else {
                None
            }
        }
    }
}

/// Detects the system language from the browser / WebKitGTK navigator environment.
/// Falls back to English when the OS language is unsupported.
pub fn detect_system_language() -> Language {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        if let Some(lang_str) = navigator.language() {
            if let Some(matched) = match_supported_language(&lang_str) {
                return matched;
            }
        }
        let languages = navigator.languages();
        for i in 0..languages.length() {
            if let Some(l) = languages.get(i).as_string() {
                if let Some(matched) = match_supported_language(&l) {
                    return matched;
                }
            }
        }
    }
    Language::En
}

impl Language {
    /// Resolves a language code or `"system"`/`"auto"` to a supported `Language`.
    pub fn from_config(s: &str) -> Self {
        let clean = s.trim().to_lowercase();
        if clean == "system" || clean == "auto" || clean.is_empty() {
            return detect_system_language();
        }
        match_supported_language(&clean).unwrap_or(Language::En)
    }
}

/// Translates a UI dictionary key for the active language.
pub fn t(lang: Language, key: &'static str) -> &'static str {
    match lang {
        Language::Tr => translate_tr(key),
        Language::En => translate_en(key),
    }
}

/// Translates a benchmark or stress test display name by its manifest ID.
pub fn bench_name<'a>(lang: Language, id: &str, fallback: &'a str) -> &'a str {
    match (lang, id) {
        (Language::Tr, "7zip") => "7-Zip Sıkıştırma Testi",
        (Language::En, "7zip") => "7-Zip Compression Benchmark",
        (Language::Tr, "blender") => "Blender Benchmark",
        (Language::En, "blender") => "Blender Benchmark",
        (Language::Tr, "cray") => "C-Ray Işın İzleme",
        (Language::En, "cray") => "C-Ray Raytracing",
        (Language::Tr, "ffmpeg") => "FFmpeg Video Kodlama",
        (Language::En, "ffmpeg") => "FFmpeg Video Encoding",
        (Language::Tr, "fio") => "FIO Depolama Testi",
        (Language::En, "fio") => "FIO Storage Benchmark",
        (Language::Tr, "furmark") => "Geeks3D FurMark GPU Stres Testi",
        (Language::En, "furmark") => "Geeks3D FurMark GPU Stress Test",
        (Language::Tr, "geekbench") => "Geekbench Sistem Testi",
        (Language::En, "geekbench") => "Geekbench",
        (Language::Tr, "gravitymark") => "GravityMark GPU Benchmark",
        (Language::En, "gravitymark") => "GravityMark GPU Benchmark",
        (Language::Tr, "llama-bench") => "llama-bench Yapay Zeka Testi",
        (Language::En, "llama-bench") => "llama-bench",
        (Language::Tr, "stream") => "STREAM Bellek Bant Genişliği",
        (Language::En, "stream") => "STREAM Memory Bandwidth",
        (Language::Tr, "unigine") => "Unigine Superposition Benchmark",
        (Language::En, "unigine") => "Unigine Superposition Benchmark",
        (Language::Tr, "ycruncher") => "y-cruncher Pi & İşlemci Testi",
        (Language::En, "ycruncher") => "y-cruncher",
        _ => fallback,
    }
}

/// Translates a benchmark or stress test description by its manifest ID.
pub fn bench_desc<'a>(lang: Language, id: &str, fallback: &'a str) -> &'a str {
    match (lang, id) {
        (Language::Tr, "7zip") => {
            "Çok çekirdekli LZMA/LZMA2 sıkıştırma ve açma işlemci performans testi."
        }
        (Language::En, "7zip") => {
            "Multi-threaded LZMA/LZMA2 compression and decompression CPU benchmark."
        }
        (Language::Tr, "blender") => {
            "Açık kaynaklı Blender 3D motoru ile CPU ve GPU sahne işleme (render) testi."
        }
        (Language::En, "blender") => {
            "CPU and GPU 3D rendering benchmark powered by the open-source Blender engine."
        }
        (Language::Tr, "cray") => {
            "Saf C ile yazılmış çok kanallı ışın izleme (raytracing) render testi."
        }
        (Language::En, "cray") => {
            "Multi-threaded raytracing rendering benchmark written in pure C."
        }
        (Language::Tr, "ffmpeg") => {
            "Çok kanallı H.264, H.265 ve AV1 video dönüştürme ve kodlama performans testi."
        }
        (Language::En, "ffmpeg") => {
            "Multi-threaded H.264, H.265, and AV1 video transcoding and encoding benchmark."
        }
        (Language::Tr, "fio") => {
            "NVMe, SSD ve HDD okuma/yazma hızlarını ölçen endüstri standardı G/Ç (I/O) testi."
        }
        (Language::En, "fio") => {
            "Industry-standard storage I/O benchmark measuring read and write performance across NVMe, SSD, and HDD."
        }
        (Language::Tr, "furmark") => {
            "Endüstri standardı OpenGL ve Vulkan tabanlı GPU termal ve kararlılık stres testi."
        }
        (Language::En, "furmark") => {
            "Industry-standard OpenGL and Vulkan GPU thermal and stability stress test."
        }
        (Language::Tr, "geekbench") => {
            "Gerçek dünya iş yükleriyle tek ve çok çekirdekli sistem performansını ölçer."
        }
        (Language::En, "geekbench") => {
            "Measures single-core and multi-core system performance across real-world workloads."
        }
        (Language::Tr, "gravitymark") => {
            "Vulkan ve OpenGL API'lerini kullanan çapraz platform GPU ve ışın izleme testi."
        }
        (Language::En, "gravitymark") => {
            "Cross-API GPU benchmark and raytracing stress test using Vulkan and OpenGL."
        }
        (Language::Tr, "llama-bench") => {
            "llama.cpp tabanlı yapay zeka (LLM) saniyede token üretme hız testi (CPU & GPU)."
        }
        (Language::En, "llama-bench") => {
            "LLM inference throughput benchmark based on llama.cpp measuring tokens per second on CPU and GPU."
        }
        (Language::Tr, "stream") => {
            "Sürdürülebilir bellek bant genişliği ölçüm testi (Triad, Copy, Scale, Add)."
        }
        (Language::En, "stream") => {
            "Sustainable memory bandwidth benchmark (Triad, Copy, Scale, Add)."
        }
        (Language::Tr, "unigine") => {
            "Unigine 3D motoru tabanlı dinamik aydınlatma, SSRTGI ve VRAM grafik testi."
        }
        (Language::En, "unigine") => {
            "Dynamic lighting, SSRTGI, and VRAM benchmark powered by the Unigine 3D engine."
        }
        (Language::Tr, "ycruncher") => {
            "AVX2/AVX512 komut setleriyle yüksek hassasiyetli Pi ve sabit hesaplama testi."
        }
        (Language::En, "ycruncher") => {
            "High-precision Pi and mathematical constant stress benchmark (AVX2/AVX512)."
        }
        _ => fallback,
    }
}

/// Translates a canonical category tag into the active UI language.
pub fn translate_category(lang: Language, category: &str) -> String {
    match (lang, category) {
        (Language::Tr, "CPU") => "İşlemci (CPU)".to_string(),
        (Language::En, "CPU") => "CPU".to_string(),
        (Language::Tr, "CPU Render") => "İşlemci / Render".to_string(),
        (Language::En, "CPU Render") => "CPU Render".to_string(),
        (Language::Tr, "Render") => "Render / Işın İzleme".to_string(),
        (Language::En, "Render") => "Render".to_string(),
        (Language::Tr, "Storage") => "Depolama".to_string(),
        (Language::En, "Storage") => "Storage".to_string(),
        (Language::Tr, "GPU") => "Ekran Kartı (GPU)".to_string(),
        (Language::En, "GPU") => "GPU".to_string(),
        (Language::Tr, "GPU Stress") => "GPU Stres Testi".to_string(),
        (Language::En, "GPU Stress") => "GPU Stress".to_string(),
        (Language::Tr, "System & CPU") => "Sistem ve İşlemci".to_string(),
        (Language::En, "System & CPU") => "System & CPU".to_string(),
        (Language::Tr, "AI") => "Yapay Zeka (AI)".to_string(),
        (Language::En, "AI") => "AI".to_string(),
        (Language::Tr, "Memory") => "Bellek (RAM)".to_string(),
        (Language::En, "Memory") => "Memory".to_string(),
        _ => category.to_string(),
    }
}

/// Translates a canonical score unit string.
pub fn translate_score_unit(lang: Language, unit: &str) -> &str {
    match (lang, unit) {
        (Language::Tr, "Points") => "Puan",
        (Language::En, "Points") => "Points",
        (Language::Tr, "samples/min") => "örnek/dk",
        (Language::En, "samples/min") => "samples/min",
        (Language::Tr, "ms (lower is better)") => "ms (düşük daha iyi)",
        (Language::En, "ms (lower is better)") => "ms (lower is better)",
        (Language::Tr, "s (lower is better)") => "sn (düşük daha iyi)",
        (Language::En, "s (lower is better)") => "s (lower is better)",
        (Language::Tr, "token/s") => "token/sn",
        (Language::En, "token/s") => "token/s",
        _ => unit,
    }
}

/// Translates a stress test API/variant label.
pub fn translate_stress_version(lang: Language, version: &str) -> String {
    match (lang, version) {
        (Language::Tr, "vulkan") => "Vulkan (furmark-vk)".to_string(),
        (Language::En, "vulkan") => "Vulkan (furmark-vk)".to_string(),
        (Language::Tr, "opengl") => "OpenGL (furmark-gl)".to_string(),
        (Language::En, "opengl") => "OpenGL (furmark-gl)".to_string(),
        (Language::Tr, "knot_vk") => "Vulkan Düğüm (furmark-knot-vk)".to_string(),
        (Language::En, "knot_vk") => "Vulkan Knot (furmark-knot-vk)".to_string(),
        _ => version.to_string(),
    }
}

/// Translates a GPU execution mode string or enum into the active UI language.
pub fn translate_gpu_mode(lang: Language, raw: &str) -> &'static str {
    let lower = raw.to_lowercase();
    if lower == "mixed" {
        t(lang, "gpu_mode_mixed")
    } else if lower.contains("nvidia") || lower.contains("dgpu") || lower.contains("dedicated") {
        t(lang, "gpu_nvidia")
    } else if lower.contains("igpu") || lower.contains("integrated") {
        t(lang, "gpu_integrated")
    } else {
        t(lang, "gpu_auto")
    }
}

/// Translates a hardware throttling reason string into the active UI language.
pub fn translate_throttle_status(lang: Language, raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    if lower.is_empty() || lower == "none" || lower == "no" || lower == "-" {
        t(lang, "throttle_status_none").to_string()
    } else if lower.contains("power") {
        t(lang, "throttle_status_power").to_string()
    } else if lower.contains("thermal") || lower.contains("temp") {
        t(lang, "throttle_status_thermal").to_string()
    } else {
        raw.to_string()
    }
}

/// Translates a typed `GpuMode` value into the active UI language.
pub fn translate_gpu_mode_enum(lang: Language, mode: GpuMode) -> &'static str {
    match mode {
        GpuMode::NvidiaDgpu => t(lang, "gpu_nvidia"),
        GpuMode::Integrated => t(lang, "gpu_integrated"),
        GpuMode::Auto => t(lang, "gpu_auto"),
    }
}

/// Translates a normalized run status string into the active UI language.
pub fn translate_status(lang: Language, status: &str) -> &'static str {
    match benchhub_types::normalize_run_status(status) {
        "stopped" | "cancelled" => t(lang, "status_cancelled"),
        "timeout" | "timeout_scored" => t(lang, "status_timeout"),
        "failed" => t(lang, "status_failed"),
        _ => t(lang, "status_completed"),
    }
}

/// Formats a structured daemon notification into a localized message and error flag.
pub fn translate_notification(lang: Language, notif: &DaemonNotification) -> (String, bool) {
    match notif {
        DaemonNotification::BenchmarkCompleted {
            benchmark_id,
            version,
            score,
            status,
        } => {
            let name = bench_name(lang, benchmark_id, benchmark_id);
            let status_txt = translate_status(lang, status);
            let score_part = score
                .map(|s| format!("{:.2}", s))
                .unwrap_or_else(|| "-".to_string());
            let msg = match lang {
                Language::Tr => format!(
                    "{} ({}) {}: {} ({})",
                    name, version, status_txt, score_part, benchmark_id
                ),
                Language::En => format!(
                    "{} ({}) {}: {} ({})",
                    name, version, status_txt, score_part, benchmark_id
                ),
            };
            (msg, status == "failed")
        }
        DaemonNotification::InstallCompleted {
            benchmark_id,
            version,
        } => {
            let name = bench_name(lang, benchmark_id, benchmark_id);
            let msg = match lang {
                Language::Tr => format!("{} ({}) kurulumu tamamlandı.", name, version),
                Language::En => format!("{} ({}) installed successfully.", name, version),
            };
            (msg, false)
        }
        DaemonNotification::UninstallCompleted {
            benchmark_id,
            version,
        } => {
            let name = bench_name(lang, benchmark_id, benchmark_id);
            let msg = match lang {
                Language::Tr => format!("{} ({}) kaldırıldı.", name, version),
                Language::En => format!("{} ({}) uninstalled.", name, version),
            };
            (msg, false)
        }
        DaemonNotification::TaskStopped { benchmark_id } => {
            let name = bench_name(lang, benchmark_id, benchmark_id);
            let msg = match lang {
                Language::Tr => format!("{} testi kullanıcı tarafından durduruldu.", name),
                Language::En => format!("{} run stopped by user.", name),
            };
            (msg, false)
        }
        DaemonNotification::TaskFailed {
            benchmark_id,
            reason,
        } => {
            let name = bench_name(lang, benchmark_id, benchmark_id);
            let msg = match lang {
                Language::Tr => format!("{} başarısız oldu: {}", name, reason),
                Language::En => format!("{} failed: {}", name, reason),
            };
            (msg, true)
        }
        DaemonNotification::Enqueued {
            benchmark_id,
            version,
        } => {
            let name = bench_name(lang, benchmark_id, benchmark_id);
            let msg = match lang {
                Language::Tr => format!("{} ({}) test kuyruğuna eklendi.", name, version),
                Language::En => format!("{} ({}) added to benchmark queue.", name, version),
            };
            (msg, false)
        }
        DaemonNotification::QueueCompleted {
            total,
            completed,
            failed,
        } => {
            let msg = match lang {
                Language::Tr => format!(
                    "Kuyruk tamamlandı: {}/{} başarılı, {} başarısız.",
                    completed, total, failed
                ),
                Language::En => format!(
                    "Queue completed: {}/{} succeeded, {} failed.",
                    completed, total, failed
                ),
            };
            (msg, *failed > 0 && *completed == 0)
        }
        DaemonNotification::AlreadyRunning => (t(lang, "notif_already_running").to_string(), true),
        DaemonNotification::ExportCompleted { path } => {
            let msg = match lang {
                Language::Tr => format!("Dışa aktarma tamamlandı: {}", path),
                Language::En => format!("Export completed: {}", path),
            };
            (msg, false)
        }
    }
}

fn translate_en(key: &'static str) -> &'static str {
    match key {
        // Navigation / Header
        "app_title" => "BenchHub",
        "app_subtitle" => "Linux Benchmark Runner & Hardware Telemetry",
        "tab_benchmarks" => "Benchmarks",
        "tab_stress" => "Stress Tests",
        "tab_monitor" => "Monitor",
        "tab_history" => "History",
        "tab_settings" => "Settings",
        "status_ready" => "Ready",
        "status_running" => "Running Test...",
        "status_installing" => "Installing...",
        "status_cancelled" => "Cancelled",
        "status_completed" => "Completed",
        "status_timeout" => "Timed Out",
        "status_failed" => "Failed",
        "status_error" => "Error",
        "cancel_active" => "Stop Task",
        "notif_already_running" => "Another installation or benchmark is already active.",
        "dismiss_toast" => "Dismiss",

        // Catalog Tab
        "catalog_title" => "Benchmark Catalog",
        "catalog_subtitle" => "Discover, configure, and execute standard Linux benchmarks",
        "search_benchmarks" => "Search benchmarks...",
        "all_categories" => "All Categories",
        "category_cpu" => "CPU",
        "category_gpu" => "GPU",
        "category_render" => "Render / Encoding",
        "category_disk" => "Storage / Disk",
        "category_memory" => "Memory / RAM",
        "category_ai" => "AI / LLM",
        "category_system" => "System",
        "version_label" => "Version:",
        "approx_size" => "Download Size:",
        "score_unit_label" => "Metric:",
        "install_btn" => "Install",
        "uninstall_btn" => "Uninstall",
        "run_btn" => "Run Benchmark",
        "custom_config_btn" => "Custom Args",
        "queue_btn" => "Add to Queue",
        "select_all_btn" => "Select All (Ctrl+A)",
        "deselect_all_btn" => "Deselect All (Ctrl+A)",
        "select_all_tooltip" => "Select or deselect all installed visible benchmarks (Ctrl+A)",
        "queued_selected_badge" => "Selected",
        "install_before_queue_msg" => "Please install this benchmark before adding it to the queue.",
        "no_installed_to_select_msg" => "No installed benchmarks are available to select in the current view.",
        "in_queue_badge" => "queued",
        "run_queue_btn" => "Run Queue",
        "running_queue_btn" => "Running Queue...",
        "clear_queue_btn" => "Clear",
        "no_benchmarks_found" => "No benchmarks found matching the criteria.",
        "gpu_mode_label" => "GPU Execution Mode:",
        "gpu_nvidia" => "Dedicated GPU (NVIDIA)",
        "gpu_integrated" => "Integrated GPU (iGPU)",
        "gpu_auto" => "System Default (Auto)",
        "gpu_mode_mixed" => "Mixed",

        // Stress Tests Tab
        "stress_title" => "System Stress Tests",
        "stress_subtitle" => "System stability verification, thermal saturation, and burn-in load testing",
        "search_stress" => "Search stress tests...",
        "stress_duration_label" => "Target Duration:",
        "stress_duration_1m" => "1 minute (Quick Check)",
        "stress_duration_5m" => "5 minutes (Thermal Check)",
        "stress_duration_15m" => "15 minutes (Standard Saturation)",
        "stress_duration_30m" => "30 minutes (Extended Burn-in)",
        "stress_duration_60m" => "60 minutes (Endurance Test)",
        "stress_duration_unlimited" => "Continuous (Manual Stop)",
        "run_stress_btn" => "Run Stress Test",
        "stress_empty_msg" => "No active stress test runners configured yet. Standard performance benchmarks are available under the Benchmarks tab.",
        "stress_select_all_tooltip" => "Select or deselect all installed stress tests (Ctrl+A)",
        "no_stress_to_select_msg" => "No installed stress tests are available to select.",
        "install_stress_before_queue_msg" => "Please install this stress test before adding it to the queue.",

        // Monitor Tab
        "monitor_title" => "Hardware Telemetry & Live Console",
        "monitor_subtitle" => "Real-time thermal metrics, clock speeds, power draw, and execution logs",
        "telemetry_cpu_temp" => "CPU Temperature",
        "telemetry_gpu_temp" => "GPU Temperature",
        "telemetry_cpu_freq" => "CPU Frequency",
        "telemetry_gpu_freq" => "GPU Frequency",
        "telemetry_cpu_usage" => "CPU Usage",
        "telemetry_gpu_usage" => "GPU Usage",
        "telemetry_power" => "System Power Draw",
        "telemetry_gpu_power" => "GPU Power Draw",
        "telemetry_ram" => "RAM Usage",
        "telemetry_vram" => "VRAM Usage",
        "telemetry_throttle" => "Thermal Throttling",
        "throttle_none" => "Normal (No Throttling)",
        "throttle_active" => "Active Throttling!",
        "load_label" => "Load:",
        "gpu_short_label" => "GPU:",
        "power_profile_label" => "Power Profile:",
        "sensors_label" => "Sensors:",
        "terminal_title" => "Live Execution Logs",
        "lines_label" => "lines",
        "terminal_follow_btn" => "Follow Live",
        "elapsed_label" => "Elapsed:",
        "copy_logs" => "Copy Logs",
        "clear_logs" => "Clear Console",
        "copied" => "Copied to clipboard!",
        "no_logs_yet" => "No logs generated yet. Run a benchmark or installation to see live output.",

        // History Tab
        "history_title" => "Benchmark History & Results",
        "history_subtitle" => "Analyze past test runs, compare hardware configurations, and export reports",
        "search_history" => "Filter runs by name, system, or ID...",
        "gpu_filter_all" => "GPU: All",
        "status_filter_all" => "Status: All",
        "sort_date_desc" => "Date (Newest First)",
        "sort_date_asc" => "Date (Oldest First)",
        "sort_score_desc" => "Score (Highest First)",
        "sort_score_asc" => "Score (Lowest First)",
        "sort_cpu_temp" => "CPU Temp (Peak)",
        "sort_gpu_temp" => "GPU Temp (Peak)",
        "sort_power" => "Power Draw (Avg)",
        "action_methodology" => "Create Methodology",
        "action_compare" => "Compare Runs (2)",
        "action_export_csv" => "Export CSV",
        "action_export_png" => "Export Chart PNG",
        "action_group" => "Assign Group",
        "action_delete" => "Delete Selected",
        "action_clear_all" => "Clear All History",
        "col_id" => "ID",
        "col_benchmark" => "Benchmark",
        "col_version" => "Version",
        "col_gpu" => "GPU Mode",
        "col_status" => "Status",
        "col_score" => "Score",
        "col_duration" => "Duration",
        "col_cpu_temp" => "CPU Temp",
        "col_gpu_temp" => "GPU Temp",
        "col_power" => "Power",
        "col_group" => "Group",
        "col_date" => "Date",
        "col_actions" => "Actions",
        "delete_run_tooltip" => "Delete run",
        "details_btn" => "Details",
        "details_run_tooltip" => "Show full telemetry and run details",
        "history_select_all_tooltip" => "Select or deselect all visible history runs (Ctrl+A)",
        "no_history_to_select_msg" => "No history runs are available to select.",
        "history_row_selected_badge" => "Selected",
        "no_history_records" => "No history records found matching your filters.",
        "runs_selected" => "runs selected",
        "methodology_badge" => "Methodology (Avg)",
        "clear_history_confirm_title" => "Clear History",
        "clear_history_confirm_msg" => "Are you sure you want to delete ALL benchmark runs from the database?",
        "history_cleared_msg" => "History cleared successfully.",
        "selected_deleted_msg" => "Selected runs deleted.",

        // Settings Tab
        "settings_title" => "Application Settings",
        "settings_subtitle" => "Configure runner preferences, thermal targets, directories, and appearance",
        "section_general" => "General & Execution Preferences",
        "language_label" => "Application Language",
        "language_system" => "System Default (Auto-detect)",
        "language_desc" => "Select interface translation language or follow system locale",
        "telemetry_interval_label" => "Telemetry Polling Interval",
        "telemetry_interval_desc" => "Background sampling frequency for CPU/GPU temperature, clocks and power",
        "interval_250" => "250 ms (High Precision)",
        "interval_500" => "500 ms (Standard)",
        "interval_1000" => "1000 ms (Low Overhead)",
        "interval_2000" => "2000 ms (Minimal)",
        "disable_telemetry_during_runs_label" => "Disable telemetry during benchmark runs",
        "disable_telemetry_during_runs_desc" => "Pause hardware sensor polling while a benchmark or stress test is running to eliminate measurement overhead (off-tab idle polling is already throttled)",
        "console_buffer_label" => "Terminal Console Buffer Limit (lines)",
        "console_buffer_desc" => "Maximum lines retained in the live terminal log buffer (50 - 1000)",
        "max_duration_label" => "Maximum Benchmark Duration (seconds)",
        "max_duration_desc" => "Automatic timeout to stop runaway or hung benchmark processes",
        "duration_0" => "Unlimited (No Timeout)",
        "duration_60" => "60 seconds (1 min)",
        "duration_120" => "120 seconds (2 mins)",
        "duration_180" => "180 seconds (3 mins)",
        "duration_300" => "300 seconds (5 mins)",
        "duration_600" => "600 seconds (10 mins)",
        "save_stopped_runs_label" => "Save records for cancelled / stopped runs",
        "save_stopped_runs_desc" => "Preserve telemetry logs and partial metrics when tests are manually cancelled",
        "section_thermal" => "Thermal Management & Guardrails",
        "thermal_mode_label" => "Thermal Strategy Mode",
        "thermal_mode_desc" => "Enforce pre-run temperature conditions to ensure reproducible results",
        "thermal_mode_off" => "Disabled (Run immediately)",
        "thermal_mode_cooldown" => "Cooldown Guard (Wait until cool before run)",
        "thermal_mode_warmup" => "Warmup Guard (Pre-heat to target temperature)",
        "thermal_mode_smart" => "Smart Thermal Window (Stabilize within range)",
        "cooldown_temp_label" => "Cooldown Target Temperature (°C)",
        "cooldown_temp_desc" => "Target temperature the system must cool down to before benchmark starts",
        "warmup_temp_label" => "Warmup Target Temperature (°C)",
        "warmup_temp_desc" => "Target baseline temperature for pre-heating runs",
        "thermal_timeout_label" => "Thermal Wait Timeout (seconds)",
        "thermal_timeout_desc" => "Maximum time to wait for target thermal state before proceeding anyway",
        "temp_40" => "40 °C",
        "temp_45" => "45 °C",
        "temp_50_std" => "50 °C (Standard)",
        "temp_55" => "55 °C",
        "temp_60" => "60 °C",
        "temp_65_std" => "65 °C (Standard)",
        "temp_70" => "70 °C",
        "temp_75" => "75 °C",
        "secs_15" => "15 s",
        "secs_30_std" => "30 s (Standard)",
        "secs_45" => "45 s",
        "secs_60" => "60 s",
        "secs_120" => "120 s (2 mins)",
        "section_directories" => "Directories & Paths",
        "runners_dir_label" => "Custom Runners Directory (Optional)",
        "runners_dir_desc" => "Custom directory path containing external runner binaries or scripts",
        "section_appearance" => "Theme & Desktop Adaptive UI",
        "theme_mode_label" => "Theme Polarity Mode",
        "theme_mode_desc" => "Choose between desktop auto-sync, forced dark, forced light, or custom color palette",
        "theme_system" => "Desktop System Theme (Live Auto-Sync)",
        "theme_dark" => "Dark Theme (Force Dark)",
        "theme_light" => "Light Theme (Force Light)",
        "theme_custom" => "Custom Palette (User Overrides)",
        "accent_color_label" => "Accent Color",
        "window_bg_label" => "Window Surface Background",
        "card_bg_label" => "Card / Panel Surface Background",
        "text_color_label" => "Primary Text Color",
        "ui_scale_label" => "UI Scale Factor",
        "ui_scale_desc" => "Scale interface elements and typography (90% to 150%)",
        "scale_90" => "90% (Compact)",
        "scale_100" => "100% (Default)",
        "scale_110" => "110% (Comfortable)",
        "scale_125" => "125% (Large)",
        "scale_150" => "150% (High-DPI)",
        "detected_desktop" => "Detected Desktop:",
        "save_settings_btn" => "Save Settings",
        "settings_saved_msg" => "Settings saved successfully.",

        // Dialogs & Modals
        "modal_eula_title" => "Install Benchmark Package",
        "modal_eula_msg" => "You are about to download and install this benchmark on your system. Please review the license terms below.",
        "modal_eula_terms" => "License & Terms Notice",
        "modal_eula_loading" => "Loading license terms...",
        "modal_eula_fetch_error" => "Failed to load license terms from daemon:",
        "modal_eula_disclaimer" => "BenchHub downloads third-party tools directly from their upstream sources and acts solely as an independent launcher/runner at your own risk. Commercial or enterprise usage of proprietary benchmarks may require a separate vendor license.",
        "modal_eula_accept_check" => "I have read and accept the third-party license / EULA terms above, and confirm my usage complies with their restrictions.",
        "modal_eula_confirm" => "Accept, Download & Install",
        "modal_eula_cancel" => "Cancel",
        "modal_eula_install_logs" => "Installation Log (Live)",
        "modal_eula_watch_monitor" => "Watch in Monitor",
        "modal_eula_close" => "Close",
        "modal_eula_status_installing" => "Downloading and installing...",
        "modal_eula_status_success" => "Installation Completed",
        "modal_eula_status_failed" => "Installation Failed",
        "modal_eula_status_cancelled" => "Installation Stopped",
        "modal_args_title" => "Custom Execution Arguments",
        "modal_args_desc" => "Pass extra command-line flags to this benchmark. Dangerous flags (-o, --output, -y), absolute root paths ('/'), and shell operators are blocked for safety.",
        "modal_args_placeholder" => "e.g. --threads 8 --preset fast",
        "modal_args_apply" => "Apply & Save",
        "modal_args_forbidden" => "Forbidden argument:",
        "custom_args_saved_msg" => "Custom arguments updated.",
        "modal_compare_title" => "Side-by-Side Run Comparison",
        "modal_compare_metric" => "Metric / Telemetry",
        "modal_compare_run" => "Run",
        "modal_compare_diff" => "Delta / Difference",
        "modal_compare_peak" => "(Peak)",
        "modal_compare_avg" => "(Avg)",
        "modal_ok_btn" => "OK",
        "modal_group_title" => "Assign Runs to Group",
        "modal_group_desc" => "Organize runs by assigning them to a benchmark group or test scenario name.",
        "modal_group_name" => "Group Name",
        "modal_group_placeholder" => "e.g. Baseline-v1, Undervolt, Performance-Mode",
        "modal_group_assign" => "Assign Group",
        "modal_group_remove" => "Remove from Group",
        "modal_confirm_yes" => "Yes, Proceed",
        "modal_confirm_no" => "Cancel",
        "modal_details_title" => "Run Telemetry & Details",
        "modal_details_score_save" => "Save",
        "modal_details_score_saved" => "Run score updated.",
        "modal_details_score_invalid" => "Please enter a valid numeric score (or leave blank to clear).",
        "modal_details_cpu_section" => "CPU Metrics",
        "modal_details_gpu_section" => "GPU Metrics",
        "modal_details_mem_section" => "Memory Metrics",
        "modal_details_other_section" => "System & Power",
        "modal_details_avg_usage" => "Avg Load",
        "modal_details_peak_usage" => "Peak Load",
        "modal_details_avg_temp" => "Avg Temp",
        "modal_details_peak_temp" => "Peak Temp",
        "modal_details_avg_freq" => "Avg Clock",
        "modal_details_peak_freq" => "Peak Clock",
        "modal_details_avg_sys_power" => "Avg Sys Power",
        "modal_details_peak_sys_power" => "Peak Sys Power",
        "modal_details_avg_gpu_power" => "Avg GPU Power",
        "modal_details_peak_gpu_power" => "Peak GPU Power",
        "modal_details_avg_ac_power" => "Avg AC Power",
        "modal_details_peak_ac_power" => "Peak AC Power",
        "modal_details_cpu_throttle" => "CPU Throttling",
        "modal_details_gpu_throttle" => "GPU Throttling",
        "modal_details_ram_avg" => "Avg RAM",
        "modal_details_ram_peak" => "Peak RAM",
        "modal_details_vram_avg" => "Avg VRAM",
        "modal_details_vram_peak" => "Peak VRAM",
        "modal_details_vram_freq_avg" => "Avg VRAM Clock",
        "modal_details_vram_freq_peak" => "Peak VRAM Clock",
        "modal_details_power_profile" => "Power Profile",
        "modal_details_category" => "Category",
        "modal_details_system_info" => "System Hardware",
        "modal_details_children_title" => "Included Runs in Methodology",
        "modal_details_run_log" => "Saved Execution Log",
        "modal_details_show_log" => "Show Log",
        "modal_details_hide_log" => "Hide Log",
        "modal_details_loading_log" => "Loading log file...",
        "modal_details_no_log" => "No saved log output found for this run.",
        "modal_details_detected_links" => "Detected Links",
        "modal_details_date_label" => "Recorded",
        "throttle_status_none" => "None",
        "throttle_status_thermal" => "Thermal Throttling",
        "throttle_status_power" => "Power Limit",

        // Units and fallbacks
        "unit_celsius" => "°C",
        "unit_mhz" => "MHz",
        "unit_ghz" => "GHz",
        "unit_watt" => "W",
        "unit_mb" => "MB",
        "unit_gb" => "GB",
        "unit_secs" => "s",

        // Fallback
        _ => key,
    }
}

fn translate_tr(key: &'static str) -> &'static str {
    match key {
        // Navigation / Header
        "app_title" => "BenchHub",
        "app_subtitle" => "Linux Benchmark Çalıştırıcı ve Donanım Telemetrisi",
        "tab_benchmarks" => "Benchmarklar",
        "tab_stress" => "Stres Testleri",
        "tab_monitor" => "Monitör",
        "tab_history" => "Geçmiş",
        "tab_settings" => "Ayarlar",
        "status_ready" => "Hazır",
        "status_running" => "Test Çalışıyor...",
        "status_installing" => "Yükleniyor...",
        "status_cancelled" => "İptal Edildi",
        "status_completed" => "Tamamlandı",
        "status_timeout" => "Zaman Aşımı",
        "status_failed" => "Başarısız",
        "status_error" => "Hata",
        "cancel_active" => "Görevi Durdur",
        "notif_already_running" => "Şu anda başka bir kurulum veya test zaten çalışıyor.",
        "dismiss_toast" => "Kapat",

        // Catalog Tab
        "catalog_title" => "Benchmark Kataloğu",
        "catalog_subtitle" => "Standart Linux benchmarklarını keşfedin, yapılandırın ve çalıştırın",
        "search_benchmarks" => "Benchmark ara...",
        "all_categories" => "Tüm Kategoriler",
        "category_cpu" => "İşlemci (CPU)",
        "category_gpu" => "Ekran Kartı (GPU)",
        "category_render" => "Render / Kodlama",
        "category_disk" => "Depolama / Disk",
        "category_memory" => "Bellek / RAM",
        "category_ai" => "Yapay Zeka / LLM",
        "category_system" => "Sistem",
        "version_label" => "Sürüm:",
        "approx_size" => "İndirme Boyutu:",
        "score_unit_label" => "Ölçüt:",
        "install_btn" => "Yükle",
        "uninstall_btn" => "Kaldır",
        "run_btn" => "Benchmarkı Çalıştır",
        "custom_config_btn" => "Özel Parametreler",
        "queue_btn" => "Kuyruğa Ekle",
        "select_all_btn" => "Tümünü Seç (Ctrl+A)",
        "deselect_all_btn" => "Seçimi Kaldır (Ctrl+A)",
        "select_all_tooltip" => "Kurulu tüm görünür testleri seçer veya seçimi kaldırır (Ctrl+A)",
        "queued_selected_badge" => "Seçili",
        "install_before_queue_msg" => "Kuyruğa eklemek için önce bu benchmarkı yükleyin.",
        "no_installed_to_select_msg" => "Mevcut görünümde seçilebilecek kurulu benchmark yok.",
        "in_queue_badge" => "kuyrukta",
        "run_queue_btn" => "Kuyruğu Başlat",
        "running_queue_btn" => "Kuyruk Çalışıyor...",
        "clear_queue_btn" => "Temizle",
        "no_benchmarks_found" => "Kriterlere uygun benchmark bulunamadı.",
        "gpu_mode_label" => "GPU Çalıştırma Modu:",
        "gpu_nvidia" => "Harici GPU (NVIDIA)",
        "gpu_integrated" => "Dahili GPU (iGPU)",
        "gpu_auto" => "Sistem Varsayılanı (Otomatik)",
        "gpu_mode_mixed" => "Karma",

        // Stress Tests Tab
        "stress_title" => "Sistem Stres Testleri",
        "stress_subtitle" => "Sistem kararlılığı doğrulama, termal doygunluk ve sürekli yük testleri",
        "search_stress" => "Stres testlerinde ara...",
        "stress_duration_label" => "Hedef Süre:",
        "stress_duration_1m" => "1 dakika (Hızlı Kontrol)",
        "stress_duration_5m" => "5 dakika (Termal Kontrol)",
        "stress_duration_15m" => "15 dakika (Standart Doygunluk)",
        "stress_duration_30m" => "30 dakika (Genişletilmiş Yük Testi)",
        "stress_duration_60m" => "60 dakika (Dayanıklılık Testi)",
        "stress_duration_unlimited" => "Süresiz (Manuel Durdurma)",
        "run_stress_btn" => "Stres Testini Başlat",
        "stress_empty_msg" => "Henüz aktif stres testi çalıştırıcısı yapılandırılmadı. Standart performans testleri Benchmarklar sekmesinde yer almaktadır.",
        "stress_select_all_tooltip" => "Kurulu tüm stres testlerini seçer veya seçimi kaldırır (Ctrl+A)",
        "no_stress_to_select_msg" => "Seçilebilecek kurulu stres testi yok.",
        "install_stress_before_queue_msg" => "Kuyruğa eklemek için önce bu stres testini yükleyin.",

        // Monitor Tab
        "monitor_title" => "Donanım Telemetrisi ve Canlı Konsol",
        "monitor_subtitle" => "Gerçek zamanlı sıcaklık, saat hızları, güç tüketimi ve çalışma günlükleri",
        "telemetry_cpu_temp" => "CPU Sıcaklığı",
        "telemetry_gpu_temp" => "GPU Sıcaklığı",
        "telemetry_cpu_freq" => "CPU Frekansı",
        "telemetry_gpu_freq" => "GPU Frekansı",
        "telemetry_cpu_usage" => "CPU Kullanımı",
        "telemetry_gpu_usage" => "GPU Kullanımı",
        "telemetry_power" => "Sistem Güç Tüketimi",
        "telemetry_gpu_power" => "GPU Güç Tüketimi",
        "telemetry_ram" => "RAM Kullanımı",
        "telemetry_vram" => "VRAM Kullanımı",
        "telemetry_throttle" => "Termal Kısılma (Throttling)",
        "throttle_none" => "Normal (Kısılma Yok)",
        "throttle_active" => "Kısılma Aktif!",
        "load_label" => "Yük:",
        "gpu_short_label" => "GPU:",
        "power_profile_label" => "Güç Profili:",
        "sensors_label" => "Sensörler:",
        "terminal_title" => "Canlı Çalışma Günlüğü",
        "lines_label" => "satır",
        "terminal_follow_btn" => "Canlı Takip",
        "elapsed_label" => "Geçen Süre:",
        "copy_logs" => "Günlükleri Kopyala",
        "clear_logs" => "Konsolu Temizle",
        "copied" => "Panoya kopyalandı!",
        "no_logs_yet" => "Henüz günlük üretilmedi. Çıktıyı görmek için bir test veya kurulum başlatın.",

        // History Tab
        "history_title" => "Benchmark Geçmişi ve Sonuçlar",
        "history_subtitle" => "Geçmiş testleri inceleyin, donanım yapılandırmalarını karşılaştırın ve raporlayın",
        "search_history" => "İsme, sisteme veya ID'ye göre filtrele...",
        "gpu_filter_all" => "GPU: Tümü",
        "status_filter_all" => "Durum: Tümü",
        "sort_date_desc" => "Tarih (Yeniden Eskiye)",
        "sort_date_asc" => "Tarih (Eskiden Yeniye)",
        "sort_score_desc" => "Skor (Yüksekten Düşüğe)",
        "sort_score_asc" => "Skor (Düşükten Yükseğe)",
        "sort_cpu_temp" => "CPU Sıcaklığı (Zirve)",
        "sort_gpu_temp" => "GPU Sıcaklığı (Zirve)",
        "sort_power" => "Güç Tüketimi (Ortalama)",
        "action_methodology" => "Metodoloji Oluştur",
        "action_compare" => "Karşılaştır (2)",
        "action_export_csv" => "CSV Dışa Aktar",
        "action_export_png" => "Grafik PNG Dışa Aktar",
        "action_group" => "Gruba Ata",
        "action_delete" => "Seçilenleri Sil",
        "action_clear_all" => "Tüm Geçmişi Temizle",
        "col_id" => "ID",
        "col_benchmark" => "Benchmark",
        "col_version" => "Sürüm",
        "col_gpu" => "GPU Modu",
        "col_status" => "Durum",
        "col_score" => "Skor",
        "col_duration" => "Süre",
        "col_cpu_temp" => "CPU Sıcaklık",
        "col_gpu_temp" => "GPU Sıcaklık",
        "col_power" => "Güç",
        "col_group" => "Grup",
        "col_date" => "Tarih",
        "col_actions" => "İşlemler",
        "delete_run_tooltip" => "Kaydı sil",
        "details_btn" => "Detaylar",
        "details_run_tooltip" => "Tüm telemetri ve test detaylarını göster",
        "history_select_all_tooltip" => "Görünür tüm geçmiş kayıtlarını seçer veya seçimi kaldırır (Ctrl+A)",
        "no_history_to_select_msg" => "Seçilebilecek geçmiş kaydı bulunmuyor.",
        "history_row_selected_badge" => "Seçili",
        "no_history_records" => "Filtrelere uygun geçmiş kaydı bulunamadı.",
        "runs_selected" => "test seçildi",
        "methodology_badge" => "Metodoloji (Ort.)",
        "clear_history_confirm_title" => "Geçmişi Temizle",
        "clear_history_confirm_msg" => "Veritabanındaki TÜM test kayıtlarını silmek istediğinizden emin misiniz?",
        "history_cleared_msg" => "Geçmiş başarıyla temizlendi.",
        "selected_deleted_msg" => "Seçilen kayıtlar silindi.",

        // Settings Tab
        "settings_title" => "Uygulama Ayarları",
        "settings_subtitle" => "Çalıştırıcı tercihleri, termal korumalar, dizinler ve arayüz görünümü",
        "section_general" => "Genel Tercihler ve Çalıştırma",
        "language_label" => "Uygulama Dili",
        "language_system" => "Sistem Dili (Otomatik Algıla)",
        "language_desc" => "Arayüz dilini seçin veya sistem dilini otomatik takip edin",
        "telemetry_interval_label" => "Telemetri Örnekleme Aralığı",
        "telemetry_interval_desc" => "CPU/GPU sıcaklık, saat hızı ve güç sensörleri için arka plan örnekleme sıklığı",
        "interval_250" => "250 ms (Yüksek Hassasiyet)",
        "interval_500" => "500 ms (Standart)",
        "interval_1000" => "1000 ms (Düşük Yük)",
        "interval_2000" => "2000 ms (En Az)",
        "disable_telemetry_during_runs_label" => "Testler sırasında telemetriyi tamamen devre dışı bırak",
        "disable_telemetry_during_runs_desc" => "Benchmark veya stres testi çalışırken ölçüm sapmasını sıfırlamak için sensör yoklamasını duraklat (Monitör dışındayken boşta zaten yavaşlatılır)",
        "console_buffer_label" => "Terminal Konsol Satır Limiti",
        "console_buffer_desc" => "Canlı terminal tamponunda saklanacak en fazla satır sayısı (50 - 1000)",
        "max_duration_label" => "Maksimum Test Süresi (saniye)",
        "max_duration_desc" => "Takılan veya çok uzun süren test süreçlerini durduracak otomatik zaman aşımı",
        "duration_0" => "Sınırsız (Zaman Aşımı Yok)",
        "duration_60" => "60 saniye (1 dk)",
        "duration_120" => "120 saniye (2 dk)",
        "duration_180" => "180 saniye (3 dk)",
        "duration_300" => "300 saniye (5 dk)",
        "duration_600" => "600 saniye (10 dk)",
        "save_stopped_runs_label" => "İptal edilen / durdurulan testlerin kayıtlarını sakla",
        "save_stopped_runs_desc" => "Kullanıcı tarafından iptal edilen testlerin telemetri ve kısmi sonuçlarını veritabanında tut",
        "section_thermal" => "Termal Yönetim ve Koruma",
        "thermal_mode_label" => "Termal Strateji Modu",
        "thermal_mode_desc" => "Tutarlı ve tekrarlanabilir sonuçlar için test öncesi sıcaklık koşullarını zorunlu kılar",
        "thermal_mode_off" => "Devre Dışı (Hemen başlat)",
        "thermal_mode_cooldown" => "Soğuma Koruması (Hedef sıcaklığa kadar bekle)",
        "thermal_mode_warmup" => "Isınma Koruması (Hedef sıcaklığa kadar ön ısıtma)",
        "thermal_mode_smart" => "Akıllı Termal Pencere (Belirli aralıkta sabitle)",
        "cooldown_temp_label" => "Soğuma Hedef Sıcaklığı (°C)",
        "cooldown_temp_desc" => "Test başlamadan önce sistemin inmesi gereken hedef sıcaklık",
        "warmup_temp_label" => "Isınma Hedef Sıcaklığı (°C)",
        "warmup_temp_desc" => "Ön ısıtma adımı için hedef başlangıç sıcaklığı",
        "thermal_timeout_label" => "Termal Bekleme Zaman Aşımı (saniye)",
        "thermal_timeout_desc" => "Hedef termal duruma ulaşmak için beklenecek azami süre",
        "temp_40" => "40 °C",
        "temp_45" => "45 °C",
        "temp_50_std" => "50 °C (Standart)",
        "temp_55" => "55 °C",
        "temp_60" => "60 °C",
        "temp_65_std" => "65 °C (Standart)",
        "temp_70" => "70 °C",
        "temp_75" => "75 °C",
        "secs_15" => "15 sn",
        "secs_30_std" => "30 sn (Standart)",
        "secs_45" => "45 sn",
        "secs_60" => "60 sn",
        "secs_120" => "120 sn (2 dk)",
        "section_directories" => "Dizinler ve Yollar",
        "runners_dir_label" => "Özel Çalıştırıcı Dizini (İsteğe bağlı)",
        "runners_dir_desc" => "Harici test çalıştırıcılarının veya betiklerinin bulunduğu özel klasör yolu",
        "section_appearance" => "Tema ve Masaüstü Uyarlamalı Arayüz",
        "theme_mode_label" => "Tema Kutupluluk Modu",
        "theme_mode_desc" => "Masaüstü otomatik senkronizasyon, koyu, açık veya özel renk paleti seçimi",
        "theme_system" => "Masaüstü Sistem Teması (Canlı Otomatik Eşzamanlama)",
        "theme_dark" => "Koyu Tema (Koyu Zorla)",
        "theme_light" => "Açık Tema (Açık Zorla)",
        "theme_custom" => "Özel Renk Paleti (Kullanıcı Tanımlı)",
        "accent_color_label" => "Vurgu Rengi",
        "window_bg_label" => "Pencere Yüzey Rengi",
        "card_bg_label" => "Kart / Panel Yüzey Rengi",
        "text_color_label" => "Birincil Metin Rengi",
        "ui_scale_label" => "Arayüz Ölçeklendirme Katsayısı",
        "ui_scale_desc" => "Arayüz bileşenlerini ve yazı boyutlarını ölçeklendirin (%90 - %150)",
        "scale_90" => "%90 (Sıkı)",
        "scale_100" => "%100 (Varsayılan)",
        "scale_110" => "%110 (Rahat)",
        "scale_125" => "%125 (Büyük)",
        "scale_150" => "%150 (Yüksek DPI)",
        "detected_desktop" => "Algılanan Masaüstü:",
        "save_settings_btn" => "Ayarları Kaydet",
        "settings_saved_msg" => "Ayarlar başarıyla kaydedildi.",

        // Dialogs & Modals
        "modal_eula_title" => "Benchmark Paketini Kur",
        "modal_eula_msg" => "Bu benchmark paketini sisteminize indirip kurmak üzeresiniz. Lütfen aşağıdaki lisans ve kullanım koşullarını inceleyin.",
        "modal_eula_terms" => "Lisans ve Kullanım Koşulları",
        "modal_eula_loading" => "Lisans metni yükleniyor...",
        "modal_eula_fetch_error" => "Lisans metni arka plan servisinden yüklenemedi:",
        "modal_eula_disclaimer" => "BenchHub, üçüncü parti araçları doğrudan kendi kaynaklarından indirir ve yalnızca bağımsız bir başlatıcı/yönetici olarak çalışır; donanım ve yazılım kullanım riski kullanıcıya aittir. Ticari veya kurumsal kullanım ilgili üreticiden ayrı lisans gerektirebilir.",
        "modal_eula_accept_check" => "Yukarıdaki üçüncü parti yazılım lisans / EULA koşullarını okudum, kabul ediyorum ve kullanımımın belirtilen kısıtlamalara uygun olduğunu onaylıyorum.",
        "modal_eula_confirm" => "Kabul Et, İndir ve Kur",
        "modal_eula_cancel" => "Vazgeç",
        "modal_eula_install_logs" => "Kurulum Günlüğü (Canlı)",
        "modal_eula_watch_monitor" => "Monitörde İzle",
        "modal_eula_close" => "Kapat",
        "modal_eula_status_installing" => "İndiriliyor ve kuruluyor...",
        "modal_eula_status_success" => "Kurulum Tamamlandı",
        "modal_eula_status_failed" => "Kurulum Başarısız",
        "modal_eula_status_cancelled" => "Kurulum Durduruldu",
        "modal_args_title" => "Özel Çalıştırma Parametreleri",
        "modal_args_desc" => "Bu test için ek komut satırı parametreleri girin. Güvenlik gerekçesiyle tehlikeli bayraklar (-o, --output, -y), kök dizin yolları ('/') ve kabuk operatörleri engellenir.",
        "modal_args_placeholder" => "Ör. --threads 8 --preset fast",
        "modal_args_apply" => "Uygula ve Kaydet",
        "modal_args_forbidden" => "Yasaklı parametre:",
        "custom_args_saved_msg" => "Özel parametreler güncellendi.",
        "modal_compare_title" => "Yan Yana Test Karşılaştırması",
        "modal_compare_metric" => "Ölçüt / Telemetri",
        "modal_compare_run" => "Kayıt",
        "modal_compare_diff" => "Fark / Değişim",
        "modal_compare_peak" => "(Zirve)",
        "modal_compare_avg" => "(Ort.)",
        "modal_ok_btn" => "Tamam",
        "modal_group_title" => "Testleri Gruba Ata",
        "modal_group_desc" => "Testleri bir grup adı veya senaryo altında toplayarak organize edin.",
        "modal_group_name" => "Grup Adı",
        "modal_group_placeholder" => "Ör. Temel-v1, Düşük-Voltaj, Performans-Modu",
        "modal_group_assign" => "Gruba Ata",
        "modal_group_remove" => "Gruptan Çıkar",
        "modal_confirm_yes" => "Evet, Devam Et",
        "modal_confirm_no" => "Vazgeç",
        "modal_details_title" => "Test Telemetrisi ve Detaylar",
        "modal_details_score_save" => "Kaydet",
        "modal_details_score_saved" => "Test skoru güncellendi.",
        "modal_details_score_invalid" => "Geçerli bir sayısal skor girin (temizlemek için boş bırakın).",
        "modal_details_cpu_section" => "İşlemci (CPU) Metrikleri",
        "modal_details_gpu_section" => "Ekran Kartı (GPU) Metrikleri",
        "modal_details_mem_section" => "Bellek (RAM / VRAM) Metrikleri",
        "modal_details_other_section" => "Sistem ve Güç",
        "modal_details_avg_usage" => "Ort. Yük",
        "modal_details_peak_usage" => "Zirve Yük",
        "modal_details_avg_temp" => "Ort. Sıcaklık",
        "modal_details_peak_temp" => "Zirve Sıcaklık",
        "modal_details_avg_freq" => "Ort. Frekans",
        "modal_details_peak_freq" => "Zirve Frekans",
        "modal_details_avg_sys_power" => "Ort. Sis. Gücü",
        "modal_details_peak_sys_power" => "Zirve Sis. Gücü",
        "modal_details_avg_gpu_power" => "Ort. GPU Gücü",
        "modal_details_peak_gpu_power" => "Zirve GPU Gücü",
        "modal_details_avg_ac_power" => "Ort. AC Güç",
        "modal_details_peak_ac_power" => "Zirve AC Güç",
        "modal_details_cpu_throttle" => "CPU Kısılması",
        "modal_details_gpu_throttle" => "GPU Kısılması",
        "modal_details_ram_avg" => "Ort. RAM",
        "modal_details_ram_peak" => "Zirve RAM",
        "modal_details_vram_avg" => "Ort. VRAM",
        "modal_details_vram_peak" => "Zirve VRAM",
        "modal_details_vram_freq_avg" => "Ort. VRAM Frek.",
        "modal_details_vram_freq_peak" => "Zirve VRAM Frek.",
        "modal_details_power_profile" => "Güç Profili",
        "modal_details_category" => "Kategori",
        "modal_details_system_info" => "Sistem Donanımı",
        "modal_details_children_title" => "Metodolojiye Dahil Testler",
        "modal_details_run_log" => "Kayıtlı Çalışma Günlüğü",
        "modal_details_show_log" => "Günlüğü Göster",
        "modal_details_hide_log" => "Günlüğü Gizle",
        "modal_details_loading_log" => "Günlük dosyası yükleniyor...",
        "modal_details_no_log" => "Bu kayıt için kaydedilmiş günlük dosyası bulunamadı.",
        "modal_details_detected_links" => "Algılanan Bağlantılar",
        "modal_details_date_label" => "Kayıt Tarihi",
        "throttle_status_none" => "Yok",
        "throttle_status_thermal" => "Termal Kısılma",
        "throttle_status_power" => "Güç Limiti",

        // Units and fallbacks
        "unit_celsius" => "°C",
        "unit_mhz" => "MHz",
        "unit_ghz" => "GHz",
        "unit_watt" => "W",
        "unit_mb" => "MB",
        "unit_gb" => "GB",
        "unit_secs" => "sn",

        // Default fallback to English
        _ => translate_en(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KEYS: &[&str] = &[
        "app_title",
        "app_subtitle",
        "tab_benchmarks",
        "tab_stress",
        "tab_monitor",
        "tab_history",
        "tab_settings",
        "status_ready",
        "status_running",
        "status_installing",
        "status_cancelled",
        "status_completed",
        "status_timeout",
        "status_failed",
        "status_error",
        "cancel_active",
        "notif_already_running",
        "dismiss_toast",
        "catalog_title",
        "catalog_subtitle",
        "search_benchmarks",
        "all_categories",
        "category_cpu",
        "category_gpu",
        "category_render",
        "category_disk",
        "category_memory",
        "category_ai",
        "category_system",
        "version_label",
        "approx_size",
        "score_unit_label",
        "install_btn",
        "uninstall_btn",
        "run_btn",
        "custom_config_btn",
        "queue_btn",
        "select_all_btn",
        "deselect_all_btn",
        "select_all_tooltip",
        "queued_selected_badge",
        "install_before_queue_msg",
        "no_installed_to_select_msg",
        "in_queue_badge",
        "run_queue_btn",
        "running_queue_btn",
        "clear_queue_btn",
        "no_benchmarks_found",
        "gpu_mode_label",
        "gpu_nvidia",
        "gpu_integrated",
        "gpu_auto",
        "gpu_mode_mixed",
        "stress_title",
        "stress_subtitle",
        "search_stress",
        "stress_duration_label",
        "stress_duration_1m",
        "stress_duration_5m",
        "stress_duration_15m",
        "stress_duration_30m",
        "stress_duration_60m",
        "stress_duration_unlimited",
        "run_stress_btn",
        "stress_empty_msg",
        "stress_select_all_tooltip",
        "no_stress_to_select_msg",
        "install_stress_before_queue_msg",
        "monitor_title",
        "monitor_subtitle",
        "telemetry_cpu_temp",
        "telemetry_gpu_temp",
        "telemetry_cpu_freq",
        "telemetry_gpu_freq",
        "telemetry_cpu_usage",
        "telemetry_gpu_usage",
        "telemetry_power",
        "telemetry_gpu_power",
        "telemetry_ram",
        "telemetry_vram",
        "telemetry_throttle",
        "throttle_none",
        "throttle_active",
        "load_label",
        "gpu_short_label",
        "power_profile_label",
        "sensors_label",
        "terminal_title",
        "lines_label",
        "terminal_follow_btn",
        "elapsed_label",
        "copy_logs",
        "clear_logs",
        "copied",
        "no_logs_yet",
        "history_title",
        "history_subtitle",
        "search_history",
        "gpu_filter_all",
        "status_filter_all",
        "sort_date_desc",
        "sort_date_asc",
        "sort_score_desc",
        "sort_score_asc",
        "sort_cpu_temp",
        "sort_gpu_temp",
        "sort_power",
        "action_methodology",
        "action_compare",
        "action_export_csv",
        "action_export_png",
        "action_group",
        "action_delete",
        "action_clear_all",
        "col_id",
        "col_benchmark",
        "col_version",
        "col_gpu",
        "col_status",
        "col_score",
        "col_duration",
        "col_cpu_temp",
        "col_gpu_temp",
        "col_power",
        "col_group",
        "col_date",
        "col_actions",
        "delete_run_tooltip",
        "details_btn",
        "details_run_tooltip",
        "history_select_all_tooltip",
        "no_history_to_select_msg",
        "history_row_selected_badge",
        "no_history_records",
        "runs_selected",
        "methodology_badge",
        "clear_history_confirm_title",
        "clear_history_confirm_msg",
        "history_cleared_msg",
        "selected_deleted_msg",
        "settings_title",
        "settings_subtitle",
        "section_general",
        "language_label",
        "language_system",
        "language_desc",
        "telemetry_interval_label",
        "telemetry_interval_desc",
        "interval_250",
        "interval_500",
        "interval_1000",
        "interval_2000",
        "disable_telemetry_during_runs_label",
        "disable_telemetry_during_runs_desc",
        "console_buffer_label",
        "console_buffer_desc",
        "max_duration_label",
        "max_duration_desc",
        "duration_0",
        "duration_60",
        "duration_120",
        "duration_180",
        "duration_300",
        "duration_600",
        "save_stopped_runs_label",
        "save_stopped_runs_desc",
        "section_thermal",
        "thermal_mode_label",
        "thermal_mode_desc",
        "thermal_mode_off",
        "thermal_mode_cooldown",
        "thermal_mode_warmup",
        "thermal_mode_smart",
        "cooldown_temp_label",
        "cooldown_temp_desc",
        "warmup_temp_label",
        "warmup_temp_desc",
        "thermal_timeout_label",
        "thermal_timeout_desc",
        "temp_40",
        "temp_45",
        "temp_50_std",
        "temp_55",
        "temp_60",
        "temp_65_std",
        "temp_70",
        "temp_75",
        "secs_15",
        "secs_30_std",
        "secs_45",
        "secs_60",
        "secs_120",
        "section_directories",
        "runners_dir_label",
        "runners_dir_desc",
        "section_appearance",
        "theme_mode_label",
        "theme_mode_desc",
        "theme_system",
        "theme_dark",
        "theme_light",
        "theme_custom",
        "accent_color_label",
        "window_bg_label",
        "card_bg_label",
        "text_color_label",
        "ui_scale_label",
        "ui_scale_desc",
        "scale_90",
        "scale_100",
        "scale_110",
        "scale_125",
        "scale_150",
        "detected_desktop",
        "save_settings_btn",
        "settings_saved_msg",
        "modal_eula_title",
        "modal_eula_msg",
        "modal_eula_terms",
        "modal_eula_loading",
        "modal_eula_fetch_error",
        "modal_eula_disclaimer",
        "modal_eula_accept_check",
        "modal_eula_confirm",
        "modal_eula_cancel",
        "modal_eula_install_logs",
        "modal_eula_watch_monitor",
        "modal_eula_close",
        "modal_eula_status_installing",
        "modal_eula_status_success",
        "modal_eula_status_failed",
        "modal_eula_status_cancelled",
        "modal_args_title",
        "modal_args_desc",
        "modal_args_placeholder",
        "modal_args_apply",
        "modal_args_forbidden",
        "custom_args_saved_msg",
        "modal_compare_title",
        "modal_compare_metric",
        "modal_compare_run",
        "modal_compare_diff",
        "modal_compare_peak",
        "modal_compare_avg",
        "modal_ok_btn",
        "modal_group_title",
        "modal_group_desc",
        "modal_group_name",
        "modal_group_placeholder",
        "modal_group_assign",
        "modal_group_remove",
        "modal_confirm_yes",
        "modal_confirm_no",
        "modal_details_title",
        "modal_details_score_save",
        "modal_details_score_saved",
        "modal_details_score_invalid",
        "modal_details_cpu_section",
        "modal_details_gpu_section",
        "modal_details_mem_section",
        "modal_details_other_section",
        "modal_details_avg_usage",
        "modal_details_peak_usage",
        "modal_details_avg_temp",
        "modal_details_peak_temp",
        "modal_details_avg_freq",
        "modal_details_peak_freq",
        "modal_details_avg_sys_power",
        "modal_details_peak_sys_power",
        "modal_details_avg_gpu_power",
        "modal_details_peak_gpu_power",
        "modal_details_avg_ac_power",
        "modal_details_peak_ac_power",
        "modal_details_cpu_throttle",
        "modal_details_gpu_throttle",
        "modal_details_ram_avg",
        "modal_details_ram_peak",
        "modal_details_vram_avg",
        "modal_details_vram_peak",
        "modal_details_vram_freq_avg",
        "modal_details_vram_freq_peak",
        "modal_details_power_profile",
        "modal_details_category",
        "modal_details_system_info",
        "modal_details_children_title",
        "modal_details_run_log",
        "modal_details_show_log",
        "modal_details_hide_log",
        "modal_details_loading_log",
        "modal_details_no_log",
        "modal_details_detected_links",
        "modal_details_date_label",
        "throttle_status_none",
        "throttle_status_thermal",
        "throttle_status_power",
        "unit_celsius",
        "unit_mhz",
        "unit_ghz",
        "unit_watt",
        "unit_mb",
        "unit_gb",
        "unit_secs",
    ];

    #[test]
    fn test_all_keys_defined_for_en_and_tr() {
        for &k in ALL_KEYS {
            let en = translate_en(k);
            let tr = translate_tr(k);
            assert_ne!(en, k, "Missing English translation for key '{}'", k);
            assert!(!en.trim().is_empty(), "Empty EN value for '{}'", k);
            assert!(!tr.trim().is_empty(), "Empty TR value for '{}'", k);
            if k != "app_title" {
                assert_ne!(tr, k, "Turkish translation fell back to raw key '{}'", k);
            }
        }
    }

    #[test]
    fn test_benchmark_metadata_and_categories_coverage() {
        let ids = [
            "7zip",
            "blender",
            "cray",
            "ffmpeg",
            "fio",
            "furmark",
            "geekbench",
            "gravitymark",
            "llama-bench",
            "stream",
            "unigine",
            "ycruncher",
        ];
        for id in ids {
            assert_ne!(bench_name(Language::En, id, ""), "");
            assert_ne!(bench_name(Language::Tr, id, ""), "");
            assert_ne!(bench_desc(Language::En, id, ""), "");
            assert_ne!(bench_desc(Language::Tr, id, ""), "");
        }

        let categories = [
            "CPU",
            "CPU Render",
            "Render",
            "Storage",
            "GPU",
            "GPU Stress",
            "System & CPU",
            "AI",
            "Memory",
        ];
        for cat in categories {
            assert!(!translate_category(Language::En, cat).is_empty());
            assert!(!translate_category(Language::Tr, cat).is_empty());
        }
    }

    #[test]
    fn test_locale_matching_and_notifications() {
        assert_eq!(match_supported_language("tr_TR.UTF-8"), Some(Language::Tr));
        assert_eq!(match_supported_language("en_US.UTF-8"), Some(Language::En));
        assert_eq!(match_supported_language("de_DE.UTF-8"), None);

        let notif = DaemonNotification::InstallCompleted {
            benchmark_id: "7zip".to_string(),
            version: "24.08".to_string(),
        };
        let (tr_msg, err) = translate_notification(Language::Tr, &notif);
        assert!(!err);
        assert!(tr_msg.contains("7-Zip"));
    }

    #[test]
    fn test_translate_status_covers_all_normalized_states() {
        for lang in [Language::En, Language::Tr] {
            let cancelled = t(lang, "status_cancelled");
            let timeout = t(lang, "status_timeout");
            let failed = t(lang, "status_failed");
            let completed = t(lang, "status_completed");

            assert_eq!(translate_status(lang, "Stopped"), cancelled);
            assert_eq!(translate_status(lang, "cancelled"), cancelled);
            assert_eq!(translate_status(lang, "Timeout"), timeout);
            assert_eq!(translate_status(lang, "Timeout (Scored)"), timeout);
            assert_eq!(translate_status(lang, "Failed"), failed);
            assert_eq!(translate_status(lang, "Failed (Scored)"), failed);
            assert_eq!(translate_status(lang, "Success"), completed);
            assert_eq!(translate_status(lang, "Completed"), completed);
            assert_ne!(cancelled, completed);
            assert_ne!(timeout, completed);
            assert_ne!(failed, completed);
        }
    }
}
