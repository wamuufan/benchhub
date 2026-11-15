use serde::{Deserialize, Serialize};

/// System theme information extracted from Linux desktop environments (KDE, GNOME/GTK, or Portal).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemThemeInfo {
    pub desktop_env: String,
    pub theme_name: String,
    pub is_dark: bool,
    pub accent_color: String,
    pub window_bg: String,
    pub view_bg: String,
    pub card_bg: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub border_color: String,
    pub is_fallback: bool,
}

impl Default for SystemThemeInfo {
    fn default() -> Self {
        Self {
            desktop_env: "Standard Desktop".to_string(),
            theme_name: "Default Dark (Fallback)".to_string(),
            is_dark: true,
            accent_color: "#3daee9".to_string(),
            window_bg: "#232629".to_string(),
            view_bg: "#1b1e20".to_string(),
            card_bg: "#31363b".to_string(),
            text_primary: "#eff0f1".to_string(),
            text_secondary: "#bdc3c7".to_string(),
            border_color: "rgba(255, 255, 255, 0.12)".to_string(),
            is_fallback: true,
        }
    }
}

pub const DEFAULT_ACCENT_COLOR: &str = "#3584e4";
pub const DEFAULT_WINDOW_BG: &str = "#232629";
pub const DEFAULT_CARD_BG: &str = "#31363b";
pub const DEFAULT_TEXT_PRIMARY: &str = "#eff0f1";

fn is_valid_hex_color(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn sanitize_hex_or_default(value: &mut String, fallback: &str) {
    let trimmed = value.trim();
    if is_valid_hex_color(trimmed) {
        *value = trimmed.to_lowercase();
    } else {
        *value = fallback.to_string();
    }
}

/// Persistent GUI appearance and scaling settings synchronized through the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    pub mode: String,
    pub custom_enabled: bool,
    pub custom_accent: String,
    pub custom_window_bg: String,
    pub custom_card_bg: String,
    pub custom_text: String,
    pub ui_scale: f32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            custom_enabled: false,
            custom_accent: DEFAULT_ACCENT_COLOR.to_string(),
            custom_window_bg: DEFAULT_WINDOW_BG.to_string(),
            custom_card_bg: DEFAULT_CARD_BG.to_string(),
            custom_text: DEFAULT_TEXT_PRIMARY.to_string(),
            ui_scale: 1.0,
        }
    }
}

impl ThemeSettings {
    /// Normalizes theme fields into safe ranges and valid color strings.
    pub fn sanitize(&mut self) {
        let mode_lower = self.mode.trim().to_lowercase();
        self.mode = match mode_lower.as_str() {
            "auto" | "dark" | "light" | "custom" => mode_lower,
            _ => "auto".to_string(),
        };

        sanitize_hex_or_default(&mut self.custom_accent, DEFAULT_ACCENT_COLOR);
        sanitize_hex_or_default(&mut self.custom_window_bg, DEFAULT_WINDOW_BG);
        sanitize_hex_or_default(&mut self.custom_card_bg, DEFAULT_CARD_BG);
        sanitize_hex_or_default(&mut self.custom_text, DEFAULT_TEXT_PRIMARY);

        if !self.ui_scale.is_finite() {
            self.ui_scale = 1.0;
        } else {
            self.ui_scale = (self.ui_scale.clamp(0.8, 1.5) * 100.0).round() / 100.0;
        }
    }
}
