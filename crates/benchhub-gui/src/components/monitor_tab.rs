//! Monitor Tab Component
//!
//! Renders real-time telemetry gauges (CPU/GPU temperatures, clocks, power draw,
//! RAM/VRAM usage, thermal throttling alerts), active task banner with cancellation,
//! hardware capability/power-profile summary, and live streaming terminal logs.

use super::icons::{Icon, IconKind};
use super::{
    copy_text_with_feedback, format_elapsed_mmss, spawn_toast_listener, LiveTerminalBox,
    TerminalFollowButton, TerminalLinesBadge,
};
use crate::i18n::{bench_name, t, Language};
use crate::tauri_bridge;
use benchhub_types::{ActiveTaskInfo, TelemetryData, TerminalRingBuffer};
use leptos::prelude::*;

fn render_thermal_card(
    l: Language,
    label_key: &'static str,
    temp_val: Option<f32>,
    unit_c: &str,
    throttle_active: bool,
) -> AnyView {
    let value_str = temp_val
        .map(|v| format!("{:.1} {}", v, unit_c))
        .unwrap_or_else(|| format!("-- {}", unit_c));
    let badge_class = if throttle_active {
        "badge badge-error"
    } else {
        "badge badge-success"
    };
    let badge_text = t(
        l,
        if throttle_active {
            "throttle_active"
        } else {
            "throttle_none"
        },
    );

    view! {
        <div class="telemetry-card" class:alert=throttle_active>
            <span class="telemetry-label">{t(l, label_key)}</span>
            <div class="telemetry-val">{value_str}</div>
            <div class="telemetry-sub">
                <span class=badge_class>{badge_text}</span>
            </div>
        </div>
    }
    .into_any()
}

fn render_metric_card(label: &'static str, value_str: String, sub_text: String) -> AnyView {
    view! {
        <div class="telemetry-card">
            <span class="telemetry-label">{label}</span>
            <div class="telemetry-val">{value_str}</div>
            <div class="telemetry-sub">{sub_text}</div>
        </div>
    }
    .into_any()
}

#[component]
pub fn MonitorTab(
    lang: RwSignal<Language>,
    active_run: RwSignal<Option<ActiveTaskInfo>>,
    live_telemetry: RwSignal<Option<TelemetryData>>,
    terminal_buf: RwSignal<TerminalRingBuffer>,
    status_toast: RwSignal<Option<(String, bool)>>,
    now_epoch_ms: RwSignal<i64>,
) -> impl IntoView {
    let copy_feedback = RwSignal::new(false);
    let power_profile = RwSignal::new(String::new());
    let hw_capabilities = RwSignal::new(String::new());
    let terminal_ref = NodeRef::<leptos::html::Div>::new();
    let auto_follow = RwSignal::new(true);

    leptos::task::spawn_local(async move {
        if let Ok(profile) = tauri_bridge::get_power_profile().await {
            power_profile.set(profile);
        }
        if let Ok(caps) = tauri_bridge::get_hardware_capabilities().await {
            hw_capabilities.set(caps);
        }
    });

    let on_cancel_click = move |_| {
        spawn_toast_listener(status_toast, tauri_bridge::cancel_active_run());
    };

    let on_copy_logs = move |_| {
        let text = terminal_buf.with_untracked(|b| b.to_string());
        copy_text_with_feedback(&text, copy_feedback);
    };

    let on_clear_logs = move |_| {
        terminal_buf.update(|b| b.clear());
        auto_follow.set(true);
        spawn_toast_listener(status_toast, tauri_bridge::clear_terminal_logs());
    };

    view! {
        <div class="app-content">
            <div class="view-header">
                <div class="view-title">
                    <h1>{move || t(lang.get(), "monitor_title")}</h1>
                    <p>{move || t(lang.get(), "monitor_subtitle")}</p>
                    {move || {
                        let l = lang.get();
                        let pp = power_profile.get();
                        let hw = hw_capabilities.get();
                        if pp.is_empty() && hw.is_empty() {
                            view! { <span></span> }.into_any()
                        } else {
                            view! {
                                <div style="display: flex; gap: 0.5rem; flex-wrap: wrap; margin-top: 0.5rem;">
                                    {(!pp.is_empty()).then(|| view! {
                                        <span class="badge badge-info">
                                            {format!("{} {}", t(l, "power_profile_label"), pp)}
                                        </span>
                                    })}
                                    {(!hw.is_empty()).then(|| view! {
                                        <span class="badge badge-info">
                                            {format!("{} {}", t(l, "sensors_label"), hw)}
                                        </span>
                                    })}
                                </div>
                            }.into_any()
                        }
                    }}
                </div>

                {move || {
                    let l = lang.get();
                    if let Some(ref current) = active_run.get() {
                        let status_text = if current.is_install {
                            t(l, "status_installing")
                        } else {
                            t(l, "status_running")
                        };
                        let name = bench_name(l, &current.benchmark_id, &current.benchmark_id);
                        let started = current.started_at_epoch_ms;

                        view! {
                            <div style="display: flex; align-items: center; gap: 0.75rem; background: color-mix(in srgb, var(--color-warning) 15%, var(--card-bg)); border: 1px solid var(--color-warning); border-radius: var(--radius-widget); padding: 0.5rem 1rem;">
                                <div class="status-dot running"></div>
                                <div>
                                    <strong style="color: var(--color-warning); font-size: 0.9rem;">
                                        {status_text}
                                    </strong>
                                    <span style="font-size: 0.8rem; color: var(--text-secondary); margin-left: 0.5rem;">
                                        {format!("{} ({})", name, current.version)}
                                    </span>
                                    <span style="font-size: 0.78rem; color: var(--text-secondary); margin-left: 0.5rem;">
                                        {move || format!("• {}", format_elapsed_mmss(now_epoch_ms.get(), started))}
                                    </span>
                                </div>
                                <button
                                    class="btn btn-danger btn-sm"
                                    on:click=on_cancel_click
                                >
                                    <Icon kind=IconKind::Stop size=14 />
                                    <span>{t(l, "cancel_active")}</span>
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>

            <div class="telemetry-grid">
                {move || {
                    let l = lang.get();
                    let data = live_telemetry.get();
                    let unit_c = t(l, "unit_celsius");
                    let unit_mhz = t(l, "unit_mhz");
                    let unit_w = t(l, "unit_watt");
                    let unit_mb = t(l, "unit_mb");

                    let fmt_opt = |v: Option<f32>, prec: usize, unit: &str| {
                        v.map(|n| format!("{:.*} {}", prec, n, unit))
                            .unwrap_or_else(|| format!("-- {}", unit))
                    };
                    let fmt_pct = |v: Option<f32>| {
                        v.map(|n| format!("{:.0}%", n))
                            .unwrap_or_else(|| "--%".into())
                    };

                    let cpu_throttle_active = data.as_ref().is_some_and(|d| d.cpu_throttle != "none");
                    let gpu_throttle_active = data.as_ref().is_some_and(|d| d.gpu_throttle != "none");
                    let d = data.as_ref();

                    view! {
                        {render_thermal_card(l, "telemetry_cpu_temp", d.map(|x| x.cpu_temp), unit_c, cpu_throttle_active)}
                        {render_thermal_card(l, "telemetry_gpu_temp", d.map(|x| x.gpu_temp), unit_c, gpu_throttle_active)}
                        {render_metric_card(
                            t(l, "telemetry_cpu_freq"),
                            fmt_opt(d.map(|x| x.cpu_freq), 0, unit_mhz),
                            format!("{} {}", t(l, "load_label"), fmt_pct(d.map(|x| x.cpu_usage))),
                        )}
                        {render_metric_card(
                            t(l, "telemetry_gpu_freq"),
                            fmt_opt(d.map(|x| x.gpu_freq_mhz), 0, unit_mhz),
                            format!("{} {}", t(l, "load_label"), fmt_pct(d.map(|x| x.gpu_usage))),
                        )}
                        {render_metric_card(
                            t(l, "telemetry_power"),
                            fmt_opt(d.map(|x| x.power_w), 1, unit_w),
                            format!("{} {}", t(l, "gpu_short_label"), fmt_opt(d.map(|x| x.gpu_power_w), 1, unit_w)),
                        )}
                        {render_metric_card(
                            t(l, "telemetry_ram"),
                            fmt_opt(d.map(|x| x.ram_usage_mb), 0, unit_mb),
                            format!("VRAM: {}", fmt_opt(d.map(|x| x.vram_usage_mb), 0, unit_mb)),
                        )}
                    }
                }}
            </div>

            <div class="card terminal-container">
                <div class="terminal-toolbar">
                    <div style="display: flex; align-items: center; gap: 0.75rem;">
                        <span style="font-weight: 700; font-size: 1rem; display: inline-flex; align-items: center; gap: 0.45rem;">
                            <Icon kind=IconKind::Terminal size=17 />
                            <span>{move || t(lang.get(), "terminal_title")}</span>
                        </span>
                        <TerminalLinesBadge lang=lang terminal_buf=terminal_buf />
                    </div>

                    <div style="display: flex; gap: 0.5rem; align-items: center;">
                        <TerminalFollowButton
                            lang=lang
                            terminal_ref=terminal_ref
                            auto_follow=auto_follow
                            icon_size=14
                        />
                        {move || {
                            if copy_feedback.get() {
                                view! {
                                    <span class="badge badge-success">
                                        {t(lang.get(), "copied")}
                                    </span>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                        <button class="btn btn-secondary btn-sm" on:click=on_copy_logs>
                            <Icon kind=IconKind::Copy size=14 />
                            <span>{move || t(lang.get(), "copy_logs")}</span>
                        </button>
                        <button class="btn btn-secondary btn-sm" on:click=on_clear_logs>
                            <Icon kind=IconKind::Trash size=14 />
                            <span>{move || t(lang.get(), "clear_logs")}</span>
                        </button>
                    </div>
                </div>

                <LiveTerminalBox
                    lang=lang
                    terminal_buf=terminal_buf
                    terminal_ref=terminal_ref
                    auto_follow=auto_follow
                    id="terminal-log-scroller"
                />
            </div>
        </div>
    }
}
