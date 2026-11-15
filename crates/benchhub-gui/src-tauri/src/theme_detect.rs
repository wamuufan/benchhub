//! Linux Desktop Environment Theme and Locale Detection.

use benchhub_types::SystemThemeInfo;

/// Detects the user's preferred desktop locale from POSIX environment variables.
pub fn detect_system_locale() -> String {
    std::env::var("LC_ALL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("LC_MESSAGES")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| std::env::var("LANG").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "en".to_string())
}

fn parse_rgb_to_hex(val: &str) -> Option<String> {
    let clean = val.trim();
    if clean.starts_with('#') {
        return Some(clean.to_string());
    }
    let parts: Vec<&str> = clean.split(',').collect();
    if parts.len() == 3 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            parts[0].trim().parse::<u8>(),
            parts[1].trim().parse::<u8>(),
            parts[2].trim().parse::<u8>(),
        ) {
            return Some(format!("#{:02x}{:02x}{:02x}", r, g, b));
        }
    }
    None
}

fn rgb_luminance(val: &str) -> Option<f32> {
    let clean = val.trim();
    let parts: Vec<&str> = clean.split(',').collect();
    if parts.len() == 3 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            parts[0].trim().parse::<f32>(),
            parts[1].trim().parse::<f32>(),
            parts[2].trim().parse::<f32>(),
        ) {
            return Some(0.299 * r + 0.587 * g + 0.114 * b);
        }
    }
    None
}

fn pick(is_dark: bool, dark: &str, light: &str) -> String {
    if is_dark { dark } else { light }.to_string()
}

#[derive(Default)]
struct KdeIniPalette {
    color_scheme: String,
    look_and_feel: String,
    accent_color: Option<String>,
    window_bg: Option<String>,
    window_fg: Option<String>,
    view_bg: Option<String>,
    button_bg: Option<String>,
    selection_bg: Option<String>,
}

impl KdeIniPalette {
    fn parse_into(&mut self, content: &str, fill_meta: bool) {
        let mut current_section = "";
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(sec) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                current_section = sec;
                continue;
            }
            let Some((k, v)) = trimmed.split_once('=') else {
                continue;
            };
            let key = k.trim();
            let val = v.trim();
            match (current_section, key) {
                ("General", "ColorScheme") if fill_meta => self.color_scheme = val.to_string(),
                ("General", "AccentColor") if fill_meta => {
                    self.accent_color = parse_rgb_to_hex(val)
                }
                ("KDE", "LookAndFeelPackage") if fill_meta => self.look_and_feel = val.to_string(),
                ("Colors:Window", "BackgroundNormal") => {
                    self.window_bg.get_or_insert_with(|| val.to_string());
                }
                ("Colors:Window", "ForegroundNormal") => {
                    self.window_fg.get_or_insert_with(|| val.to_string());
                }
                ("Colors:View", "BackgroundNormal") => {
                    self.view_bg.get_or_insert_with(|| val.to_string());
                }
                ("Colors:Button", "BackgroundNormal") => {
                    self.button_bg.get_or_insert_with(|| val.to_string());
                }
                ("Colors:Selection", "BackgroundNormal") => {
                    self.selection_bg.get_or_insert_with(|| val.to_string());
                }
                _ => {}
            }
        }
    }
}

fn detect_kde_theme(home: &str) -> Option<SystemThemeInfo> {
    let kdeglobals_path = std::path::Path::new(home).join(".config/kdeglobals");
    let content = std::fs::read_to_string(kdeglobals_path).ok()?;

    let mut pal = KdeIniPalette::default();
    pal.parse_into(&content, true);

    let scheme_name = if !pal.color_scheme.is_empty() {
        pal.color_scheme.clone()
    } else if !pal.look_and_feel.is_empty() {
        pal.look_and_feel
            .replace("org.kde.", "")
            .replace(".desktop", "")
    } else {
        String::new()
    };

    if (pal.window_bg.is_none() || pal.selection_bg.is_none()) && !scheme_name.is_empty() {
        let scheme_paths = [
            format!("{home}/.local/share/color-schemes/{scheme_name}.colors"),
            format!("/usr/share/color-schemes/{scheme_name}.colors"),
            format!("/usr/share/color-schemes/{scheme_name}Dark.colors"),
            "/usr/share/color-schemes/BreezeDark.colors".to_string(),
            "/usr/share/color-schemes/BreezeLight.colors".to_string(),
        ];
        for path in &scheme_paths {
            if let Ok(scheme_content) = std::fs::read_to_string(path) {
                pal.parse_into(&scheme_content, false);
                break;
            }
        }
    }

    let mut is_dark = scheme_name.to_lowercase().contains("dark")
        || pal.look_and_feel.to_lowercase().contains("dark");
    if let Some(ref w_bg) = pal.window_bg {
        if let Some(lum) = rgb_luminance(w_bg) {
            is_dark = lum < 128.0;
        }
    }

    let hex_or = |raw: &Option<String>, dark: &str, light: &str| {
        raw.as_deref()
            .and_then(parse_rgb_to_hex)
            .unwrap_or_else(|| pick(is_dark, dark, light))
    };

    let display_name = if !pal.color_scheme.is_empty() {
        format!("KDE Plasma ({})", pal.color_scheme)
    } else if is_dark {
        "KDE Plasma (Breeze Dark)".to_string()
    } else {
        "KDE Plasma (Breeze)".to_string()
    };

    Some(SystemThemeInfo {
        desktop_env: "KDE Plasma".to_string(),
        theme_name: display_name,
        is_dark,
        accent_color: pal
            .accent_color
            .unwrap_or_else(|| hex_or(&pal.selection_bg, "#3daee9", "#1d99f3")),
        window_bg: hex_or(&pal.window_bg, "#232629", "#eff0f1"),
        view_bg: hex_or(&pal.view_bg, "#1b1e20", "#fcfcfc"),
        card_bg: hex_or(&pal.button_bg, "#31363b", "#ffffff"),
        text_primary: hex_or(&pal.window_fg, "#eff0f1", "#232629"),
        text_secondary: pick(is_dark, "#bdc3c7", "#6e747b"),
        border_color: pick(is_dark, "rgba(255, 255, 255, 0.12)", "rgba(0, 0, 0, 0.12)"),
        is_fallback: false,
    })
}

fn detect_gtk_theme(home: &str) -> Option<SystemThemeInfo> {
    let mut theme_name = String::new();
    let mut prefer_dark = false;

    for ini_path in &[
        format!("{home}/.config/gtk-3.0/settings.ini"),
        format!("{home}/.config/gtk-4.0/settings.ini"),
    ] {
        if let Ok(content) = std::fs::read_to_string(ini_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some((k, v)) = trimmed.split_once('=') {
                    match k.trim() {
                        "gtk-theme-name" if theme_name.is_empty() => {
                            theme_name = v.trim().to_string()
                        }
                        "gtk-application-prefer-dark-theme" => {
                            prefer_dark = v.trim() == "1" || v.trim().eq_ignore_ascii_case("true")
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if theme_name.is_empty() {
        if let Ok(out) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .to_string();
            if !s.is_empty() && !s.contains("No such") {
                theme_name = s;
            }
        }
    }

    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        if s.contains("prefer-dark") {
            prefer_dark = true;
        }
    }

    if theme_name.is_empty() && !prefer_dark {
        return None;
    }

    let is_dark = prefer_dark || theme_name.to_lowercase().contains("dark");

    let mut bg_color = None;
    let mut sel_color = None;
    let mut fg_color = None;

    if !theme_name.is_empty() {
        for base in &[
            format!("{home}/.themes/{theme_name}/gtk-3.0/gtk.css"),
            format!("{home}/.local/share/themes/{theme_name}/gtk-3.0/gtk.css"),
            format!("/usr/share/themes/{theme_name}/gtk-3.0/gtk.css"),
        ] {
            if let Ok(css) = std::fs::read_to_string(base) {
                for line in css.lines() {
                    if let Some(rest) = line.trim().strip_prefix("@define-color ") {
                        let mut parts = rest.split_whitespace();
                        if let (Some(name), Some(val)) = (parts.next(), parts.next()) {
                            let color = val.trim_end_matches(';').to_string();
                            match name {
                                "theme_bg_color" => bg_color = Some(color),
                                "theme_selected_bg_color" => sel_color = Some(color),
                                "theme_fg_color" => fg_color = Some(color),
                                _ => {}
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    Some(SystemThemeInfo {
        desktop_env: "GNOME / GTK".to_string(),
        theme_name: if theme_name.is_empty() {
            "Adwaita".to_string()
        } else {
            theme_name
        },
        is_dark,
        accent_color: sel_color.unwrap_or_else(|| pick(is_dark, "#3584e4", "#1c71d8")),
        window_bg: bg_color.unwrap_or_else(|| pick(is_dark, "#242424", "#fafafa")),
        view_bg: pick(is_dark, "#1e1e1e", "#f0f0f0"),
        card_bg: pick(is_dark, "#303030", "#ffffff"),
        text_primary: fg_color.unwrap_or_else(|| pick(is_dark, "#ffffff", "#1e1e1e")),
        text_secondary: pick(is_dark, "#a0a0a0", "#5e5e5e"),
        border_color: pick(is_dark, "rgba(255, 255, 255, 0.08)", "rgba(0, 0, 0, 0.08)"),
        is_fallback: false,
    })
}

/// Detects the current Linux desktop theme palette (KDE Plasma Breeze, GNOME/GTK, or XDG Portal fallback).
pub fn detect_system_theme_info() -> SystemThemeInfo {
    let desktop_raw = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase();
    let home = std::env::var("HOME").unwrap_or_default();

    let is_kde = desktop_raw.contains("KDE")
        || desktop_raw.contains("PLASMA")
        || std::env::var("KDE_FULL_SESSION").is_ok();

    if is_kde {
        if let Some(kde_info) = detect_kde_theme(&home) {
            return kde_info;
        }
    }

    if let Some(gtk_info) = detect_gtk_theme(&home) {
        return gtk_info;
    }

    let mut is_dark = true;
    if let Ok(output) = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("uint32 1") {
            is_dark = true;
        } else if stdout.contains("uint32 2") || stdout.contains("uint32 0") {
            is_dark = false;
        }
    }

    SystemThemeInfo {
        desktop_env: if desktop_raw.is_empty() {
            "Standard Desktop".to_string()
        } else {
            desktop_raw
        },
        theme_name: pick(
            is_dark,
            "Default Dark (Fallback)",
            "Default Light (Fallback)",
        ),
        is_dark,
        accent_color: pick(is_dark, "#3daee9", "#1d99f3"),
        window_bg: pick(is_dark, "#232629", "#eff0f1"),
        view_bg: pick(is_dark, "#1b1e20", "#fcfcfc"),
        card_bg: pick(is_dark, "#31363b", "#ffffff"),
        text_primary: pick(is_dark, "#eff0f1", "#232629"),
        text_secondary: pick(is_dark, "#bdc3c7", "#6e747b"),
        border_color: pick(is_dark, "rgba(255, 255, 255, 0.12)", "rgba(0, 0, 0, 0.12)"),
        is_fallback: true,
    }
}
