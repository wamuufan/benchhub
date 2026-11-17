//! Linux Desktop Theme Synchronization & Reactive Styling Engine
//!
//! Synchronizes KDE Plasma (Breeze) and GNOME/GTK (Libadwaita) color schemes
//! with the Leptos CSR frontend, persists overrides cleanly through `benchhubd`
//! (`AppSettings.theme`), and performs live 2500ms + focus polling.

use crate::tauri_bridge;
use benchhub_types::{SystemThemeInfo, ThemeSettings};
use leptos::prelude::*;

/// Sanitizes a string into a strict 7-character `#rrggbb` hex color.
pub fn to_valid_hex_7(s: &str, fallback: &str) -> String {
    let s = s.trim();
    if s.starts_with('#') && s.len() == 7 && s[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        s.to_string()
    } else if s.starts_with('#') && s.len() == 4 && s[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        let chars: Vec<char> = s[1..].chars().collect();
        format!("#{0}{0}{1}{1}{2}{2}", chars[0], chars[1], chars[2])
    } else {
        fallback.to_string()
    }
}

/// Calculates perceptual luminance to determine whether a hex color is dark.
pub fn is_color_dark(hex: &str) -> bool {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0) as f64;
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0) as f64;
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0) as f64;
        (0.299 * r + 0.587 * g + 0.114 * b) < 140.0
    } else {
        true
    }
}

/// Asynchronous sleep helper using `window.setTimeout`.
pub async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(win) = web_sys::window() {
            let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Reactive state holding active desktop theme and daemon-persisted overrides.
#[derive(Clone, Copy)]
pub struct ThemeState {
    pub sys_theme_info: RwSignal<SystemThemeInfo>,
    pub theme_mode: RwSignal<String>, // "system", "dark", "light"
    pub custom_theme_enabled: RwSignal<bool>,
    pub custom_accent_color: RwSignal<String>,
    pub custom_window_bg: RwSignal<String>,
    pub custom_card_bg: RwSignal<String>,
    pub custom_text_color: RwSignal<String>,
    pub ui_scale: RwSignal<String>, // "90", "100", "110", "125", "150"
}

impl ThemeState {
    /// Initializes theme state with defaults until `AppSettings.theme` arrives from `benchhubd`.
    pub fn new() -> Self {
        let def = ThemeSettings::default();
        Self {
            sys_theme_info: RwSignal::new(SystemThemeInfo::default()),
            theme_mode: RwSignal::new(def.mode),
            custom_theme_enabled: RwSignal::new(def.custom_enabled),
            custom_accent_color: RwSignal::new(def.custom_accent),
            custom_window_bg: RwSignal::new(def.custom_window_bg),
            custom_card_bg: RwSignal::new(def.custom_card_bg),
            custom_text_color: RwSignal::new(def.custom_text),
            ui_scale: RwSignal::new(scale_float_to_percent_str(def.ui_scale)),
        }
    }

    /// Applies daemon-stored `ThemeSettings` into reactive GUI signals.
    pub fn apply_settings(&self, theme: &ThemeSettings) {
        self.theme_mode.set(theme.mode.clone());
        self.custom_theme_enabled.set(theme.custom_enabled);
        self.custom_accent_color
            .set(to_valid_hex_7(&theme.custom_accent, "#3584e4"));
        self.custom_window_bg
            .set(to_valid_hex_7(&theme.custom_window_bg, "#242424"));
        self.custom_card_bg
            .set(to_valid_hex_7(&theme.custom_card_bg, "#303030"));
        self.custom_text_color
            .set(to_valid_hex_7(&theme.custom_text, "#ffffff"));
        self.ui_scale
            .set(scale_float_to_percent_str(theme.ui_scale));
    }

    /// Serializes current UI theme signals back into `ThemeSettings` for daemon persistence.
    pub fn to_theme_settings(self) -> ThemeSettings {
        let scale_pct = self.ui_scale.get_untracked();
        let ui_scale = match scale_pct.as_str() {
            "90" => 0.9,
            "110" => 1.1,
            "125" => 1.25,
            "150" => 1.5,
            _ => 1.0,
        };
        let mut settings = ThemeSettings {
            mode: self.theme_mode.get_untracked(),
            custom_enabled: self.custom_theme_enabled.get_untracked(),
            custom_accent: self.custom_accent_color.get_untracked(),
            custom_window_bg: self.custom_window_bg.get_untracked(),
            custom_card_bg: self.custom_card_bg.get_untracked(),
            custom_text: self.custom_text_color.get_untracked(),
            ui_scale,
        };
        settings.sanitize();
        settings
    }

    /// Starts live theme polling loop (2500ms) and window focus trigger when system mode is active.
    pub fn start_sync(&self) {
        let sys_info = self.sys_theme_info;
        let theme_mode = self.theme_mode;
        let custom_enabled = self.custom_theme_enabled;
        let is_system_active = move || {
            let mode = theme_mode.get_untracked();
            (mode == "system" || mode == "auto") && !custom_enabled.get_untracked()
        };

        // Immediate fetch on mount
        leptos::task::spawn_local(async move {
            if let Ok(theme) = tauri_bridge::get_system_theme_info().await {
                sys_info.set(theme);
            }
        });

        // 2500ms polling loop (only when system theme synchronization is active)
        leptos::task::spawn_local(async move {
            loop {
                sleep_ms(2500).await;
                if !is_system_active() {
                    continue;
                }
                if let Ok(fresh) = tauri_bridge::get_system_theme_info().await {
                    if sys_info.get_untracked() != fresh {
                        sys_info.set(fresh);
                    }
                }
            }
        });

        // Window focus event listener
        if let Some(win) = web_sys::window() {
            let cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
                if !is_system_active() {
                    return;
                }
                leptos::task::spawn_local(async move {
                    if let Ok(fresh) = tauri_bridge::get_system_theme_info().await {
                        if sys_info.get_untracked() != fresh {
                            sys_info.set(fresh);
                        }
                    }
                });
            });
            let _ = win.add_event_listener_with_callback(
                "focus",
                wasm_bindgen::JsCast::unchecked_ref(cb.as_ref()),
            );
            cb.forget();
        }
    }

    /// Computes dynamic HTML root classes (`app-root {theme-dark|theme-light} {de-kde|de-gnome}`).
    pub fn compute_classes(&self) -> String {
        let de_class = if self
            .sys_theme_info
            .get()
            .desktop_env
            .to_uppercase()
            .contains("KDE")
        {
            "de-kde"
        } else {
            "de-gnome"
        };

        let theme_class = if self.custom_theme_enabled.get() {
            if is_color_dark(&self.custom_window_bg.get()) {
                "theme-dark"
            } else {
                "theme-light"
            }
        } else {
            match self.theme_mode.get().as_str() {
                "dark" => "theme-dark",
                "light" => "theme-light",
                _ => {
                    if self.sys_theme_info.get().is_dark {
                        "theme-dark"
                    } else {
                        "theme-light"
                    }
                }
            }
        };

        format!("app-root {} {}", theme_class, de_class)
    }

    /// Injects dynamic CSS variables directly into root container style attribute.
    pub fn compute_styles(&self) -> String {
        let info = self.sys_theme_info.get();
        let is_custom = self.custom_theme_enabled.get();

        let (w_bg, v_bg, c_bg, acc, text_p, text_s, border) = if is_custom {
            let w = self.custom_window_bg.get();
            let c = self.custom_card_bg.get();
            let acc = self.custom_accent_color.get();
            let tp = self.custom_text_color.get();
            let dark = is_color_dark(&w);
            let ts = if dark {
                "rgba(255, 255, 255, 0.7)".to_string()
            } else {
                "rgba(0, 0, 0, 0.65)".to_string()
            };
            let brd = if dark {
                "rgba(255, 255, 255, 0.12)".to_string()
            } else {
                "rgba(0, 0, 0, 0.12)".to_string()
            };
            (w.clone(), w, c, acc, tp, ts, brd)
        } else {
            let effective_dark = match self.theme_mode.get().as_str() {
                "dark" => true,
                "light" => false,
                _ => info.is_dark,
            };

            let (w_bg, v_bg, c_bg, text_p, text_s, border) =
                if self.theme_mode.get() == "system" || effective_dark == info.is_dark {
                    (
                        info.window_bg,
                        info.view_bg,
                        info.card_bg,
                        info.text_primary,
                        info.text_secondary,
                        info.border_color,
                    )
                } else if effective_dark {
                    (
                        "#232629".to_string(),
                        "#1b1e20".to_string(),
                        "#31363b".to_string(),
                        "#eff0f1".to_string(),
                        "#bdc3c7".to_string(),
                        "rgba(255, 255, 255, 0.12)".to_string(),
                    )
                } else {
                    (
                        "#eff0f1".to_string(),
                        "#fcfcfc".to_string(),
                        "#ffffff".to_string(),
                        "#232629".to_string(),
                        "#6e747b".to_string(),
                        "rgba(0, 0, 0, 0.12)".to_string(),
                    )
                };
            (w_bg, v_bg, c_bg, info.accent_color, text_p, text_s, border)
        };

        let factor = match self.ui_scale.get().as_str() {
            "90" => "0.9",
            "100" => "1.0",
            "110" => "1.1",
            "125" => "1.25",
            "150" => "1.5",
            _ => "1.0",
        };

        format!(
            "--ui-scale: {}; --window-bg: {}; --view-bg: {}; --card-bg: {}; --accent-color: {}; --text-primary: {}; --text-secondary: {}; --card-border: {}; background: {}; color: {};",
            factor, w_bg, v_bg, c_bg, acc, text_p, text_s, border, w_bg, text_p
        )
    }
}

fn scale_float_to_percent_str(factor: f32) -> String {
    let pct = (factor * 100.0).round() as i32;
    match pct {
        90 => "90".to_string(),
        110 => "110".to_string(),
        125 => "125".to_string(),
        150 => "150".to_string(),
        _ => "100".to_string(),
    }
}
