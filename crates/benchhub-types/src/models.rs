use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct BenchmarkVersion {
    pub version: String,
    #[serde(default)]
    pub download_size: Option<String>,
    pub download_cmd: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    pub run_cmd: Option<String>,
    #[serde(default, alias = "run_arguments")]
    pub run_args: Option<Vec<String>>,
    pub score_regex: Option<String>,
    #[serde(default)]
    pub archive_type: Option<String>,
    #[serde(default)]
    pub binary_relative_path: Option<String>,
    #[serde(default)]
    pub score_unit: Option<String>,
    #[serde(default)]
    pub approx_size_mb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct BenchmarkProfile {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub download_size: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub download_url: Option<String>,
    pub download_cmd: Option<String>,
    #[serde(default)]
    pub run_cmd: String,
    #[serde(default, alias = "run_arguments")]
    pub run_args: Vec<String>,
    pub score_regex: Option<String>,
    #[serde(default)]
    pub versions: Vec<BenchmarkVersion>,
    #[serde(default)]
    pub default_version: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub archive_type: Option<String>,
    #[serde(default)]
    pub binary_relative_path: Option<String>,
    #[serde(default)]
    pub score_unit: Option<String>,
    #[serde(default)]
    pub approx_size_mb: Option<u64>,
    #[serde(default)]
    pub test_type: Option<String>,
}

impl BenchmarkProfile {
    /// Returns true if this profile represents a stress test instead of a standard benchmark
    pub fn is_stress_test(&self) -> bool {
        self.test_type.as_deref() == Some("stress")
            || self.category.eq_ignore_ascii_case("stress")
            || self.id.to_lowercase().contains("stress")
    }

    /// Formats the download size for display in the UI
    pub fn display_download_size(&self) -> String {
        if let Some(ref s) = self.download_size {
            s.clone()
        } else if let Some(mb) = self.approx_size_mb {
            format!("~{} MB", mb)
        } else {
            "Unknown".to_string()
        }
    }

    /// Returns a profile with version-specific overrides applied
    pub fn for_version(&self, version_name: &str) -> BenchmarkProfile {
        let target_ver = if version_name.is_empty() {
            self.default_version.as_deref().unwrap_or(version_name)
        } else {
            version_name
        };
        if let Some(v) = self.versions.iter().find(|v| v.version == target_ver) {
            BenchmarkProfile {
                id: self.id.clone(),
                name: format!("{} ({})", self.name, v.version),
                category: self.category.clone(),
                download_size: v
                    .download_size
                    .clone()
                    .or_else(|| self.download_size.clone()),
                description: self.description.clone(),
                download_url: v.download_url.clone().or_else(|| self.download_url.clone()),
                download_cmd: v.download_cmd.clone().or_else(|| self.download_cmd.clone()),
                run_cmd: v.run_cmd.clone().unwrap_or_else(|| self.run_cmd.clone()),
                run_args: v.run_args.clone().unwrap_or_else(|| self.run_args.clone()),
                score_regex: v.score_regex.clone().or_else(|| self.score_regex.clone()),
                versions: Vec::new(),
                default_version: self.default_version.clone(),
                version: Some(v.version.clone()),
                archive_type: v.archive_type.clone().or_else(|| self.archive_type.clone()),
                binary_relative_path: v
                    .binary_relative_path
                    .clone()
                    .or_else(|| self.binary_relative_path.clone()),
                score_unit: v.score_unit.clone().or_else(|| self.score_unit.clone()),
                approx_size_mb: v.approx_size_mb.or(self.approx_size_mb),
                test_type: self.test_type.clone(),
            }
        } else {
            self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryData {
    pub timestamp: i64,
    pub cpu_temp: f32,
    pub gpu_temp: f32,
    #[serde(default)]
    pub cpu_usage: f32,
    #[serde(default)]
    pub gpu_usage: f32,
    pub cpu_freq: f32,
    #[serde(default)]
    pub gpu_freq_mhz: f32,
    #[serde(default)]
    pub vram_freq_mhz: f32,
    pub power_w: f32,
    #[serde(default)]
    pub gpu_power_w: f32,
    #[serde(default)]
    pub ac_power_w: f32,
    pub ram_usage_mb: f32,
    #[serde(default)]
    pub vram_usage_mb: f32,
    #[serde(default = "default_throttle_str")]
    pub cpu_throttle: String,
    #[serde(default = "default_throttle_str")]
    pub gpu_throttle: String,
}

fn default_throttle_str() -> String {
    "none".to_string()
}

fn default_power_profile_str() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RunResult {
    pub id: i64,
    pub benchmark_id: String,
    pub category: String,
    pub preset_or_version: String,
    pub gpu_mode: String,
    pub score: Option<f64>,
    pub status: String,
    pub timestamp: i64,
    pub duration_secs: f32,

    // CPU Metrics
    pub avg_cpu_usage: f32,
    pub peak_cpu_usage: f32,
    pub avg_cpu_temp: f32,
    pub peak_cpu_temp: f32,
    pub avg_cpu_freq_mhz: u32,
    pub peak_cpu_freq_mhz: u32,
    #[serde(default = "default_throttle_str")]
    pub cpu_throttling: String,

    // GPU Metrics
    pub avg_gpu_usage: f32,
    pub peak_gpu_usage: f32,
    pub avg_gpu_temp: f32,
    pub peak_gpu_temp: f32,
    pub avg_gpu_freq_mhz: u32,
    pub peak_gpu_freq_mhz: u32,
    pub avg_vram_freq_mhz: u32,
    pub peak_vram_freq_mhz: u32,
    pub avg_gpu_power_w: f32,
    pub peak_gpu_power_w: f32,
    #[serde(default = "default_throttle_str")]
    pub gpu_throttling: String,

    // Memory Metrics (RAM & VRAM)
    pub avg_ram_gb: f32,
    pub peak_ram_gb: f32,
    #[serde(default)]
    pub avg_vram_gb: f32,
    #[serde(default)]
    pub peak_vram_gb: f32,

    // Power & Other Metrics
    pub avg_power_w: f32,
    pub peak_power_w: f32,
    pub avg_ac_power_w: f32,
    pub peak_ac_power_w: f32,

    pub system_info_summary: String,
    pub log_path: String,
    #[serde(default = "default_power_profile_str")]
    pub power_profile: String,
    #[serde(default)]
    pub is_methodology: bool,
    #[serde(default)]
    pub methodology_parent_id: Option<i64>,
    #[serde(default)]
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GpuMode {
    #[default]
    #[serde(
        alias = "nvidia_dgpu",
        alias = "NvidiaDgpu",
        alias = "nvidia",
        alias = "Dedicated GPU (NVIDIA)"
    )]
    NvidiaDgpu,
    #[serde(
        alias = "integrated",
        alias = "Integrated",
        alias = "igpu",
        alias = "Integrated GPU (iGPU)"
    )]
    Integrated,
    #[serde(
        alias = "auto",
        alias = "System Default",
        alias = "Auto",
        alias = "system"
    )]
    Auto,
}

impl GpuMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            GpuMode::NvidiaDgpu => "nvidia_dgpu",
            GpuMode::Integrated => "integrated",
            GpuMode::Auto => "auto",
        }
    }
}

impl std::fmt::Display for GpuMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for GpuMode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        if lower.contains("nvidia") || lower.contains("dgpu") || lower.contains("dedicated") {
            Ok(GpuMode::NvidiaDgpu)
        } else if lower.contains("igpu") || lower.contains("integrated") {
            Ok(GpuMode::Integrated)
        } else {
            Ok(GpuMode::Auto)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HistorySortOrder {
    #[default]
    #[serde(alias = "date_desc", alias = "DateDesc")]
    DateDesc,
    #[serde(alias = "date_asc", alias = "DateAsc")]
    DateAsc,
    #[serde(alias = "score_desc", alias = "ScoreDesc")]
    ScoreDesc,
    #[serde(alias = "score_asc", alias = "ScoreAsc")]
    ScoreAsc,
    #[serde(alias = "cpu_temp_peak", alias = "CpuTempPeak")]
    CpuTempPeak,
    #[serde(alias = "gpu_temp_peak", alias = "GpuTempPeak")]
    GpuTempPeak,
    #[serde(alias = "power_avg", alias = "PowerAvg")]
    PowerAvg,
}

impl HistorySortOrder {
    pub fn to_sql_order_clause(&self) -> &'static str {
        match self {
            HistorySortOrder::DateDesc => "timestamp DESC",
            HistorySortOrder::DateAsc => "timestamp ASC",
            HistorySortOrder::ScoreDesc => "CASE WHEN score IS NULL THEN 1 ELSE 0 END, score DESC",
            HistorySortOrder::ScoreAsc => "CASE WHEN score IS NULL THEN 1 ELSE 0 END, score ASC",
            HistorySortOrder::CpuTempPeak => "peak_cpu_temp DESC",
            HistorySortOrder::GpuTempPeak => "peak_gpu_temp DESC",
            HistorySortOrder::PowerAvg => "avg_power_w DESC",
        }
    }
}

impl std::str::FromStr for HistorySortOrder {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        if lower.contains("oldest") || lower.contains("date_asc") {
            Ok(HistorySortOrder::DateAsc)
        } else if lower.contains("score") && (lower.contains("high") || lower.contains("desc")) {
            Ok(HistorySortOrder::ScoreDesc)
        } else if lower.contains("score") && (lower.contains("low") || lower.contains("asc")) {
            Ok(HistorySortOrder::ScoreAsc)
        } else if lower.contains("cpu") {
            Ok(HistorySortOrder::CpuTempPeak)
        } else if lower.contains("gpu") {
            Ok(HistorySortOrder::GpuTempPeak)
        } else if lower.contains("power") {
            Ok(HistorySortOrder::PowerAvg)
        } else {
            Ok(HistorySortOrder::DateDesc)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunComparisonDiff {
    pub run_a: RunResult,
    pub run_b: RunResult,
    pub score_diff: Option<f64>,
    pub score_diff_pct: Option<f64>,
    pub duration_diff_secs: f32,

    // CPU Deltas
    pub avg_cpu_usage_diff: f32,
    pub peak_cpu_usage_diff: f32,
    pub avg_cpu_temp_diff: f32,
    pub peak_cpu_temp_diff: f32,
    pub avg_cpu_freq_diff_mhz: i32,
    pub peak_cpu_freq_diff_mhz: i32,

    // GPU Deltas
    pub avg_gpu_usage_diff: f32,
    pub peak_gpu_usage_diff: f32,
    pub avg_gpu_temp_diff: f32,
    pub peak_gpu_temp_diff: f32,
    pub avg_gpu_freq_diff_mhz: i32,
    pub peak_gpu_freq_diff_mhz: i32,
    pub avg_vram_freq_diff_mhz: i32,
    pub peak_vram_freq_diff_mhz: i32,
    pub avg_gpu_power_diff_w: f32,
    pub peak_gpu_power_diff_w: f32,

    // Power & RAM Deltas
    pub avg_power_diff_w: f32,
    pub peak_power_diff_w: f32,
    pub avg_ac_power_diff_w: f32,
    pub peak_ac_power_diff_w: f32,
    pub avg_ram_diff_gb: f32,
    pub peak_ram_diff_gb: f32,
    pub avg_vram_diff_gb: f32,
    pub peak_vram_diff_gb: f32,
}

impl RunComparisonDiff {
    pub fn calculate(run_a: RunResult, run_b: RunResult) -> Self {
        let (score_diff, score_diff_pct) = match (run_a.score, run_b.score) {
            (Some(sa), Some(sb)) => {
                let diff = sb - sa;
                let pct = if sa.abs() > 0.0001 {
                    (diff / sa) * 100.0
                } else {
                    0.0
                };
                (Some(diff), Some(pct))
            }
            _ => (None, None),
        };

        let duration_diff_secs = run_b.duration_secs - run_a.duration_secs;

        let avg_cpu_usage_diff = run_b.avg_cpu_usage - run_a.avg_cpu_usage;
        let peak_cpu_usage_diff = run_b.peak_cpu_usage - run_a.peak_cpu_usage;
        let avg_cpu_temp_diff = run_b.avg_cpu_temp - run_a.avg_cpu_temp;
        let peak_cpu_temp_diff = run_b.peak_cpu_temp - run_a.peak_cpu_temp;
        let avg_cpu_freq_diff_mhz =
            (run_b.avg_cpu_freq_mhz as i32) - (run_a.avg_cpu_freq_mhz as i32);
        let peak_cpu_freq_diff_mhz =
            (run_b.peak_cpu_freq_mhz as i32) - (run_a.peak_cpu_freq_mhz as i32);

        let avg_gpu_usage_diff = run_b.avg_gpu_usage - run_a.avg_gpu_usage;
        let peak_gpu_usage_diff = run_b.peak_gpu_usage - run_a.peak_gpu_usage;
        let avg_gpu_temp_diff = run_b.avg_gpu_temp - run_a.avg_gpu_temp;
        let peak_gpu_temp_diff = run_b.peak_gpu_temp - run_a.peak_gpu_temp;
        let avg_gpu_freq_diff_mhz =
            (run_b.avg_gpu_freq_mhz as i32) - (run_a.avg_gpu_freq_mhz as i32);
        let peak_gpu_freq_diff_mhz =
            (run_b.peak_gpu_freq_mhz as i32) - (run_a.peak_gpu_freq_mhz as i32);
        let avg_vram_freq_diff_mhz =
            (run_b.avg_vram_freq_mhz as i32) - (run_a.avg_vram_freq_mhz as i32);
        let peak_vram_freq_diff_mhz =
            (run_b.peak_vram_freq_mhz as i32) - (run_a.peak_vram_freq_mhz as i32);
        let avg_gpu_power_diff_w = run_b.avg_gpu_power_w - run_a.avg_gpu_power_w;
        let peak_gpu_power_diff_w = run_b.peak_gpu_power_w - run_a.peak_gpu_power_w;

        let avg_power_diff_w = run_b.avg_power_w - run_a.avg_power_w;
        let peak_power_diff_w = run_b.peak_power_w - run_a.peak_power_w;
        let avg_ac_power_diff_w = run_b.avg_ac_power_w - run_a.avg_ac_power_w;
        let peak_ac_power_diff_w = run_b.peak_ac_power_w - run_a.peak_ac_power_w;
        let avg_ram_diff_gb = run_b.avg_ram_gb - run_a.avg_ram_gb;
        let peak_ram_diff_gb = run_b.peak_ram_gb - run_a.peak_ram_gb;
        let avg_vram_diff_gb = run_b.avg_vram_gb - run_a.avg_vram_gb;
        let peak_vram_diff_gb = run_b.peak_vram_gb - run_a.peak_vram_gb;

        Self {
            run_a,
            run_b,
            score_diff,
            score_diff_pct,
            duration_diff_secs,
            avg_cpu_usage_diff,
            peak_cpu_usage_diff,
            avg_cpu_temp_diff,
            peak_cpu_temp_diff,
            avg_cpu_freq_diff_mhz,
            peak_cpu_freq_diff_mhz,
            avg_gpu_usage_diff,
            peak_gpu_usage_diff,
            avg_gpu_temp_diff,
            peak_gpu_temp_diff,
            avg_gpu_freq_diff_mhz,
            peak_gpu_freq_diff_mhz,
            avg_vram_freq_diff_mhz,
            peak_vram_freq_diff_mhz,
            avg_gpu_power_diff_w,
            peak_gpu_power_diff_w,
            avg_power_diff_w,
            peak_power_diff_w,
            avg_ac_power_diff_w,
            peak_ac_power_diff_w,
            avg_ram_diff_gb,
            peak_ram_diff_gb,
            avg_vram_diff_gb,
            peak_vram_diff_gb,
        }
    }
}

// Default Configuration Constants
pub const DEFAULT_TELEMETRY_INTERVAL_MS: u64 = 500;
pub const DEFAULT_TERMINAL_BUFFER_LINES: usize = 150;
pub const DEFAULT_SAVE_STOPPED_RUNS: bool = true;
pub const DEFAULT_SELECTED_GPU_MODE: GpuMode = GpuMode::NvidiaDgpu;
pub const DEFAULT_MAX_TEST_DURATION_SECS: u64 = 600;
pub const DEFAULT_THERMAL_MODE: &str = "off";
pub const DEFAULT_THERMAL_COOLDOWN_TEMP: u32 = 50;
pub const DEFAULT_THERMAL_WARMUP_TEMP: u32 = 65;
pub const DEFAULT_THERMAL_TIMEOUT_SECS: u32 = 30;

pub const VALID_TELEMETRY_INTERVALS: [u64; 4] = [250, 500, 1000, 2000];
pub const VALID_MAX_TEST_DURATIONS: [u64; 6] = [0, 60, 120, 180, 300, 600];
pub const VALID_THERMAL_MODES: [&str; 4] = ["off", "cooldown", "warmup", "smart"];
pub const VALID_COOLDOWN_TEMPS: [u32; 5] = [40, 45, 50, 55, 60];
pub const VALID_WARMUP_TEMPS: [u32; 5] = [55, 60, 65, 70, 75];
pub const VALID_THERMAL_TIMEOUTS: [u32; 5] = [15, 30, 45, 60, 120];
pub const MIN_TERMINAL_BUFFER_LINES: usize = 50;
pub const MAX_TERMINAL_BUFFER_LINES: usize = 1000;

fn default_telemetry_interval_ms() -> u64 {
    DEFAULT_TELEMETRY_INTERVAL_MS
}

fn default_terminal_buffer_lines() -> usize {
    DEFAULT_TERMINAL_BUFFER_LINES
}

fn default_save_stopped_runs() -> bool {
    DEFAULT_SAVE_STOPPED_RUNS
}

fn default_selected_gpu_mode() -> GpuMode {
    DEFAULT_SELECTED_GPU_MODE
}

fn default_language() -> String {
    "system".to_string()
}

fn default_max_test_duration_secs() -> u64 {
    DEFAULT_MAX_TEST_DURATION_SECS
}

fn default_thermal_mode() -> String {
    DEFAULT_THERMAL_MODE.to_string()
}

fn default_thermal_cooldown_temp() -> u32 {
    DEFAULT_THERMAL_COOLDOWN_TEMP
}

fn default_thermal_warmup_temp() -> u32 {
    DEFAULT_THERMAL_WARMUP_TEMP
}

fn default_thermal_timeout_secs() -> u32 {
    DEFAULT_THERMAL_TIMEOUT_SECS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_telemetry_interval_ms")]
    pub telemetry_interval_ms: u64,
    #[serde(default = "default_terminal_buffer_lines")]
    pub terminal_buffer_lines: usize,
    #[serde(default = "default_save_stopped_runs")]
    pub save_stopped_runs: bool,
    #[serde(default = "default_selected_gpu_mode")]
    pub selected_gpu_mode: GpuMode,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_max_test_duration_secs")]
    pub max_test_duration_secs: u64,
    #[serde(default = "default_thermal_mode")]
    pub thermal_mode: String,
    #[serde(default = "default_thermal_cooldown_temp")]
    pub thermal_cooldown_temp: u32,
    #[serde(default = "default_thermal_warmup_temp")]
    pub thermal_warmup_temp: u32,
    #[serde(default = "default_thermal_timeout_secs")]
    pub thermal_timeout_secs: u32,
    #[serde(default)]
    pub custom_runners_dir: String,
    #[serde(default)]
    pub theme: crate::theme::ThemeSettings,
    #[serde(default)]
    pub disable_telemetry_during_runs: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            telemetry_interval_ms: DEFAULT_TELEMETRY_INTERVAL_MS,
            terminal_buffer_lines: DEFAULT_TERMINAL_BUFFER_LINES,
            save_stopped_runs: DEFAULT_SAVE_STOPPED_RUNS,
            selected_gpu_mode: DEFAULT_SELECTED_GPU_MODE,
            language: "system".to_string(),
            max_test_duration_secs: DEFAULT_MAX_TEST_DURATION_SECS,
            thermal_mode: DEFAULT_THERMAL_MODE.to_string(),
            thermal_cooldown_temp: DEFAULT_THERMAL_COOLDOWN_TEMP,
            thermal_warmup_temp: DEFAULT_THERMAL_WARMUP_TEMP,
            thermal_timeout_secs: DEFAULT_THERMAL_TIMEOUT_SECS,
            custom_runners_dir: String::new(),
            theme: crate::theme::ThemeSettings::default(),
            disable_telemetry_during_runs: false,
        }
    }
}

impl AppSettings {
    /// Validates and sanitizes settings values within allowed boundaries.
    pub fn sanitize(&mut self) {
        if !VALID_TELEMETRY_INTERVALS.contains(&self.telemetry_interval_ms) {
            self.telemetry_interval_ms = DEFAULT_TELEMETRY_INTERVAL_MS;
        }

        if !VALID_MAX_TEST_DURATIONS.contains(&self.max_test_duration_secs) {
            self.max_test_duration_secs = DEFAULT_MAX_TEST_DURATION_SECS;
        }

        if self.terminal_buffer_lines < MIN_TERMINAL_BUFFER_LINES
            || self.terminal_buffer_lines > MAX_TERMINAL_BUFFER_LINES
        {
            self.terminal_buffer_lines = self
                .terminal_buffer_lines
                .clamp(MIN_TERMINAL_BUFFER_LINES, MAX_TERMINAL_BUFFER_LINES);
        }

        let lang_lower = self.language.trim().to_lowercase();
        if lang_lower == "tr" || lang_lower == "turkish" {
            self.language = "tr".to_string();
        } else if lang_lower == "en" || lang_lower == "english" {
            self.language = "en".to_string();
        } else {
            self.language = "system".to_string();
        }

        self.thermal_mode = self.thermal_mode.to_lowercase();
        if !VALID_THERMAL_MODES.contains(&self.thermal_mode.as_str()) {
            self.thermal_mode = DEFAULT_THERMAL_MODE.to_string();
        }

        if self.thermal_cooldown_temp < 35 || self.thermal_cooldown_temp > 75 {
            self.thermal_cooldown_temp = self.thermal_cooldown_temp.clamp(35, 75);
        }

        if self.thermal_warmup_temp < 45 || self.thermal_warmup_temp > 95 {
            self.thermal_warmup_temp = self.thermal_warmup_temp.clamp(45, 95);
        }

        if self.thermal_timeout_secs < 5 || self.thermal_timeout_secs > 300 {
            self.thermal_timeout_secs = self.thermal_timeout_secs.clamp(5, 300);
        }

        self.custom_runners_dir = self.custom_runners_dir.trim().to_string();
        self.theme.sanitize();
    }
}

/// Normalizes an execution status string into a canonical lowercase status token.
pub fn normalize_run_status(status: &str) -> &'static str {
    let lower = status.trim().to_lowercase();
    if lower.contains("timeout") && lower.contains("scored") {
        "timeout_scored"
    } else if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("stop") || lower.contains("cancel") {
        "stopped"
    } else if lower.contains("fail")
        || lower.contains("error")
        || lower.contains("crash")
        || lower.contains("abort")
        || lower.contains("kill")
    {
        "failed"
    } else if lower.contains("complete")
        || lower.contains("success")
        || lower == "scored"
        || lower == "ok"
        || lower == "pass"
        || lower == "passed"
    {
        "completed"
    } else {
        "failed"
    }
}

/// Validates an individual CLI argument token for safety.
/// Rejects dangerous flags (-o, --output, -y), path traversal ('..'), absolute root paths ('/'),
/// and dangerous shell injection characters.
pub fn is_safe_custom_arg(arg: &str) -> bool {
    validate_custom_arg(arg).is_ok()
}

/// Filters a string of custom arguments, keeping only safe tokens.
pub fn filter_safe_custom_args(custom_args: &str) -> Vec<String> {
    custom_args
        .split_whitespace()
        .filter(|token| is_safe_custom_arg(token))
        .map(|s| s.to_string())
        .collect()
}

/// Validates an individual custom argument or returns an error string describing why it's unsafe.
pub fn validate_custom_arg(arg: &str) -> Result<(), &'static str> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Err("Argument cannot be empty");
    }
    if trimmed == "-o"
        || trimmed == "--output"
        || trimmed == "-y"
        || trimmed.starts_with("-o=")
        || trimmed.starts_with("--output=")
        || trimmed.starts_with("-y=")
    {
        return Err("Dangerous flag usage (-o, --output, -y) is forbidden");
    }
    if trimmed.contains("..") {
        return Err("Arguments containing path traversal ('..') are forbidden");
    }
    if trimmed.starts_with('/') || trimmed.contains("=/") {
        return Err("Absolute paths referencing root directory ('/') are forbidden");
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n' | '\r'))
    {
        return Err("Arguments containing dangerous shell characters are forbidden");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfmpegConfig {
    pub codec: String,
    pub crf: u32,
    pub preset: String,
    pub resolution: String,
    pub input_source: String,
    pub duration_enabled: bool,
    pub duration_secs: u32,
    pub threads: u32,
    pub custom_args: String,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            codec: "H.264 (libx264 - CPU)".to_string(),
            crf: 23,
            preset: "medium".to_string(),
            resolution: "1080p FHD (1920x1080)".to_string(),
            input_source: "Synthetic Test Source (lavfi testsrc2)".to_string(),
            duration_enabled: true,
            duration_secs: 20,
            threads: 0,
            custom_args: String::new(),
        }
    }
}

impl FfmpegConfig {
    pub fn for_version(_version: &str) -> Self {
        Self::default()
    }

    pub fn to_run_args(&self) -> (Vec<String>, String, String) {
        let codec_lower = self.codec.to_lowercase();
        let codec_raw = if codec_lower.contains("libx265")
            || (codec_lower.contains("h.265") && codec_lower.contains("cpu"))
        {
            "libx265"
        } else if codec_lower.contains("libsvtav1") || codec_lower.contains("svt") {
            "libsvtav1"
        } else if codec_lower.contains("libaom") || codec_lower.contains("aom") {
            "libaom-av1"
        } else if codec_lower.contains("vp9") {
            "libvpx-vp9"
        } else if codec_lower.contains("h264_nvenc")
            || (codec_lower.contains("h.264") && codec_lower.contains("nvenc"))
        {
            "h264_nvenc"
        } else if codec_lower.contains("hevc_nvenc")
            || ((codec_lower.contains("hevc") || codec_lower.contains("h.265"))
                && codec_lower.contains("nvenc"))
        {
            "hevc_nvenc"
        } else if codec_lower.contains("av1_nvenc")
            || (codec_lower.contains("av1") && codec_lower.contains("nvenc"))
        {
            "av1_nvenc"
        } else if codec_lower.contains("h264_vaapi")
            || (codec_lower.contains("h.264") && codec_lower.contains("vaapi"))
        {
            "h264_vaapi"
        } else if codec_lower.contains("hevc_vaapi")
            || ((codec_lower.contains("hevc") || codec_lower.contains("h.265"))
                && codec_lower.contains("vaapi"))
        {
            "hevc_vaapi"
        } else {
            "libx264"
        };

        let res_raw = if self.resolution.contains("3840x2160") || self.resolution.contains("4K") {
            "3840x2160"
        } else if self.resolution.contains("2560x1440")
            || self.resolution.contains("1440p")
            || self.resolution.contains("2K")
        {
            "2560x1440"
        } else if self.resolution.contains("1280x720") || self.resolution.contains("720p") {
            "1280x720"
        } else if self.resolution.contains("854x480") || self.resolution.contains("480p") {
            "854x480"
        } else {
            "1920x1080"
        };

        let preset_raw = self.preset.trim();

        let is_demo_video = self.input_source.contains("Buck Bunny")
            || self.input_source.contains("Tears of Steel");

        let mut args: Vec<String> = vec!["-y".to_string()];

        if is_demo_video {
            let video_file = if self.input_source.contains("Buck Bunny") {
                "big_buck_bunny_1080p.mp4"
            } else {
                "tears_of_steel_1080p.mp4"
            };
            args.push("-i".to_string());
            args.push(video_file.to_string());
            args.push("-vf".to_string());
            args.push(format!("scale={}", res_raw.replace('x', ":")));
        } else {
            let input_src = if self.duration_enabled && self.duration_secs > 0 {
                format!(
                    "testsrc2=duration={}:size={}:rate=60",
                    self.duration_secs, res_raw
                )
            } else {
                format!("testsrc2=size={}:rate=60", res_raw)
            };
            args.push("-f".to_string());
            args.push("lavfi".to_string());
            args.push("-i".to_string());
            args.push(input_src);
        }

        let is_nvenc = codec_raw.ends_with("_nvenc");
        let is_vaapi = codec_raw.ends_with("_vaapi");

        if is_vaapi {
            args.push("-vaapi_device".to_string());
            args.push("/dev/dri/renderD128".to_string());
            args.push("-vf".to_string());
            args.push("format=nv12,hwupload".to_string());
        }

        args.push("-c:v".to_string());
        args.push(codec_raw.to_string());

        if is_nvenc {
            args.push("-cq".to_string());
            args.push(self.crf.to_string());
            args.push("-preset".to_string());
            args.push("p4".to_string());
        } else if is_vaapi {
            args.push("-qp".to_string());
            args.push(self.crf.to_string());
        } else if codec_raw == "libvpx-vp9" {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
        } else if codec_raw == "libsvtav1" {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            let svt_preset = if preset_raw.chars().all(|c| c.is_ascii_digit()) {
                preset_raw
            } else {
                "6"
            };
            args.push("-preset".to_string());
            args.push(svt_preset.to_string());
        } else if codec_raw == "libaom-av1" {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
            args.push("-cpu-used".to_string());
            args.push("4".to_string());
        } else {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            args.push("-preset".to_string());
            args.push(preset_raw.to_string());
        }

        if self.threads > 0 {
            args.push("-threads".to_string());
            args.push(self.threads.to_string());
        }

        args.extend(filter_safe_custom_args(&self.custom_args));

        args.push("-f".to_string());
        args.push("null".to_string());
        args.push("-".to_string());

        let cmd_preview = format!("./ffmpeg {}", args.join(" "));
        let res_name = if res_raw == "3840x2160" {
            "4K"
        } else if res_raw == "1920x1080" {
            "1080p"
        } else if res_raw == "2560x1440" {
            "1440p"
        } else if res_raw == "1280x720" {
            "720p"
        } else {
            res_raw
        };

        let preset_display = format!(
            "{} {} (CRF {}, {})",
            res_name, codec_raw, self.crf, preset_raw
        );

        (args, cmd_preview, preset_display)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SevenZipConfig {
    pub threads: u32,
    pub dict_size: String,
    pub passes: u32,
    pub custom_args: String,
}

impl Default for SevenZipConfig {
    fn default() -> Self {
        Self {
            threads: 0,
            dict_size: "32MB (Default)".to_string(),
            passes: 1,
            custom_args: String::new(),
        }
    }
}

impl SevenZipConfig {
    pub fn to_run_args(&self) -> (Vec<String>, String, String) {
        let mut args = vec!["b".to_string()];

        if self.threads > 0 {
            args.push(format!("-mmt={}", self.threads));
        }

        let dict_flag = if self.dict_size.contains("64") {
            "-md=64m"
        } else if self.dict_size.contains("128") {
            "-md=128m"
        } else if self.dict_size.contains("256") {
            "-md=256m"
        } else if self.dict_size.contains("512") {
            "-md=512m"
        } else if self.dict_size.contains("1024")
            || self.dict_size.contains("1 GB")
            || self.dict_size.contains("1GB")
        {
            "-md=1024m"
        } else {
            "-md=32m"
        };
        args.push(dict_flag.to_string());

        if self.passes > 1 {
            args.push(self.passes.to_string());
        }

        args.extend(filter_safe_custom_args(&self.custom_args));

        let cmd_preview = format!("./7zz {}", args.join(" "));
        let dict_label = dict_flag.trim_start_matches("-md=");
        let threads_label = if self.threads == 0 {
            "Auto".to_string()
        } else {
            self.threads.to_string()
        };
        let preset_display = format!(
            "7z ({}T, {} Dict, {}p)",
            threads_label, dict_label, self.passes
        );

        (args, cmd_preview, preset_display)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnigineConfig {
    pub preset: String,
    pub custom_args: String,
}

impl Default for UnigineConfig {
    fn default() -> Self {
        Self {
            preset: "1080p Extreme".to_string(),
            custom_args: String::new(),
        }
    }
}

impl UnigineConfig {
    pub fn for_version(version: &str) -> Self {
        let preset = if version.contains("Heaven") {
            "Extreme (1080p)".to_string()
        } else {
            "1080p Extreme".to_string()
        };
        Self {
            preset,
            custom_args: String::new(),
        }
    }

    pub fn to_run_args(&self, version: &str) -> (Vec<String>, String, String) {
        let is_heaven = version.contains("Heaven");
        let mut args = Vec::new();

        if is_heaven {
            args.extend(vec![
                "-data_path".to_string(),
                "../".to_string(),
                "-sound_app".to_string(),
                "null".to_string(),
                "-engine_config".to_string(),
                "../data/heaven_4.0.cfg".to_string(),
                "-system_script".to_string(),
                "heaven/unigine.cpp".to_string(),
                "-video_mode".to_string(),
                "-1".to_string(),
                "-extern_define".to_string(),
                "PHORONIX,RELEASE".to_string(),
                "-video_app".to_string(),
                "opengl".to_string(),
                "-video_fullscreen".to_string(),
                "1".to_string(),
            ]);

            if self.preset.contains("720p") || self.preset.contains("Basic") {
                args.push("-video_width".to_string());
                args.push("1280".to_string());
                args.push("-video_height".to_string());
                args.push("720".to_string());
            } else {
                args.push("-video_width".to_string());
                args.push("1920".to_string());
                args.push("-video_height".to_string());
                args.push("1080".to_string());
            }
        } else {
            let (w, h, p_num) = if self.preset.contains("8K") {
                ("7680", "4320", "6")
            } else if self.preset.contains("4K") {
                ("3840", "2160", "5")
            } else if self.preset.contains("720p") || self.preset.contains("Low") {
                ("1280", "720", "1")
            } else if self.preset.contains("Medium") {
                ("1920", "1080", "2")
            } else if self.preset.contains("High") {
                ("1920", "1080", "3")
            } else {
                ("1920", "1080", "4")
            };

            args.extend(vec![
                "-sound_app".to_string(),
                "openal".to_string(),
                "-system_script".to_string(),
                "superposition/system_script.cpp".to_string(),
                "-data_path".to_string(),
                "../".to_string(),
                "-engine_config".to_string(),
                "../data/superposition/unigine.cfg".to_string(),
                "-video_width".to_string(),
                w.to_string(),
                "-video_height".to_string(),
                h.to_string(),
                "-video_mode".to_string(),
                "-1".to_string(),
                "-project_name".to_string(),
                "Superposition".to_string(),
                "-video_resizable".to_string(),
                "1".to_string(),
                "-console_command".to_string(),
                "config_readonly 1 && world_load superposition/superposition".to_string(),
                "-mode".to_string(),
                "2".to_string(),
                "-preset".to_string(),
                p_num.to_string(),
            ]);
        }

        args.extend(filter_safe_custom_args(&self.custom_args));

        let cmd_name = if is_heaven {
            "./heaven_x64"
        } else {
            "./superposition"
        };
        let cmd_preview = format!("{} {}", cmd_name, args.join(" "));
        let preset_display = format!("{} ({})", version, self.preset);

        (args, cmd_preview, preset_display)
    }
}
