use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_TELEMETRY_INTERVAL_MS: u64 = 500;
pub const DEFAULT_TERMINAL_BUFFER_LINES: usize = 150;
pub const DEFAULT_SAVE_STOPPED_RUNS: bool = true;
pub const DEFAULT_SELECTED_GPU_MODE: &str = "Harici GPU (NVIDIA)";
pub const DEFAULT_MAX_TEST_DURATION_SECS: u64 = 600;

pub const VALID_TELEMETRY_INTERVALS: [u64; 4] = [250, 500, 1000, 2000];
pub const VALID_MAX_TEST_DURATIONS: [u64; 6] = [0, 60, 120, 180, 300, 600];
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

fn default_selected_gpu_mode() -> String {
    DEFAULT_SELECTED_GPU_MODE.to_string()
}

fn default_language() -> String {
    "tr".to_string()
}

fn default_max_test_duration_secs() -> u64 {
    DEFAULT_MAX_TEST_DURATION_SECS
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_telemetry_interval_ms")]
    pub telemetry_interval_ms: u64,
    #[serde(default = "default_terminal_buffer_lines")]
    pub terminal_buffer_lines: usize,
    #[serde(default = "default_save_stopped_runs")]
    pub save_stopped_runs: bool,
    #[serde(default = "default_selected_gpu_mode")]
    pub selected_gpu_mode: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_max_test_duration_secs")]
    pub max_test_duration_secs: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            telemetry_interval_ms: DEFAULT_TELEMETRY_INTERVAL_MS,
            terminal_buffer_lines: DEFAULT_TERMINAL_BUFFER_LINES,
            save_stopped_runs: DEFAULT_SAVE_STOPPED_RUNS,
            selected_gpu_mode: DEFAULT_SELECTED_GPU_MODE.to_string(),
            language: "tr".to_string(),
            max_test_duration_secs: DEFAULT_MAX_TEST_DURATION_SECS,
        }
    }
}

impl AppSettings {
    /// Validates and sanitizes settings values within allowed boundaries.
    pub fn sanitize(&mut self) {
        if !VALID_TELEMETRY_INTERVALS.contains(&self.telemetry_interval_ms) {
            tracing::warn!(
                "Geçersiz telemetri aralığı: {}ms. {}ms varsayılanına dönülüyor.",
                self.telemetry_interval_ms,
                DEFAULT_TELEMETRY_INTERVAL_MS
            );
            self.telemetry_interval_ms = DEFAULT_TELEMETRY_INTERVAL_MS;
        }

        if !VALID_MAX_TEST_DURATIONS.contains(&self.max_test_duration_secs) {
            tracing::warn!(
                "Geçersiz test süresi limiti: {}s. {}s varsayılanına dönülüyor.",
                self.max_test_duration_secs,
                DEFAULT_MAX_TEST_DURATION_SECS
            );
            self.max_test_duration_secs = DEFAULT_MAX_TEST_DURATION_SECS;
        }

        if self.terminal_buffer_lines < MIN_TERMINAL_BUFFER_LINES
            || self.terminal_buffer_lines > MAX_TERMINAL_BUFFER_LINES
        {
            let clamped = self
                .terminal_buffer_lines
                .clamp(MIN_TERMINAL_BUFFER_LINES, MAX_TERMINAL_BUFFER_LINES);
            tracing::warn!(
                "Konsol satır sınırı sınır dışı: {}. {} değerine sınırlandı.",
                self.terminal_buffer_lines,
                clamped
            );
            self.terminal_buffer_lines = clamped;
        }

        let mode = crate::models::GpuMode::from_display_str(&self.selected_gpu_mode);
        let normalized = mode.to_display_str().to_string();
        if self.selected_gpu_mode != normalized {
            tracing::warn!(
                "Geçersiz veya standart dışı GPU modu: '{}'. '{}' olarak normalize edildi.",
                self.selected_gpu_mode,
                normalized
            );
            self.selected_gpu_mode = normalized;
        }

        if self.language != "tr"
            && self.language != "en"
            && self.language != "Türkçe"
            && self.language != "English"
        {
            tracing::warn!(
                "Geçersiz dil: '{}'. 'tr' varsayılanına dönülüyor.",
                self.language
            );
            self.language = "tr".to_string();
        } else if self.language == "Türkçe" {
            self.language = "tr".to_string();
        } else if self.language == "English" {
            self.language = "en".to_string();
        }
    }
}

pub fn parse_max_test_duration(s: &str) -> u64 {
    if s.contains("600") {
        600
    } else if s.contains("300") {
        300
    } else if s.contains("180") {
        180
    } else if s.contains("120") {
        120
    } else if s.contains("60") {
        60
    } else if s.contains("Sınırsız")
        || s.contains("Unlimited")
        || s.contains("Süresiz")
        || s.contains("Limit Yok")
        || s.contains("No Limit")
    {
        0
    } else {
        let digits: String = s
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().unwrap_or(0)
    }
}

pub fn format_max_test_duration(secs: u64, lang: &str) -> String {
    let is_en = lang == "en";
    match secs {
        60 => {
            if is_en {
                "60 seconds (1 min)".to_string()
            } else {
                "60 saniye (1 dk)".to_string()
            }
        }
        120 => {
            if is_en {
                "120 seconds (2 min)".to_string()
            } else {
                "120 saniye (2 dk)".to_string()
            }
        }
        180 => {
            if is_en {
                "180 seconds (3 min)".to_string()
            } else {
                "180 saniye (3 dk)".to_string()
            }
        }
        300 => {
            if is_en {
                "300 seconds (5 min - Recommended)".to_string()
            } else {
                "300 saniye (5 dk - Önerilen)".to_string()
            }
        }
        600 => {
            if is_en {
                "600 seconds (10 min)".to_string()
            } else {
                "600 saniye (10 dk)".to_string()
            }
        }
        _ => {
            if is_en {
                "Unlimited (No Limit)".to_string()
            } else {
                "Sınırsız (Limit Yok)".to_string()
            }
        }
    }
}

/// Loads and validates application settings from the specified JSON file path.
/// If the file does not exist, or contains invalid data/JSON, logs a warning
/// and gracefully falls back to sanitized default settings without overwriting user data.
pub fn load_settings(path: &Path) -> AppSettings {
    if !path.exists() {
        tracing::debug!(
            "Ayar dosyası bulunamadı ({:?}), varsayılan ayarlar yükleniyor.",
            path
        );
        return AppSettings::default();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(e) => {
                tracing::warn!(
                    "Ayar dosyası ({:?}) okunamadı veya bozuk: {}. Varsayılan ayarlar kullanılıyor.",
                    path,
                    e
                );
                AppSettings::default()
            }
        },
        Err(e) => {
            tracing::warn!(
                "Ayar dosyası ({:?}) okunamadı: {}. Varsayılan ayarlar kullanılıyor.",
                path,
                e
            );
            AppSettings::default()
        }
    }
}

/// Atomically saves sanitized application settings to the specified path.
/// Writes to a temporary file and renames it to prevent corruption during unexpected shutdowns.
pub fn save_settings(path: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    let mut validated = settings.clone();
    validated.sanitize();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Ayar dizini oluşturulamadı: {:?}", parent))?;
    }

    let json_str = serde_json::to_string_pretty(&validated)
        .context("Ayar nesnesi JSON formatına dönüştürülemedi")?;

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp_path)
            .with_context(|| format!("Geçici ayar dosyası oluşturulamadı: {:?}", tmp_path))?;
        file.write_all(json_str.as_bytes())
            .with_context(|| format!("Geçici ayar dosyasına yazılamadı: {:?}", tmp_path))?;
        file.flush().with_context(|| {
            format!("Geçici ayar dosyası boşaltılamadı (flush): {:?}", tmp_path)
        })?;
        file.sync_all().with_context(|| {
            format!(
                "Geçici ayar dosyası senkronize edilemedi (sync): {:?}",
                tmp_path
            )
        })?;
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Geçici ayar dosyası {:?} -> {:?} olarak yeniden adlandırılamadı",
            tmp_path, path
        )
    })?;

    tracing::debug!("Ayarlar başarıyla kaydedildi: {:?}", path);
    Ok(())
}

/// Helper function to load, modify, and atomically persist settings.
pub fn update_settings<F>(path: &Path, updater: F) -> anyhow::Result<AppSettings>
where
    F: FnOnce(&mut AppSettings),
{
    let mut settings = load_settings(path);
    updater(&mut settings);
    save_settings(path, &settings)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_settings() {
        let defaults = AppSettings::default();
        assert_eq!(defaults.telemetry_interval_ms, 500);
        assert_eq!(defaults.terminal_buffer_lines, 150);
        assert!(defaults.save_stopped_runs);
        assert_eq!(defaults.selected_gpu_mode, "Harici GPU (NVIDIA)");
        assert_eq!(defaults.language, "tr");
        assert_eq!(defaults.max_test_duration_secs, 600);
    }

    #[test]
    fn test_boundary_clamping_and_sanitization() {
        let mut s = AppSettings {
            telemetry_interval_ms: 123, // invalid
            terminal_buffer_lines: 10,  // below 50
            save_stopped_runs: true,
            selected_gpu_mode: "invalid_mode".into(),
            language: "invalid_lang".into(),
            max_test_duration_secs: 9999, // invalid
        };
        s.sanitize();
        assert_eq!(s.telemetry_interval_ms, 500);
        assert_eq!(s.terminal_buffer_lines, 50);
        assert_eq!(s.selected_gpu_mode, "System Default");
        assert_eq!(s.language, "tr");
        assert_eq!(s.max_test_duration_secs, 600);

        let mut s2 = AppSettings {
            telemetry_interval_ms: 1000,
            terminal_buffer_lines: 5000, // above 1000
            save_stopped_runs: false,
            selected_gpu_mode: "iGPU Dahili".into(),
            language: "English".into(),
            max_test_duration_secs: 180,
        };
        s2.sanitize();
        assert_eq!(s2.telemetry_interval_ms, 1000);
        assert_eq!(s2.terminal_buffer_lines, 1000);
        assert_eq!(s2.selected_gpu_mode, "Dahili GPU (iGPU)");
        assert_eq!(s2.language, "en");
        assert_eq!(s2.max_test_duration_secs, 180);
    }

    #[test]
    fn test_invalid_json_recovery() {
        let tmp_dir =
            std::env::temp_dir().join(format!("benchhub_test_config_{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp_dir);
        let config_file = tmp_dir.join("invalid_config.json");

        fs::write(&config_file, "{ this is not valid json! }").unwrap();

        let loaded = load_settings(&config_file);
        assert_eq!(loaded, AppSettings::default());

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_missing_fields_recovery() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "benchhub_test_config_missing_{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&tmp_dir);
        let config_file = tmp_dir.join("partial_config.json");

        fs::write(&config_file, r#"{"telemetry_interval_ms": 250}"#).unwrap();

        let loaded = load_settings(&config_file);
        assert_eq!(loaded.telemetry_interval_ms, 250);
        assert_eq!(loaded.terminal_buffer_lines, 150);
        assert!(loaded.save_stopped_runs);
        assert_eq!(loaded.selected_gpu_mode, "Harici GPU (NVIDIA)");

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "benchhub_test_config_roundtrip_{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&tmp_dir);
        let config_file = tmp_dir.join("test_config.json");

        let settings = AppSettings {
            telemetry_interval_ms: 2000,
            terminal_buffer_lines: 450,
            save_stopped_runs: false,
            selected_gpu_mode: "Dahili GPU (iGPU)".into(),
            language: "tr".into(),
            max_test_duration_secs: 120,
        };

        save_settings(&config_file, &settings).unwrap();

        let loaded = load_settings(&config_file);
        assert_eq!(settings, loaded);

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_update_settings_helper() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "benchhub_test_config_update_{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&tmp_dir);
        let config_file = tmp_dir.join("test_update_config.json");

        let initial = update_settings(&config_file, |s| {
            s.telemetry_interval_ms = 1000;
        })
        .unwrap();
        assert_eq!(initial.telemetry_interval_ms, 1000);

        let reloaded = load_settings(&config_file);
        assert_eq!(reloaded.telemetry_interval_ms, 1000);

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_parse_and_format_max_test_duration() {
        assert_eq!(parse_max_test_duration("60 saniye (1 dk)"), 60);
        assert_eq!(parse_max_test_duration("60 seconds (1 min)"), 60);
        assert_eq!(parse_max_test_duration("120 saniye (2 dk)"), 120);
        assert_eq!(parse_max_test_duration("120 seconds (2 min)"), 120);
        assert_eq!(parse_max_test_duration("180 saniye (3 dk)"), 180);
        assert_eq!(parse_max_test_duration("180 seconds (3 min)"), 180);
        assert_eq!(parse_max_test_duration("300 saniye (5 dk - Önerilen)"), 300);
        assert_eq!(
            parse_max_test_duration("300 seconds (5 min - Recommended)"),
            300
        );
        assert_eq!(parse_max_test_duration("600 saniye (10 dk)"), 600);
        assert_eq!(parse_max_test_duration("600 seconds (10 min)"), 600);
        assert_eq!(parse_max_test_duration("Sınırsız (Limit Yok)"), 0);
        assert_eq!(parse_max_test_duration("Unlimited (No Limit)"), 0);
        assert_eq!(parse_max_test_duration("Süresiz"), 0);
        assert_eq!(parse_max_test_duration("unknown text"), 0);

        assert_eq!(format_max_test_duration(60, "tr"), "60 saniye (1 dk)");
        assert_eq!(format_max_test_duration(60, "en"), "60 seconds (1 min)");
        assert_eq!(
            format_max_test_duration(300, "tr"),
            "300 saniye (5 dk - Önerilen)"
        );
        assert_eq!(
            format_max_test_duration(300, "en"),
            "300 seconds (5 min - Recommended)"
        );
        assert_eq!(format_max_test_duration(0, "tr"), "Sınırsız (Limit Yok)");
        assert_eq!(format_max_test_duration(0, "en"), "Unlimited (No Limit)");
    }
}
