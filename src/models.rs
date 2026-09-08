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
}

impl BenchmarkProfile {
    /// Returns the command or binary path used to run the benchmark
    pub fn get_run_cmd(&self) -> String {
        if !self.run_cmd.is_empty() {
            self.run_cmd.clone()
        } else if let Some(ref rel) = self.binary_relative_path {
            if rel.starts_with("./") || rel.starts_with('/') {
                rel.clone()
            } else {
                format!("./{}", rel)
            }
        } else {
            String::new()
        }
    }

    /// Formats the download size for display in the UI
    pub fn display_download_size(&self) -> String {
        if let Some(ref s) = self.download_size {
            s.clone()
        } else if let Some(mb) = self.approx_size_mb {
            format!("~{} MB", mb)
        } else {
            "Bilinmiyor".to_string()
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
    #[serde(default)]
    pub cpu_throttle: String,
    #[serde(default)]
    pub gpu_throttle: String,
}

fn default_throttle_str() -> String {
    "none".to_string()
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

fn default_power_profile_str() -> String {
    "Bilinmiyor".to_string()
}

pub use crate::config::AppSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GpuMode {
    #[default]
    NvidiaDgpu, // Harici NVIDIA GPU
    Integrated, // Dahili iGPU
    Auto,       // Sistem Varsayılanı
}

impl GpuMode {
    pub fn from_display_str(s: &str) -> Self {
        if s.contains("dGPU")
            || s.contains("NVIDIA")
            || s.contains("Harici")
            || s.contains("Dedicated")
        {
            GpuMode::NvidiaDgpu
        } else if s.contains("iGPU") || s.contains("Dahili") || s.contains("Integrated") {
            GpuMode::Integrated
        } else {
            GpuMode::Auto
        }
    }

    pub fn to_display_str(&self) -> &'static str {
        match self {
            GpuMode::NvidiaDgpu => "Harici GPU (NVIDIA)",
            GpuMode::Integrated => "Dahili GPU (iGPU)",
            GpuMode::Auto => "Sistem Varsayılanı",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HistorySortOrder {
    #[default]
    DateDesc, // Tarih (En Yeni)
    DateAsc,     // Tarih (En Eski)
    ScoreDesc,   // Skor (En Yüksek)
    ScoreAsc,    // Skor (En Düşük)
    CpuTempPeak, // CPU Sıcaklığı (Pik)
    GpuTempPeak, // GPU Sıcaklığı (Pik)
    PowerAvg,    // Güç Tüketimi (Ortalama)
}

impl HistorySortOrder {
    pub fn from_display_str(s: &str) -> Self {
        if s.contains("En Eski") || s.contains("Oldest") {
            HistorySortOrder::DateAsc
        } else if (s.contains("Skor") || s.contains("Score"))
            && (s.contains("Yüksek") || s.contains("High") || s.contains("Azalan"))
        {
            HistorySortOrder::ScoreDesc
        } else if (s.contains("Skor") || s.contains("Score"))
            && (s.contains("Düşük") || s.contains("Low") || s.contains("Artan"))
        {
            HistorySortOrder::ScoreAsc
        } else if s.contains("CPU") {
            HistorySortOrder::CpuTempPeak
        } else if s.contains("GPU") {
            HistorySortOrder::GpuTempPeak
        } else if s.contains("Güç") || s.contains("Power") {
            HistorySortOrder::PowerAvg
        } else {
            HistorySortOrder::DateDesc
        }
    }

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
