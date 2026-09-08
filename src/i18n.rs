use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

static TR: OnceLock<HashMap<String, String>> = OnceLock::new();
static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
static LANG: AtomicU8 = AtomicU8::new(0); // 0 = tr, 1 = en

fn init_dicts() {
    TR.get_or_init(|| {
        let tr_str = include_str!("../resources/i18n/tr.toml");
        toml::from_str(tr_str).expect("Failed to parse tr.toml")
    });
    EN.get_or_init(|| {
        let en_str = include_str!("../resources/i18n/en.toml");
        toml::from_str(en_str).expect("Failed to parse en.toml")
    });
}

pub fn normalize_lang(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "en" | "english" => "en",
        _ => "tr",
    }
}

pub fn set_language(lang: &str) {
    let norm = normalize_lang(lang);
    LANG.store(if norm == "en" { 1 } else { 0 }, Ordering::SeqCst);
}

pub fn get_language() -> String {
    if LANG.load(Ordering::SeqCst) == 1 {
        "en".to_string()
    } else {
        "tr".to_string()
    }
}

pub fn t(key: &str) -> String {
    init_dicts();
    let is_en = LANG.load(Ordering::SeqCst) == 1;
    let dict = if is_en {
        EN.get().unwrap()
    } else {
        TR.get().unwrap()
    };

    if let Some(val) = dict.get(key) {
        return val.clone();
    }
    if let Some(val) = TR.get().unwrap().get(key) {
        return val.clone();
    }
    key.to_string()
}

pub fn t_lang(key: &str, lang: &str) -> String {
    init_dicts();
    let norm = normalize_lang(lang);
    let dict = if norm == "en" {
        EN.get().unwrap()
    } else {
        TR.get().unwrap()
    };
    if let Some(val) = dict.get(key) {
        return val.clone();
    }
    if let Some(val) = TR.get().unwrap().get(key) {
        return val.clone();
    }
    key.to_string()
}

/// Format a translation pattern with positional `{}` placeholders.
/// `args` are inserted sequentially into each `{}` occurrence.
/// If there are more `{}` than args, the extra placeholders remain.
/// If there are more args than `{}`, extra args are ignored.
pub fn t_fmt(key: &str, args: &[&str]) -> String {
    let pattern = t(key);
    let mut result = pattern;
    for arg in args {
        if let Some(pos) = result.find("{}") {
            result.replace_range(pos..pos + 2, arg);
        } else {
            break;
        }
    }
    result
}

pub fn localize_throttling(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed == "Yok"
        || trimmed == "-"
        || trimmed == "Hayır"
    {
        return t("none");
    }

    if let (Some(open), Some(close)) = (trimmed.find("(%"), trimmed.find(')')) {
        if open < close {
            let pct = &trimmed[open..=close];
            let lower = trimmed.to_lowercase();
            if lower.contains("termal") || lower.contains("thermal") {
                return format!("{} {}", t("throttling_thermal"), pct);
            } else if lower.contains("güç") || lower.contains("guc") || lower.contains("power") {
                return format!("{} {}", t("throttling_power"), pct);
            } else if lower.contains("donanım")
                || lower.contains("donanim")
                || lower.contains("hardware")
            {
                return format!("{} {}", t("throttling_hardware"), pct);
            }
        }
    } else if let (Some(open), Some(close)) = (trimmed.find('('), trimmed.find(')')) {
        if open < close {
            let inner = &trimmed[open..=close];
            let lower = trimmed.to_lowercase();
            if lower.contains("termal") || lower.contains("thermal") {
                return format!("{} {}", t("throttling_thermal"), inner);
            } else if lower.contains("güç") || lower.contains("guc") || lower.contains("power") {
                return format!("{} {}", t("throttling_power"), inner);
            } else if lower.contains("donanım")
                || lower.contains("donanim")
                || lower.contains("hardware")
            {
                return format!("{} {}", t("throttling_hardware"), inner);
            }
        }
    }

    let lower = trimmed.to_lowercase();
    if lower.contains("termal") || lower.contains("thermal") {
        t("throttling_thermal")
    } else if lower.contains("güç") || lower.contains("guc") || lower.contains("power") {
        t("throttling_power")
    } else if lower.contains("donanım") || lower.contains("donanim") || lower.contains("hardware")
    {
        t("throttling_hardware")
    } else if lower.contains("tespit") || lower.contains("detected") {
        t("detected")
    } else {
        trimmed.to_string()
    }
}

pub fn localize_power_profile(s: &str) -> String {
    let lower = s.to_lowercase();
    if lower.contains("performance") || lower.contains("performans") {
        t("profile_performance")
    } else if lower.contains("balanced") || lower.contains("dengeli") {
        t("profile_balanced")
    } else if lower.contains("power-saver") || lower.contains("tasarruf") || lower.contains("saver")
    {
        t("profile_power_saver")
    } else if lower.contains("metodoloji") || lower.contains("methodology") {
        t("methodology_badge")
    } else if s.is_empty()
        || s == "Yok"
        || s.eq_ignore_ascii_case("none")
        || s == "-"
        || s == "Bilinmiyor"
        || s.eq_ignore_ascii_case("unknown")
    {
        t("unknown")
    } else {
        s.to_string()
    }
}

pub fn localize_status(s: &str) -> String {
    let trimmed = s.trim();
    let lower = trimmed.to_lowercase();
    if trimmed.contains("TAMAM")
        || lower.contains("tamam")
        || lower.contains("completed")
        || lower.contains("başarılı")
        || lower.contains("basarili")
        || lower == "success"
    {
        t("status_completed")
    } else if trimmed.contains("HATA")
        || lower.contains("hata")
        || lower.contains("error")
        || lower.contains("başarısız")
        || lower.contains("basarisiz")
        || lower.contains("failed")
        || lower == "fail"
    {
        t("status_error")
    } else if trimmed.contains("İPTAL")
        || lower.contains("iptal")
        || lower.contains("durduruldu")
        || lower.contains("cancel")
        || lower.contains("stopped")
    {
        t("status_cancelled")
    } else {
        trimmed.to_string()
    }
}

pub fn localize_gpu_mode(s: &str) -> String {
    let lower = s.trim().to_lowercase();
    if lower.contains("karma") || lower.contains("mixed") {
        t("gpu_mode_mixed")
    } else {
        match crate::models::GpuMode::from_display_str(s) {
            crate::models::GpuMode::NvidiaDgpu => t("gpu_mode_nvidia"),
            crate::models::GpuMode::Integrated => t("gpu_mode_igpu"),
            crate::models::GpuMode::Auto => t("gpu_mode_default"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_parity() {
        let tr_str = include_str!("../resources/i18n/tr.toml");
        let en_str = include_str!("../resources/i18n/en.toml");
        let tr: HashMap<String, String> = toml::from_str(tr_str).unwrap();
        let en: HashMap<String, String> = toml::from_str(en_str).unwrap();

        for key in tr.keys() {
            assert!(en.contains_key(key), "Key '{}' missing in en.toml", key);
        }
        for key in en.keys() {
            assert!(tr.contains_key(key), "Key '{}' missing in tr.toml", key);
        }
    }

    #[test]
    fn test_fallback() {
        assert_eq!(t("nonexistent_key"), "nonexistent_key");
    }

    #[test]
    fn test_language_switch_and_helpers() {
        set_language("en");
        assert_eq!(t("ready"), "Ready");
        assert_eq!(t_lang("ready", "tr"), "Hazır");
        assert_eq!(t_lang("ready", "en"), "Ready");
        assert_eq!(localize_throttling("Yok"), "None");
        assert_eq!(localize_throttling("None"), "None");
        assert_eq!(localize_throttling("none"), "None");
        assert_eq!(localize_throttling("-"), "None");
        assert_eq!(localize_throttling("Termal Kısma"), "Thermal Throttling");
        assert_eq!(
            localize_throttling("thermal_throttle"),
            "Thermal Throttling"
        );
        assert_eq!(
            localize_throttling("Termal Kısma (%45)"),
            "Thermal Throttling (%45)"
        );
        assert_eq!(
            localize_throttling("thermal_throttle (%45)"),
            "Thermal Throttling (%45)"
        );
        assert_eq!(localize_throttling("Güç Limiti (%20)"), "Power Limit (%20)");
        assert_eq!(
            localize_throttling("power_limit (%20)"),
            "Power Limit (%20)"
        );
        assert_eq!(
            localize_throttling("Donanım Kısması"),
            "Hardware Throttling"
        );
        assert_eq!(
            localize_throttling("hardware_throttle"),
            "Hardware Throttling"
        );
        assert_eq!(localize_power_profile("performance"), "Performance");
        assert_eq!(localize_power_profile("balanced"), "Balanced");
        assert_eq!(localize_power_profile("power-saver"), "Power Saver");
        assert_eq!(localize_power_profile("Metodoloji"), "METHODOLOGY");
        assert_eq!(localize_status("TAMAMLANDI"), "COMPLETED");
        assert_eq!(localize_status("completed"), "COMPLETED");
        assert_eq!(localize_status("Tamamlandı"), "COMPLETED");
        assert_eq!(localize_status("HATA"), "ERROR");
        assert_eq!(localize_status("error"), "ERROR");
        assert_eq!(localize_status("İPTAL"), "CANCELLED");
        assert_eq!(localize_status("cancelled"), "CANCELLED");
        assert_eq!(localize_gpu_mode("Karma"), "Mixed");
        assert_eq!(localize_gpu_mode("mixed"), "Mixed");
        assert_eq!(
            localize_gpu_mode("Harici GPU (NVIDIA)"),
            "Dedicated GPU NVIDIA"
        );
        assert_eq!(localize_gpu_mode("Dahili GPU (iGPU)"), "Integrated GPU");

        set_language("tr");
        assert_eq!(t("ready"), "Hazır");
        assert_eq!(localize_throttling("Yok"), "Yok");
        assert_eq!(localize_throttling("None"), "Yok");
        assert_eq!(localize_throttling("none"), "Yok");
        assert_eq!(
            localize_throttling("Termal Kısma (%45)"),
            "Termal Kısma (%45)"
        );
        assert_eq!(
            localize_throttling("thermal_throttle (%45)"),
            "Termal Kısma (%45)"
        );
        assert_eq!(localize_power_profile("performance"), "Performans");
        assert_eq!(localize_power_profile("Metodoloji"), "METODOLOJİ");
        assert_eq!(localize_status("completed"), "TAMAMLANDI");
        assert_eq!(localize_status("Tamamlandı"), "TAMAMLANDI");
        assert_eq!(localize_gpu_mode("mixed"), "Karma");
        assert_eq!(localize_gpu_mode("Karma"), "Karma");
    }
}
