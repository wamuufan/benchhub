//! Settings Tab Component
//!
//! Provides configuration for language, telemetry polling interval, terminal console buffer,
//! execution timeouts, thermal guardrails, directory paths, and Linux desktop theme synchronization.

use super::icons::{Icon, IconKind};
use super::{apply_system_locale_async, LocalizedSelect, SelectOption};
use crate::i18n::{t, Language};
use crate::tauri_bridge;
use crate::theme::{to_valid_hex_7, ThemeState};
use benchhub_types::{
    AppSettings, DEFAULT_TERMINAL_BUFFER_LINES, MAX_TERMINAL_BUFFER_LINES,
    MIN_TERMINAL_BUFFER_LINES,
};
use leptos::prelude::*;

const LANGUAGE_OPTIONS: &[SelectOption] = &[
    SelectOption::key("system", "language_system"),
    SelectOption::literal("en", "English"),
    SelectOption::literal("tr", "Türkçe"),
];

const TELEMETRY_INTERVAL_OPTIONS: &[SelectOption] = &[
    SelectOption::key("250", "interval_250"),
    SelectOption::key("500", "interval_500"),
    SelectOption::key("1000", "interval_1000"),
    SelectOption::key("2000", "interval_2000"),
];

const MAX_DURATION_OPTIONS: &[SelectOption] = &[
    SelectOption::key("0", "duration_0"),
    SelectOption::key("60", "duration_60"),
    SelectOption::key("120", "duration_120"),
    SelectOption::key("180", "duration_180"),
    SelectOption::key("300", "duration_300"),
    SelectOption::key("600", "duration_600"),
];

const THERMAL_MODE_OPTIONS: &[SelectOption] = &[
    SelectOption::key("off", "thermal_mode_off"),
    SelectOption::key("cooldown", "thermal_mode_cooldown"),
    SelectOption::key("warmup", "thermal_mode_warmup"),
    SelectOption::key("smart", "thermal_mode_smart"),
];

const COOLDOWN_TEMP_OPTIONS: &[SelectOption] = &[
    SelectOption::key("40", "temp_40"),
    SelectOption::key("45", "temp_45"),
    SelectOption::key("50", "temp_50_std"),
    SelectOption::key("55", "temp_55"),
    SelectOption::key("60", "temp_60"),
];

const WARMUP_TEMP_OPTIONS: &[SelectOption] = &[
    SelectOption::key("55", "temp_55"),
    SelectOption::key("60", "temp_60"),
    SelectOption::key("65", "temp_65_std"),
    SelectOption::key("70", "temp_70"),
    SelectOption::key("75", "temp_75"),
];

const THERMAL_TIMEOUT_OPTIONS: &[SelectOption] = &[
    SelectOption::key("15", "secs_15"),
    SelectOption::key("30", "secs_30_std"),
    SelectOption::key("45", "secs_45"),
    SelectOption::key("60", "secs_60"),
    SelectOption::key("120", "secs_120"),
];

const THEME_MODE_OPTIONS: &[SelectOption] = &[
    SelectOption::key("system", "theme_system"),
    SelectOption::key("dark", "theme_dark"),
    SelectOption::key("light", "theme_light"),
    SelectOption::key("custom", "theme_custom"),
];

const UI_SCALE_OPTIONS: &[SelectOption] = &[
    SelectOption::key("90", "scale_90"),
    SelectOption::key("100", "scale_100"),
    SelectOption::key("110", "scale_110"),
    SelectOption::key("125", "scale_125"),
    SelectOption::key("150", "scale_150"),
];

#[component]
fn SettingsSection(
    lang: RwSignal<Language>,
    title_key: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="settings-section">
            <h3 style="margin: 0; color: var(--text-primary);">{move || t(lang.get(), title_key)}</h3>
            {children()}
        </div>
    }
}

#[component]
fn SettingsRow(
    lang: RwSignal<Language>,
    title_key: &'static str,
    #[prop(optional)] desc_key: Option<&'static str>,
    #[prop(optional)] desc_text: Option<Signal<String>>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="settings-row">
            <div class="settings-info">
                <h4>{move || t(lang.get(), title_key)}</h4>
                <p>
                    {move || {
                        if let Some(key) = desc_key {
                            t(lang.get(), key).to_string()
                        } else if let Some(sig) = desc_text {
                            sig.get()
                        } else {
                            String::new()
                        }
                    }}
                </p>
            </div>
            {children()}
        </div>
    }
}

#[component]
pub fn SettingsTab(
    lang: RwSignal<Language>,
    theme_state: ThemeState,
    status_toast: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    let settings = RwSignal::new(AppSettings::default());
    let buffer_lines_input = RwSignal::new(DEFAULT_TERMINAL_BUFFER_LINES.to_string());
    let saved_notification = RwSignal::new(false);

    // Load current settings from the daemon
    leptos::task::spawn_local(async move {
        if let Ok(loaded) = tauri_bridge::get_settings().await {
            if loaded.language == "system"
                || loaded.language == "auto"
                || loaded.language.is_empty()
            {
                apply_system_locale_async(lang);
            } else {
                lang.set(Language::from_config(&loaded.language));
            }
            theme_state.apply_settings(&loaded.theme);
            buffer_lines_input.set(loaded.terminal_buffer_lines.to_string());
            settings.set(loaded);
        }
    });

    let on_save = move |_| {
        let mut s = settings.get_untracked();
        s.theme = theme_state.to_theme_settings();
        s.sanitize();
        buffer_lines_input.set(s.terminal_buffer_lines.to_string());

        leptos::task::spawn_local(async move {
            match tauri_bridge::save_settings(&s).await {
                Ok(_) => {
                    settings.set(s);
                    saved_notification.set(true);
                    status_toast.set(Some((
                        t(lang.get_untracked(), "settings_saved_msg").to_string(),
                        false,
                    )));
                    crate::theme::sleep_ms(2500).await;
                    saved_notification.set(false);
                }
                Err(err) => {
                    status_toast.set(Some((err, true)));
                }
            }
        });
    };

    let color_pickers = [
        (
            "accent_color_label",
            theme_state.custom_accent_color,
            "#3584e4",
        ),
        ("window_bg_label", theme_state.custom_window_bg, "#242424"),
        ("card_bg_label", theme_state.custom_card_bg, "#303030"),
        ("text_color_label", theme_state.custom_text_color, "#ffffff"),
    ];

    view! {
        <div class="app-content">
            <div class="view-header">
                <div class="view-title">
                    <h1>{move || t(lang.get(), "settings_title")}</h1>
                    <p>{move || t(lang.get(), "settings_subtitle")}</p>
                </div>

                <div style="display: flex; align-items: center; gap: 0.75rem;">
                    {move || {
                        if saved_notification.get() {
                            view! {
                                <span class="badge badge-success">
                                    {t(lang.get(), "settings_saved_msg")}
                                </span>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }
                    }}
                    <button class="btn btn-primary" on:click=on_save>
                        <Icon kind=IconKind::Save size=15 />
                        <span>{move || t(lang.get(), "save_settings_btn")}</span>
                    </button>
                </div>
            </div>

            <SettingsSection lang=lang title_key="section_general">
                <SettingsRow lang=lang title_key="language_label" desc_key="language_desc">
                    <LocalizedSelect
                        lang=lang
                        options=LANGUAGE_OPTIONS
                        selected_value=Signal::derive(move || settings.get().language)
                        on_change=Callback::new(move |val: String| {
                            if val == "system" {
                                settings.update(|s| s.language = "system".to_string());
                                apply_system_locale_async(lang);
                            } else {
                                lang.set(Language::from_config(&val));
                                settings.update(|s| s.language = val);
                            }
                        })
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="telemetry_interval_label" desc_key="telemetry_interval_desc">
                    <LocalizedSelect
                        lang=lang
                        options=TELEMETRY_INTERVAL_OPTIONS
                        selected_value=Signal::derive(move || settings.get().telemetry_interval_ms.to_string())
                        on_change=Callback::new(move |val: String| {
                            if let Ok(parsed) = val.parse::<u64>() {
                                settings.update(|s| s.telemetry_interval_ms = parsed);
                            }
                        })
                    />
                </SettingsRow>

                <SettingsRow
                    lang=lang
                    title_key="disable_telemetry_during_runs_label"
                    desc_key="disable_telemetry_during_runs_desc"
                >
                    <input
                        type="checkbox"
                        prop:checked=move || settings.get().disable_telemetry_during_runs
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            settings.update(|s| s.disable_telemetry_during_runs = checked);
                        }
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="console_buffer_label" desc_key="console_buffer_desc">
                    <input
                        type="number"
                        min="50"
                        max="1000"
                        step="50"
                        prop:value=move || buffer_lines_input.get()
                        on:input=move |ev| {
                            let raw = event_target_value(&ev);
                            buffer_lines_input.set(raw.clone());
                            if let Ok(val) = raw.trim().parse::<usize>() {
                                let clamped =
                                    val.clamp(MIN_TERMINAL_BUFFER_LINES, MAX_TERMINAL_BUFFER_LINES);
                                settings.update(|s| s.terminal_buffer_lines = clamped);
                            }
                        }
                        on:blur=move |_| {
                            let raw = buffer_lines_input.get_untracked();
                            let normalized = raw
                                .trim()
                                .parse::<usize>()
                                .map(|val| {
                                    val.clamp(MIN_TERMINAL_BUFFER_LINES, MAX_TERMINAL_BUFFER_LINES)
                                })
                                .unwrap_or(DEFAULT_TERMINAL_BUFFER_LINES);
                            settings.update(|s| s.terminal_buffer_lines = normalized);
                            buffer_lines_input.set(normalized.to_string());
                        }
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="max_duration_label" desc_key="max_duration_desc">
                    <LocalizedSelect
                        lang=lang
                        options=MAX_DURATION_OPTIONS
                        selected_value=Signal::derive(move || settings.get().max_test_duration_secs.to_string())
                        on_change=Callback::new(move |val: String| {
                            if let Ok(parsed) = val.parse::<u64>() {
                                settings.update(|s| s.max_test_duration_secs = parsed);
                            }
                        })
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="save_stopped_runs_label" desc_key="save_stopped_runs_desc">
                    <input
                        type="checkbox"
                        prop:checked=move || settings.get().save_stopped_runs
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            settings.update(|s| s.save_stopped_runs = checked);
                        }
                    />
                </SettingsRow>
            </SettingsSection>

            <SettingsSection lang=lang title_key="section_thermal">
                <SettingsRow lang=lang title_key="thermal_mode_label" desc_key="thermal_mode_desc">
                    <LocalizedSelect
                        lang=lang
                        options=THERMAL_MODE_OPTIONS
                        selected_value=Signal::derive(move || settings.get().thermal_mode)
                        on_change=Callback::new(move |val: String| {
                            settings.update(|s| s.thermal_mode = val);
                        })
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="cooldown_temp_label" desc_key="cooldown_temp_desc">
                    <LocalizedSelect
                        lang=lang
                        options=COOLDOWN_TEMP_OPTIONS
                        selected_value=Signal::derive(move || settings.get().thermal_cooldown_temp.to_string())
                        on_change=Callback::new(move |val: String| {
                            if let Ok(parsed) = val.parse::<u32>() {
                                settings.update(|s| s.thermal_cooldown_temp = parsed);
                            }
                        })
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="warmup_temp_label" desc_key="warmup_temp_desc">
                    <LocalizedSelect
                        lang=lang
                        options=WARMUP_TEMP_OPTIONS
                        selected_value=Signal::derive(move || settings.get().thermal_warmup_temp.to_string())
                        on_change=Callback::new(move |val: String| {
                            if let Ok(parsed) = val.parse::<u32>() {
                                settings.update(|s| s.thermal_warmup_temp = parsed);
                            }
                        })
                    />
                </SettingsRow>

                <SettingsRow lang=lang title_key="thermal_timeout_label" desc_key="thermal_timeout_desc">
                    <LocalizedSelect
                        lang=lang
                        options=THERMAL_TIMEOUT_OPTIONS
                        selected_value=Signal::derive(move || settings.get().thermal_timeout_secs.to_string())
                        on_change=Callback::new(move |val: String| {
                            if let Ok(parsed) = val.parse::<u32>() {
                                settings.update(|s| s.thermal_timeout_secs = parsed);
                            }
                        })
                    />
                </SettingsRow>
            </SettingsSection>

            <SettingsSection lang=lang title_key="section_directories">
                <SettingsRow lang=lang title_key="runners_dir_label" desc_key="runners_dir_desc">
                    <input
                        type="text"
                        style="min-width: 300px;"
                        placeholder="~/.local/share/benchhub/runners"
                        prop:value=move || settings.get().custom_runners_dir
                        on:input=move |ev| {
                            let val = event_target_value(&ev);
                            settings.update(|s| s.custom_runners_dir = val);
                        }
                    />
                </SettingsRow>
            </SettingsSection>

            <SettingsSection lang=lang title_key="section_appearance">
                <SettingsRow lang=lang title_key="theme_mode_label" desc_key="theme_mode_desc">
                    <LocalizedSelect
                        lang=lang
                        options=THEME_MODE_OPTIONS
                        selected_value=Signal::derive(move || {
                            if theme_state.custom_theme_enabled.get() {
                                "custom".to_string()
                            } else {
                                theme_state.theme_mode.get()
                            }
                        })
                        on_change=Callback::new(move |val: String| {
                            if val == "custom" {
                                theme_state.custom_theme_enabled.set(true);
                            } else {
                                theme_state.custom_theme_enabled.set(false);
                                theme_state.theme_mode.set(val);
                            }
                        })
                    />
                </SettingsRow>

                {move || {
                    if theme_state.custom_theme_enabled.get() {
                        view! {
                            <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 1rem; padding: 1rem; background: var(--view-bg); border-radius: var(--radius-widget);">
                                {color_pickers
                                    .into_iter()
                                    .map(|(label_key, signal, fallback)| {
                                        view! {
                                            <div style="display: flex; align-items: center; justify-content: space-between;">
                                                <label style="font-size: 0.85rem; font-weight: 500;">
                                                    {t(lang.get(), label_key)}
                                                </label>
                                                <input
                                                    type="color"
                                                    prop:value=move || signal.get()
                                                    on:input=move |ev| {
                                                        let val = event_target_value(&ev);
                                                        signal.set(to_valid_hex_7(&val, fallback));
                                                    }
                                                />
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}

                <SettingsRow lang=lang title_key="ui_scale_label" desc_key="ui_scale_desc">
                    <LocalizedSelect
                        lang=lang
                        options=UI_SCALE_OPTIONS
                        selected_value=Signal::derive(move || theme_state.ui_scale.get())
                        on_change=Callback::new(move |val: String| theme_state.ui_scale.set(val))
                    />
                </SettingsRow>

                <SettingsRow
                    lang=lang
                    title_key="detected_desktop"
                    desc_text=Signal::derive(move || theme_state.sys_theme_info.get().desktop_env)
                >
                    <div>
                        <span class="badge badge-info">
                            {move || theme_state.sys_theme_info.get().theme_name}
                        </span>
                    </div>
                </SettingsRow>
            </SettingsSection>
        </div>
    }
}
