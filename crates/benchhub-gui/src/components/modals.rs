//! Modal Dialog Overlays
//!
//! Provides interactive modals for package installation confirmations (EULA),
//! custom benchmark argument configuration with security filters, side-by-side run comparisons,
//! group assignments, run details inspection, and generic action confirmations.

use super::icons::{Icon, IconKind};
use super::nav_header::AppTab;
use super::{
    copy_text_with_feedback, format_timestamp_local, run_status_badge_class,
    scroll_terminal_to_bottom, spawn_toast_listener, LiveTerminalBox, ModalShell,
    TerminalFollowButton, TerminalLinesBadge,
};
use crate::i18n::{
    bench_name, t, translate_category, translate_gpu_mode, translate_status,
    translate_throttle_status, Language,
};
use crate::tauri_bridge;
use benchhub_types::{is_safe_custom_arg, RunComparisonDiff, RunResult, TerminalRingBuffer};
use leptos::prelude::*;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
enum InstallPhase {
    Confirm,
    Installing,
    Finished {
        success: bool,
        cancelled: bool,
        error_msg: Option<String>,
    },
}

#[component]
pub fn EulaModal(
    target: RwSignal<Option<(String, String)>>,
    lang: RwSignal<Language>,
    active_tab: RwSignal<AppTab>,
    terminal_buf: RwSignal<TerminalRingBuffer>,
    status_toast: RwSignal<Option<(String, bool)>>,
    now_epoch_ms: RwSignal<i64>,
    last_stopped_task: RwSignal<(u64, String)>,
) -> impl IntoView {
    let eula_text = RwSignal::new(Option::<String>::None);
    let eula_loaded = RwSignal::new(false);
    let eula_accepted = RwSignal::new(false);
    let phase = RwSignal::new(InstallPhase::Confirm);
    let install_session_seq = RwSignal::new(0u64);
    let stop_seq_at_start = RwSignal::new(0u64);
    let stop_requested = RwSignal::new(false);
    let cancelling = RwSignal::new(false);
    let install_started_ms = RwSignal::new(0i64);
    let finished_elapsed_secs = RwSignal::new(0i64);
    let terminal_ref = NodeRef::<leptos::html::Div>::new();
    let auto_follow = RwSignal::new(true);

    // Reset state and fetch license text from daemon whenever a package dialog opens
    Effect::new(move |_| {
        eula_accepted.set(false);
        eula_loaded.set(false);
        phase.set(InstallPhase::Confirm);
        stop_requested.set(false);
        cancelling.set(false);
        auto_follow.set(true);
        install_session_seq.update(|s| *s = s.wrapping_add(1));

        if let Some((ref id, _)) = target.get() {
            eula_text.set(None);
            let id_fetch = id.clone();
            leptos::task::spawn_local(async move {
                let res = tauri_bridge::get_eula_text(&id_fetch).await;
                if target
                    .get_untracked()
                    .as_ref()
                    .is_some_and(|(cur, _)| cur == &id_fetch)
                {
                    match res {
                        Ok(text) => {
                            eula_text.set(Some(text));
                            eula_loaded.set(true);
                        }
                        Err(err) => {
                            let prefix = t(lang.get_untracked(), "modal_eula_fetch_error");
                            eula_text.set(Some(format!("{} {}", prefix, err)));
                            eula_loaded.set(false);
                        }
                    }
                }
            });
        }
    });

    // Reflect external or daemon-confirmed task stops even if Stop arrived around RPC completion
    Effect::new(move |_| {
        let (stop_seq, stopped_id) = last_stopped_task.get();
        if stop_seq == 0 {
            return;
        }
        if let Some((cur_id, _)) = target.get_untracked() {
            if cur_id == stopped_id
                && stop_seq > stop_seq_at_start.get_untracked()
                && matches!(
                    phase.get_untracked(),
                    InstallPhase::Installing | InstallPhase::Finished { success: true, .. }
                )
            {
                let elapsed = ((js_sys::Date::now() as i64) - install_started_ms.get_untracked())
                    .max(0)
                    / 1000;
                finished_elapsed_secs.set(elapsed);
                cancelling.set(false);
                phase.set(InstallPhase::Finished {
                    success: false,
                    cancelled: true,
                    error_msg: None,
                });
            }
        }
    });

    view! {
        {move || {
            target.get().map(|(id, version)| {
                let id_for_start = id.clone();
                let ver_for_start = version.clone();
                let display_id = id.clone();
                let display_ver = version.clone();

                let on_start_install = move |_| {
                    if !eula_accepted.get_untracked() || !eula_loaded.get_untracked() {
                        return;
                    }
                    let id_req = id_for_start.clone();
                    let ver_req = ver_for_start.clone();
                    let session = install_session_seq.get_untracked();
                    let now_ms = js_sys::Date::now() as i64;

                    stop_seq_at_start.set(last_stopped_task.get_untracked().0);
                    stop_requested.set(false);
                    cancelling.set(false);
                    install_started_ms.set(now_ms);
                    finished_elapsed_secs.set(0);
                    now_epoch_ms.set(now_ms);
                    auto_follow.set(true);
                    eula_accepted.set(false);
                    phase.set(InstallPhase::Installing);
                    scroll_terminal_to_bottom(terminal_ref);

                    leptos::task::spawn_local(async move {
                        let res = tauri_bridge::install_benchmark(&id_req, &ver_req, true).await;
                        let elapsed = ((js_sys::Date::now() as i64) - now_ms).max(0) / 1000;

                        let still_same_modal = install_session_seq.get_untracked() == session
                            && target.get_untracked().is_some();

                        if !still_same_modal {
                            if let Err(err) = res {
                                status_toast.set(Some((err, true)));
                            }
                            return;
                        }

                        finished_elapsed_secs.set(elapsed);
                        cancelling.set(false);
                        match res {
                            Ok(()) => {
                                let (latest_stop_seq, latest_stopped_id) =
                                    last_stopped_task.get_untracked();
                                let was_stopped = stop_requested.get_untracked()
                                    || (latest_stop_seq > stop_seq_at_start.get_untracked()
                                        && latest_stopped_id == id_req);
                                phase.set(InstallPhase::Finished {
                                    success: !was_stopped,
                                    cancelled: was_stopped,
                                    error_msg: None,
                                });
                            }
                            Err(err) => {
                                phase.set(InstallPhase::Finished {
                                    success: false,
                                    cancelled: false,
                                    error_msg: Some(err),
                                });
                            }
                        }
                    });
                };

                let on_stop_install = move |_| {
                    if cancelling.get_untracked() {
                        return;
                    }
                    stop_requested.set(true);
                    cancelling.set(true);
                    leptos::task::spawn_local(async move {
                        if let Err(err) = tauri_bridge::cancel_active_run().await {
                            cancelling.set(false);
                            stop_requested.set(false);
                            status_toast.set(Some((err, true)));
                        }
                    });
                };

                let on_watch_in_monitor = move |_| {
                    active_tab.set(AppTab::Monitor);
                    target.set(None);
                };

                let close_cb = Callback::new(move |_| target.set(None));
                let panel_cls = Signal::derive(move || {
                    if matches!(phase.get(), InstallPhase::Confirm) {
                        "modal-panel".to_string()
                    } else {
                        "modal-panel modal-panel-wide".to_string()
                    }
                });
                let header = view! { <h3>{move || t(lang.get(), "modal_eula_title")}</h3> }.into_any();
                let footer = view! {
                    {move || {
                        let l = lang.get();
                        match phase.get() {
                            InstallPhase::Confirm => view! {
                                <button class="btn btn-secondary" on:click=move |_| target.set(None)>
                                    {t(l, "modal_eula_cancel")}
                                </button>
                                <button
                                    class="btn btn-primary"
                                    disabled=move || !eula_accepted.get() || !eula_loaded.get()
                                    on:click=on_start_install.clone()
                                >
                                    {t(l, "modal_eula_confirm")}
                                </button>
                            }.into_any(),
                            InstallPhase::Installing => view! {
                                <button class="btn btn-secondary" on:click=on_watch_in_monitor>
                                    {t(l, "modal_eula_watch_monitor")}
                                </button>
                                <button
                                    class="btn btn-danger"
                                    disabled=move || cancelling.get()
                                    on:click=on_stop_install
                                >
                                    {t(l, "stop_task")}
                                </button>
                            }.into_any(),
                            InstallPhase::Finished { .. } => view! {
                                <button class="btn btn-secondary" on:click=on_watch_in_monitor>
                                    {t(l, "modal_eula_watch_monitor")}
                                </button>
                                <button class="btn btn-primary" on:click=move |_| target.set(None)>
                                    {t(l, "modal_eula_close")}
                                </button>
                            }.into_any(),
                        }
                    }}
                }.into_any();

                view! {
                    <ModalShell panel_class=panel_cls header=header footer=footer on_close=close_cb>
                        {move || {
                            let l = lang.get();
                            let display_name = bench_name(l, &display_id, &display_id).to_string();
                            view! {
                                <div style="margin-bottom: 0.85rem; padding: 0.75rem; background: var(--view-bg); border-radius: var(--radius-widget); display: flex; justify-content: space-between; align-items: center; gap: 0.75rem; flex-wrap: wrap;">
                                    <div>
                                        <div><strong>{t(l, "col_benchmark")}:</strong> " " {display_name}</div>
                                        <div><strong>{t(l, "col_version")}:</strong> " " {display_ver.clone()}</div>
                                    </div>
                                </div>
                            }
                        }}

                        <Show
                            when=move || matches!(phase.get(), InstallPhase::Confirm)
                            fallback=move || {
                                view! {
                                    <div class="install-status-bar">
                                        <div style="display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap;">
                                            {move || {
                                                let l = lang.get();
                                                match phase.get() {
                                                    InstallPhase::Installing => view! {
                                                        <span class="badge badge-warning">
                                                            {t(l, "modal_eula_status_installing")}
                                                        </span>
                                                    }.into_any(),
                                                    InstallPhase::Finished { success: true, .. } => view! {
                                                        <span class="badge badge-success">
                                                            <Icon kind=IconKind::Check size=13 />
                                                            <span>{t(l, "modal_eula_status_success")}</span>
                                                        </span>
                                                    }.into_any(),
                                                    InstallPhase::Finished { cancelled: true, .. } => view! {
                                                        <span class="badge badge-warning">
                                                            {t(l, "modal_eula_status_cancelled")}
                                                        </span>
                                                    }.into_any(),
                                                    InstallPhase::Finished { .. } => view! {
                                                        <span class="badge badge-error">
                                                            <Icon kind=IconKind::AlertTriangle size=13 />
                                                            <span>{t(l, "modal_eula_status_failed")}</span>
                                                        </span>
                                                    }.into_any(),
                                                    InstallPhase::Confirm => view! { <span></span> }.into_any(),
                                                }
                                            }}
                                            <span class="badge badge-info">
                                                {move || {
                                                    let l = lang.get();
                                                    let elapsed = match phase.get() {
                                                        InstallPhase::Installing => {
                                                            (now_epoch_ms.get() - install_started_ms.get()).max(0) / 1000
                                                        }
                                                        _ => finished_elapsed_secs.get(),
                                                    };
                                                    format!("{} {}", elapsed, t(l, "unit_secs"))
                                                }}
                                            </span>
                                        </div>

                                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                                            <TerminalLinesBadge lang=lang terminal_buf=terminal_buf />
                                            <TerminalFollowButton
                                                lang=lang
                                                terminal_ref=terminal_ref
                                                auto_follow=auto_follow
                                                icon_size=13
                                            />
                                        </div>
                                    </div>

                                    {move || {
                                        if let InstallPhase::Finished { error_msg: Some(err), .. } = phase.get() {
                                            view! { <div class="install-error-notice">{err}</div> }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }
                                    }}

                                    <div style="margin-top: 0.75rem;">
                                        <div style="font-size: 0.85rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 0.4rem;">
                                            {move || t(lang.get(), "modal_eula_install_logs")}
                                        </div>
                                        <LiveTerminalBox
                                            lang=lang
                                            terminal_buf=terminal_buf
                                            terminal_ref=terminal_ref
                                            auto_follow=auto_follow
                                            class="terminal-box modal-terminal-box"
                                        />
                                    </div>
                                }
                            }
                        >
                            {move || {
                                let l = lang.get();
                                view! {
                                    <p>{t(l, "modal_eula_msg")}</p>
                                    <div style="margin-top: 0.75rem;">
                                        <div style="font-size: 0.85rem; font-weight: 600; color: var(--text-secondary);">
                                            {t(l, "modal_eula_terms")}
                                        </div>
                                        <div class="eula-box">
                                            {move || {
                                                eula_text
                                                    .get()
                                                    .unwrap_or_else(|| t(lang.get(), "modal_eula_loading").to_string())
                                            }}
                                        </div>
                                    </div>
                                    <div class="eula-disclaimer">{t(l, "modal_eula_disclaimer")}</div>
                                    <label class="eula-consent-row">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || eula_accepted.get()
                                            disabled=move || !eula_loaded.get()
                                            on:change=move |ev| {
                                                eula_accepted.set(event_target_checked(&ev));
                                            }
                                        />
                                        <span>{t(l, "modal_eula_accept_check")}</span>
                                    </label>
                                }
                            }}
                        </Show>
                    </ModalShell>
                }
            })
        }}
    }
}

fn validate_custom_args_input(lang: Language, raw: &str) -> Option<String> {
    raw.split_whitespace()
        .find(|token| !is_safe_custom_arg(token))
        .map(|bad| format!("{} '{}'", t(lang, "modal_args_forbidden"), bad))
}

#[component]
pub fn CustomArgsModal<F>(
    target: RwSignal<Option<(String, String, String)>>,
    lang: RwSignal<Language>,
    on_save: F,
) -> impl IntoView
where
    F: Fn(String, String, String) + Send + Sync + Copy + 'static,
{
    let input_args = RwSignal::new(String::new());
    let error_msg = RwSignal::new(Option::<String>::None);

    Effect::new(move |_| {
        if let Some((_, _, ref args)) = target.get() {
            input_args.set(args.clone());
            error_msg.set(None);
        }
    });

    view! {
        {move || {
            target.get().map(|(id, version, _)| {
                let l = lang.get();
                let id_clone = id.clone();
                let ver_clone = version.clone();

                let on_input = move |ev| {
                    let val = event_target_value(&ev);
                    error_msg.set(validate_custom_args_input(lang.get_untracked(), &val));
                    input_args.set(val);
                };

                let on_save_click = move |_| {
                    let val = input_args.get();
                    if let Some(err) = validate_custom_args_input(lang.get_untracked(), &val) {
                        error_msg.set(Some(err));
                    } else {
                        target.set(None);
                        on_save(id_clone.clone(), ver_clone.clone(), val);
                    }
                };

                let close_cb = Callback::new(move |_| target.set(None));
                let header = view! { <h3>{t(l, "modal_args_title")}</h3> }.into_any();
                let footer = view! {
                    <button class="btn btn-secondary" on:click=move |_| target.set(None)>
                        {t(l, "modal_eula_cancel")}
                    </button>
                    <button class="btn btn-primary" on:click=on_save_click>
                        {t(l, "modal_args_apply")}
                    </button>
                }.into_any();

                view! {
                    <ModalShell header=header footer=footer on_close=close_cb>
                        <p>{t(l, "modal_args_desc")}</p>
                        <div style="margin: 0.5rem 0;">
                            <span class="badge badge-info">
                                {format!("{} ({})", bench_name(l, &id, &id), version)}
                            </span>
                        </div>
                        <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 1rem;">
                            <input
                                type="text"
                                style="width: 100%; font-family: monospace;"
                                placeholder=t(l, "modal_args_placeholder")
                                prop:value=move || input_args.get()
                                on:input=on_input
                            />
                            {move || error_msg.get().map(|err| view! {
                                <div style="color: var(--color-error); font-size: 0.82rem; font-weight: 500; display: inline-flex; align-items: center; gap: 0.35rem;">
                                    <Icon kind=IconKind::AlertTriangle size=14 />
                                    <span>{err}</span>
                                </div>
                            })}
                        </div>
                    </ModalShell>
                }
            })
        }}
    }
}

fn render_compare_row(
    label: String,
    val_a: String,
    val_b: String,
    diff: String,
    diff_style: Option<String>,
) -> AnyView {
    view! {
        <tr>
            <td>{label}</td>
            <td>{val_a}</td>
            <td>{val_b}</td>
            <td style=diff_style>{diff}</td>
        </tr>
    }
    .into_any()
}

#[component]
pub fn CompareModal(
    diff: RwSignal<Option<RunComparisonDiff>>,
    lang: RwSignal<Language>,
) -> impl IntoView {
    view! {
        {move || {
            diff.get().map(|d| {
                let l = lang.get();
                let score_diff_str = match (d.score_diff, d.score_diff_pct) {
                    (Some(diff), Some(pct)) => {
                        let sign = if diff >= 0.0 { "+" } else { "" };
                        format!("{}{:.2} ({}{:.1}%)", sign, diff, sign, pct)
                    }
                    _ => "-".to_string(),
                };
                let score_diff_color = match d.score_diff {
                    Some(v) if v > 0.0 => "var(--color-success)",
                    Some(v) if v < 0.0 => "var(--color-error)",
                    _ => "var(--text-primary)",
                };

                let unit_s = t(l, "unit_secs");
                let unit_c = t(l, "unit_celsius");
                let unit_w = t(l, "unit_watt");
                let unit_gb = t(l, "unit_gb");
                let peak_suffix = t(l, "modal_compare_peak");
                let avg_suffix = t(l, "modal_compare_avg");

                let run_a_name = bench_name(l, &d.run_a.benchmark_id, &d.run_a.benchmark_id).to_string();
                let run_b_name = bench_name(l, &d.run_b.benchmark_id, &d.run_b.benchmark_id).to_string();
                let score_a = d.run_a.score.map(|s| format!("{:.2}", s)).unwrap_or_else(|| "-".into());
                let score_b = d.run_b.score.map(|s| format!("{:.2}", s)).unwrap_or_else(|| "-".into());

                let rows = vec![
                    (
                        t(l, "col_duration").to_string(),
                        format!("{:.1} {}", d.run_a.duration_secs, unit_s),
                        format!("{:.1} {}", d.run_b.duration_secs, unit_s),
                        format!("{:+0.1} {}", d.duration_diff_secs, unit_s),
                    ),
                    (
                        format!("{} {}", t(l, "telemetry_cpu_temp"), peak_suffix),
                        format!("{:.1} {}", d.run_a.peak_cpu_temp, unit_c),
                        format!("{:.1} {}", d.run_b.peak_cpu_temp, unit_c),
                        format!("{:+0.1} {}", d.peak_cpu_temp_diff, unit_c),
                    ),
                    (
                        format!("{} {}", t(l, "telemetry_gpu_temp"), peak_suffix),
                        format!("{:.1} {}", d.run_a.peak_gpu_temp, unit_c),
                        format!("{:.1} {}", d.run_b.peak_gpu_temp, unit_c),
                        format!("{:+0.1} {}", d.peak_gpu_temp_diff, unit_c),
                    ),
                    (
                        format!("{} {}", t(l, "telemetry_power"), avg_suffix),
                        format!("{:.1} {}", d.run_a.avg_power_w, unit_w),
                        format!("{:.1} {}", d.run_b.avg_power_w, unit_w),
                        format!("{:+0.1} {}", d.avg_power_diff_w, unit_w),
                    ),
                    (
                        format!("{} {}", t(l, "telemetry_ram"), peak_suffix),
                        format!("{:.2} {}", d.run_a.peak_ram_gb, unit_gb),
                        format!("{:.2} {}", d.run_b.peak_ram_gb, unit_gb),
                        format!("{:+0.2} {}", d.peak_ram_diff_gb, unit_gb),
                    ),
                    (
                        t(l, "col_gpu").to_string(),
                        translate_gpu_mode(l, &d.run_a.gpu_mode).to_string(),
                        translate_gpu_mode(l, &d.run_b.gpu_mode).to_string(),
                        "-".to_string(),
                    ),
                ];

                let close_cb = Callback::new(move |_| diff.set(None));
                let header = view! { <h3>{t(l, "modal_compare_title")}</h3> }.into_any();
                let footer = view! {
                    <button class="btn btn-primary" on:click=move |_| diff.set(None)>
                        {t(l, "modal_ok_btn")}
                    </button>
                }.into_any();

                view! {
                    <ModalShell
                        panel_style="max-width: 750px;"
                        body_style="overflow-x: auto;"
                        header=header
                        footer=footer
                        on_close=close_cb
                    >
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>{t(l, "modal_compare_metric")}</th>
                                    <th>{format!("{} #{} ({})", t(l, "modal_compare_run"), d.run_a.id, run_a_name)}</th>
                                    <th>{format!("{} #{} ({})", t(l, "modal_compare_run"), d.run_b.id, run_b_name)}</th>
                                    <th>{t(l, "modal_compare_diff")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr>
                                    <td><strong>{t(l, "col_score")}</strong></td>
                                    <td>{score_a}</td>
                                    <td>{score_b}</td>
                                    <td style=format!("font-weight: 700; color: {};", score_diff_color)>
                                        {score_diff_str}
                                    </td>
                                </tr>
                                {rows
                                    .into_iter()
                                    .map(|(lbl, a, b, df)| render_compare_row(lbl, a, b, df, None))
                                    .collect_view()}
                            </tbody>
                        </table>
                    </ModalShell>
                }
            })
        }}
    }
}

#[component]
pub fn GroupModal<FA, FU>(
    target: RwSignal<Option<Vec<i64>>>,
    lang: RwSignal<Language>,
    on_assign: FA,
    on_ungroup: FU,
) -> impl IntoView
where
    FA: Fn(Vec<i64>, String) + Send + Sync + Copy + 'static,
    FU: Fn(Vec<i64>) + Send + Sync + Copy + 'static,
{
    let group_name = RwSignal::new(String::new());

    Effect::new(move |_| {
        if target.get().is_some() {
            group_name.set(String::new());
        }
    });

    view! {
        {move || {
            target.get().map(|run_ids| {
                let l = lang.get();
                let count = run_ids.len();
                let ids_assign = run_ids.clone();
                let ids_remove = run_ids.clone();
                let close_cb = Callback::new(move |_| target.set(None));
                let header = view! { <h3>{t(l, "modal_group_title")}</h3> }.into_any();
                let footer = view! {
                    <button
                        class="btn btn-secondary"
                        on:click=move |_| {
                            target.set(None);
                            on_ungroup(ids_remove.clone());
                        }
                    >
                        {t(l, "modal_group_remove")}
                    </button>
                    <button
                        class="btn btn-primary"
                        on:click=move |_| {
                            let name = group_name.get().trim().to_string();
                            if !name.is_empty() {
                                target.set(None);
                                on_assign(ids_assign.clone(), name);
                            }
                        }
                    >
                        {t(l, "modal_group_assign")}
                    </button>
                }.into_any();

                view! {
                    <ModalShell header=header footer=footer on_close=close_cb>
                        <p>{t(l, "modal_group_desc")}</p>
                        <div style="margin: 0.5rem 0;">
                            <span class="badge badge-info">{format!("{} {}", count, t(l, "runs_selected"))}</span>
                        </div>
                        <div style="margin-top: 1rem;">
                            <label style="display: block; font-size: 0.85rem; margin-bottom: 0.35rem; color: var(--text-secondary);">
                                {t(l, "modal_group_name")}
                            </label>
                            <input
                                type="text"
                                style="width: 100%;"
                                placeholder=t(l, "modal_group_placeholder")
                                prop:value=move || group_name.get()
                                on:input=move |ev| group_name.set(event_target_value(&ev))
                            />
                        </div>
                    </ModalShell>
                }
            })
        }}
    }
}

#[component]
pub fn ConfirmModal<F>(
    target: RwSignal<Option<(String, String)>>,
    lang: RwSignal<Language>,
    on_confirm: F,
) -> impl IntoView
where
    F: Fn() + Send + Sync + Copy + 'static,
{
    view! {
        {move || {
            target.get().map(|(title, msg)| {
                let l = lang.get();
                let close_cb = Callback::new(move |_| target.set(None));
                let header = view! { <h3>{title}</h3> }.into_any();
                let footer = view! {
                    <button class="btn btn-secondary" on:click=move |_| target.set(None)>
                        {t(l, "modal_confirm_no")}
                    </button>
                    <button
                        class="btn btn-danger"
                        on:click=move |_| {
                            target.set(None);
                            on_confirm();
                        }
                    >
                        {t(l, "modal_confirm_yes")}
                    </button>
                }.into_any();

                view! {
                    <ModalShell header=header footer=footer on_close=close_cb>
                        <p>{msg}</p>
                    </ModalShell>
                }
            })
        }}
    }
}

fn format_positive_metric(val: f32, unit: &str, decimals: usize) -> String {
    if val > 0.0 {
        format!("{:.*} {}", decimals, val, unit)
    } else {
        "-".to_string()
    }
}

fn format_positive_freq(mhz: u32, unit: &str) -> String {
    if mhz > 0 {
        format!("{} {}", mhz, unit)
    } else {
        "-".to_string()
    }
}

fn format_score_edit_value(score: Option<f64>) -> String {
    score
        .map(|s| {
            if s.fract().abs() < 1e-9 {
                format!("{:.0}", s)
            } else {
                format!("{:.2}", s)
            }
        })
        .unwrap_or_default()
}

fn throttle_box_class(raw: &str) -> &'static str {
    let lower = raw.trim().to_lowercase();
    if lower.is_empty() || lower == "none" || lower == "no" || lower == "-" {
        "details-metric-box"
    } else if lower.contains("power") {
        "details-metric-box throttle-warning"
    } else {
        "details-metric-box throttle-danger"
    }
}

fn extract_http_urls(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    for token in text.split_whitespace() {
        let candidate = token
            .trim_matches(['"', '\'', '<', '>', '(', '[', '{'])
            .trim_end_matches(['.', ',', ';', ':', ')', ']', '}', '"', '\'']);
        if (candidate.starts_with("https://") || candidate.starts_with("http://"))
            && candidate.len() <= 2048
            && seen.insert(candidate.to_string())
        {
            urls.push(candidate.to_string());
            if urls.len() >= 20 {
                break;
            }
        }
    }
    urls
}

fn render_metric_column(
    title: &'static str,
    rows: Vec<(&'static str, String, &'static str)>,
) -> AnyView {
    view! {
        <div class="details-metric-col">
            <h4>{title}</h4>
            {rows
                .into_iter()
                .map(|(label, value, box_class)| {
                    view! {
                        <div class=box_class>
                            <span class="details-metric-label">{label}</span>
                            <span class="details-metric-value">{value}</span>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
    .into_any()
}

#[component]
pub fn RunDetailsModal<F>(
    target: RwSignal<Option<RunResult>>,
    lang: RwSignal<Language>,
    status_toast: RwSignal<Option<(String, bool)>>,
    on_score_updated: F,
) -> impl IntoView
where
    F: Fn(RunResult) + Send + Sync + Copy + 'static,
{
    let session_seq = RwSignal::new(0u64);
    let loaded_run_id = RwSignal::new(Option::<i64>::None);
    let score_input = RwSignal::new(String::new());
    let saving_score = RwSignal::new(false);
    let show_log = RwSignal::new(false);
    let log_loading = RwSignal::new(false);
    let log_content = RwSignal::new(Option::<String>::None);
    let log_error = RwSignal::new(Option::<String>::None);
    let copied_log = RwSignal::new(false);
    let children = RwSignal::new(Vec::<RunResult>::new());

    // Reset modal transient state whenever a different run is opened (preserve state on in-place score updates)
    Effect::new(move |_| match target.get() {
        None => {
            loaded_run_id.set(None);
            show_log.set(false);
            log_loading.set(false);
            log_content.set(None);
            log_error.set(None);
            copied_log.set(false);
            children.set(Vec::new());
            session_seq.update(|s| *s = s.wrapping_add(1));
        }
        Some(run) => {
            score_input.set(format_score_edit_value(run.score));
            if loaded_run_id.get_untracked() != Some(run.id) {
                loaded_run_id.set(Some(run.id));
                show_log.set(false);
                log_loading.set(false);
                log_content.set(None);
                log_error.set(None);
                copied_log.set(false);
                children.set(Vec::new());
                let next_seq = session_seq.get_untracked().wrapping_add(1);
                session_seq.set(next_seq);

                if run.is_methodology {
                    let meth_id = run.id;
                    leptos::task::spawn_local(async move {
                        if let Ok(list) = tauri_bridge::get_methodology_children(meth_id).await {
                            if session_seq.get_untracked() == next_seq {
                                children.set(list);
                            }
                        }
                    });
                }
            }
        }
    });

    view! {
        {move || {
            target.get().map(|run| {
                let l = lang.get();
                let run_id = run.id;
                let unit_s = t(l, "unit_secs");
                let unit_c = t(l, "unit_celsius");
                let unit_mhz = t(l, "unit_mhz");
                let unit_w = t(l, "unit_watt");
                let unit_gb = t(l, "unit_gb");

                let display_name = bench_name(l, &run.benchmark_id, &run.benchmark_id).to_string();
                let status_txt = translate_status(l, &run.status);
                let status_cls = run_status_badge_class(&run.status);
                let gpu_txt = translate_gpu_mode(l, &run.gpu_mode);
                let cat_txt = translate_category(l, &run.category);
                let date_txt = format_timestamp_local(run.timestamp);
                let cpu_throttle_txt = translate_throttle_status(l, &run.cpu_throttling);
                let gpu_throttle_txt = translate_throttle_status(l, &run.gpu_throttling);
                let sys_info = run.system_info_summary.trim().to_string();
                let version_label = run.preset_or_version.clone();
                let power_profile = if run.power_profile.trim().is_empty() {
                    "-".to_string()
                } else {
                    run.power_profile.clone()
                };

                let on_save_score = move |_| {
                    if saving_score.get_untracked() {
                        return;
                    }
                    let raw = score_input.get_untracked();
                    let trimmed = raw.trim();
                    let parsed = if trimmed.is_empty() {
                        None
                    } else {
                        match trimmed.parse::<f64>() {
                            Ok(v) if v.is_finite() => Some(v),
                            _ => {
                                status_toast.set(Some((
                                    t(lang.get_untracked(), "modal_details_score_invalid").to_string(),
                                    true,
                                )));
                                return;
                            }
                        }
                    };

                    saving_score.set(true);
                    let active_seq = session_seq.get_untracked();
                    leptos::task::spawn_local(async move {
                        let res = tauri_bridge::update_run_score(run_id, parsed).await;
                        if session_seq.get_untracked() != active_seq {
                            return;
                        }
                        saving_score.set(false);
                        match res {
                            Ok(updated) => {
                                score_input.set(format_score_edit_value(updated.score));
                                target.set(Some(updated.clone()));
                                on_score_updated(updated);
                                status_toast.set(Some((
                                    t(lang.get_untracked(), "modal_details_score_saved").to_string(),
                                    false,
                                )));
                            }
                            Err(err) => status_toast.set(Some((err, true))),
                        }
                    });
                };

                let on_toggle_log = move |_| {
                    let next_show = !show_log.get_untracked();
                    show_log.set(next_show);
                    if next_show && log_content.get_untracked().is_none() && !log_loading.get_untracked() {
                        log_loading.set(true);
                        log_error.set(None);
                        let active_seq = session_seq.get_untracked();
                        leptos::task::spawn_local(async move {
                            let res = tauri_bridge::get_run_log(run_id).await;
                            if session_seq.get_untracked() != active_seq {
                                return;
                            }
                            log_loading.set(false);
                            match res {
                                Ok(txt) => log_content.set(Some(txt)),
                                Err(err) => log_error.set(Some(err)),
                            }
                        });
                    }
                };

                let on_copy_log = move |_| {
                    if let Some(txt) = log_content.get_untracked() {
                        copy_text_with_feedback(&txt, copied_log);
                    }
                };

                let close_cb = Callback::new(move |_| target.set(None));
                let header = view! {
                    <div style="display: flex; align-items: center; gap: 0.55rem; flex-wrap: wrap;">
                        <h3>{t(l, "modal_details_title")}</h3>
                        <span class="badge badge-info">{format!("#{}", run_id)}</span>
                        <span class=status_cls>{status_txt}</span>
                        {run.is_methodology.then(|| view! {
                            <span class="badge badge-warning">{t(l, "methodology_badge")}</span>
                        })}
                        {run.group_name.clone().map(|grp| view! {
                            <span class="badge badge-info">{grp}</span>
                        })}
                    </div>
                }.into_any();
                let footer = view! {
                    <span style="font-size: 0.82rem; color: var(--text-secondary);">
                        {format!("{}: {}", t(l, "modal_details_date_label"), date_txt)}
                    </span>
                    <button class="btn btn-primary" on:click=move |_| target.set(None)>
                        {t(l, "modal_eula_close")}
                    </button>
                }.into_any();

                let cpu_rows = vec![
                    (t(l, "modal_details_avg_usage"), format!("{:.0}%", run.avg_cpu_usage), "details-metric-box"),
                    (t(l, "modal_details_peak_usage"), format!("{:.0}%", run.peak_cpu_usage), "details-metric-box"),
                    (t(l, "modal_details_avg_temp"), format_positive_metric(run.avg_cpu_temp, unit_c, 1), "details-metric-box"),
                    (t(l, "modal_details_peak_temp"), format_positive_metric(run.peak_cpu_temp, unit_c, 1), "details-metric-box"),
                    (t(l, "modal_details_avg_freq"), format_positive_freq(run.avg_cpu_freq_mhz, unit_mhz), "details-metric-box"),
                    (t(l, "modal_details_peak_freq"), format_positive_freq(run.peak_cpu_freq_mhz, unit_mhz), "details-metric-box"),
                    (t(l, "modal_details_avg_sys_power"), format_positive_metric(run.avg_power_w, unit_w, 1), "details-metric-box"),
                    (t(l, "modal_details_peak_sys_power"), format_positive_metric(run.peak_power_w, unit_w, 1), "details-metric-box"),
                    (t(l, "modal_details_cpu_throttle"), cpu_throttle_txt, throttle_box_class(&run.cpu_throttling)),
                ];
                let gpu_rows = vec![
                    (t(l, "modal_details_avg_usage"), format!("{:.0}%", run.avg_gpu_usage), "details-metric-box"),
                    (t(l, "modal_details_peak_usage"), format!("{:.0}%", run.peak_gpu_usage), "details-metric-box"),
                    (t(l, "modal_details_avg_temp"), format_positive_metric(run.avg_gpu_temp, unit_c, 1), "details-metric-box"),
                    (t(l, "modal_details_peak_temp"), format_positive_metric(run.peak_gpu_temp, unit_c, 1), "details-metric-box"),
                    (t(l, "modal_details_avg_freq"), format_positive_freq(run.avg_gpu_freq_mhz, unit_mhz), "details-metric-box"),
                    (t(l, "modal_details_peak_freq"), format_positive_freq(run.peak_gpu_freq_mhz, unit_mhz), "details-metric-box"),
                    (t(l, "modal_details_avg_gpu_power"), format_positive_metric(run.avg_gpu_power_w, unit_w, 1), "details-metric-box"),
                    (t(l, "modal_details_peak_gpu_power"), format_positive_metric(run.peak_gpu_power_w, unit_w, 1), "details-metric-box"),
                    (t(l, "modal_details_gpu_throttle"), gpu_throttle_txt, throttle_box_class(&run.gpu_throttling)),
                ];
                let mem_rows = vec![
                    (t(l, "modal_details_ram_avg"), format_positive_metric(run.avg_ram_gb, unit_gb, 2), "details-metric-box"),
                    (t(l, "modal_details_ram_peak"), format_positive_metric(run.peak_ram_gb, unit_gb, 2), "details-metric-box"),
                    (t(l, "modal_details_vram_avg"), format_positive_metric(run.avg_vram_gb, unit_gb, 2), "details-metric-box"),
                    (t(l, "modal_details_vram_peak"), format_positive_metric(run.peak_vram_gb, unit_gb, 2), "details-metric-box"),
                    (t(l, "modal_details_vram_freq_avg"), format_positive_freq(run.avg_vram_freq_mhz, unit_mhz), "details-metric-box"),
                    (t(l, "modal_details_vram_freq_peak"), format_positive_freq(run.peak_vram_freq_mhz, unit_mhz), "details-metric-box"),
                ];
                let other_rows = vec![
                    (t(l, "modal_details_avg_ac_power"), format_positive_metric(run.avg_ac_power_w, unit_w, 1), "details-metric-box"),
                    (t(l, "modal_details_peak_ac_power"), format_positive_metric(run.peak_ac_power_w, unit_w, 1), "details-metric-box"),
                    (t(l, "modal_details_power_profile"), power_profile, "details-metric-box"),
                    (t(l, "modal_details_category"), cat_txt, "details-metric-box"),
                    (t(l, "col_duration"), format!("{:.1} {}", run.duration_secs, unit_s), "details-metric-box"),
                    (t(l, "col_gpu"), gpu_txt.to_string(), "details-metric-box"),
                ];

                view! {
                    <ModalShell
                        panel_class="modal-panel modal-panel-wide modal-panel-details"
                        footer_style="justify-content: space-between; align-items: center;"
                        header=header
                        footer=footer
                        on_close=close_cb
                    >
                        <div class="details-summary-strip">
                            <div>
                                <div style="font-size: 1.05rem; font-weight: 700; color: var(--text-primary);">
                                    {if version_label.is_empty() {
                                        display_name
                                    } else {
                                        format!("{} ({})", display_name, version_label)
                                    }}
                                </div>
                                <div style="font-size: 0.82rem; color: var(--text-secondary); margin-top: 0.25rem;">
                                    {format!(
                                        "{}: {}  |  {}: {:.1} {}",
                                        t(l, "col_gpu"),
                                        gpu_txt,
                                        t(l, "col_duration"),
                                        run.duration_secs,
                                        unit_s
                                    )}
                                </div>
                            </div>

                            <div class="details-score-editor">
                                <label style="font-size: 0.85rem; color: var(--text-secondary); font-weight: 600;">
                                    {format!("{}:", t(l, "col_score"))}
                                </label>
                                <input
                                    type="text"
                                    class="details-score-input"
                                    prop:value=move || score_input.get()
                                    on:input=move |ev| score_input.set(event_target_value(&ev))
                                />
                                <button
                                    class="btn btn-primary btn-sm"
                                    disabled=move || saving_score.get()
                                    on:click=on_save_score
                                >
                                    <Icon kind=IconKind::Save size=13 />
                                    <span>{t(l, "modal_details_score_save")}</span>
                                </button>
                            </div>
                        </div>

                        <div class="details-metrics-grid">
                            {render_metric_column(t(l, "modal_details_cpu_section"), cpu_rows)}
                            {render_metric_column(t(l, "modal_details_gpu_section"), gpu_rows)}
                            {render_metric_column(t(l, "modal_details_mem_section"), mem_rows)}
                            {render_metric_column(t(l, "modal_details_other_section"), other_rows)}
                        </div>

                        {(!sys_info.is_empty()).then(|| view! {
                            <div class="details-sys-banner">
                                <strong>{format!("{}: ", t(l, "modal_details_system_info"))}</strong>
                                <span>{sys_info}</span>
                            </div>
                        })}

                        {move || {
                            let child_list = children.get();
                            let l_cur = lang.get();
                            let unit_s_cur = t(l_cur, "unit_secs");
                            if child_list.is_empty() {
                                view! { <span></span> }.into_any()
                            } else {
                                view! {
                                    <div style="margin-top: 0.9rem;">
                                        <div style="font-size: 0.85rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 0.45rem;">
                                            {t(l_cur, "modal_details_children_title")}
                                        </div>
                                        <div class="data-table-container">
                                            <table class="data-table">
                                                <thead>
                                                    <tr>
                                                        <th>{t(l_cur, "col_id")}</th>
                                                        <th>{t(l_cur, "col_version")}</th>
                                                        <th>{t(l_cur, "col_score")}</th>
                                                        <th>{t(l_cur, "col_duration")}</th>
                                                        <th>{t(l_cur, "col_date")}</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {child_list.into_iter().map(|child| view! {
                                                        <tr>
                                                            <td>{format!("#{}", child.id)}</td>
                                                            <td>{child.preset_or_version}</td>
                                                            <td style="font-weight: 700; color: var(--accent-color);">
                                                                {child.score.map(|s| format!("{:.2}", s)).unwrap_or_else(|| "-".into())}
                                                            </td>
                                                            <td>{format!("{:.1} {}", child.duration_secs, unit_s_cur)}</td>
                                                            <td>{format_timestamp_local(child.timestamp)}</td>
                                                        </tr>
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        }}

                        <div style="margin-top: 1rem;">
                            <div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.5rem;">
                                <div style="font-size: 0.88rem; font-weight: 600; color: var(--text-primary); display: inline-flex; align-items: center; gap: 0.4rem;">
                                    <Icon kind=IconKind::Terminal size=15 />
                                    <span>{t(l, "modal_details_run_log")}</span>
                                </div>
                                <div style="display: flex; gap: 0.5rem; align-items: center;">
                                    {move || {
                                        let l_cur = lang.get();
                                        let has_nonempty_log = show_log.get()
                                            && log_content
                                                .with(|c| c.as_ref().is_some_and(|s| !s.trim().is_empty()));
                                        if has_nonempty_log {
                                            view! {
                                                <button class="btn btn-secondary btn-sm" on:click=on_copy_log>
                                                    <Icon kind=IconKind::Copy size=13 />
                                                    <span>
                                                        {if copied_log.get() {
                                                            t(l_cur, "copied")
                                                        } else {
                                                            t(l_cur, "copy_logs")
                                                        }}
                                                    </span>
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }
                                    }}
                                    <button
                                        class=move || {
                                            if show_log.get() {
                                                "btn btn-secondary btn-sm"
                                            } else {
                                                "btn btn-primary btn-sm"
                                            }
                                        }
                                        on:click=on_toggle_log
                                    >
                                        <Icon kind=IconKind::FileText size=13 />
                                        <span>
                                            {move || {
                                                let l_cur = lang.get();
                                                if show_log.get() {
                                                    t(l_cur, "modal_details_hide_log")
                                                } else {
                                                    t(l_cur, "modal_details_show_log")
                                                }
                                            }}
                                        </span>
                                    </button>
                                </div>
                            </div>

                            {move || {
                                if !show_log.get() {
                                    return view! { <span></span> }.into_any();
                                }
                                let l_cur = lang.get();
                                if log_loading.get() {
                                    return view! {
                                        <div class="eula-box">{t(l_cur, "modal_details_loading_log")}</div>
                                    }.into_any();
                                }
                                if let Some(err) = log_error.get() {
                                    return view! {
                                        <div class="install-error-notice">{err}</div>
                                    }.into_any();
                                }
                                let txt = log_content.get().unwrap_or_default();
                                if txt.trim().is_empty() {
                                    return view! {
                                        <div class="eula-box">{t(l_cur, "modal_details_no_log")}</div>
                                    }.into_any();
                                }
                                let urls = extract_http_urls(&txt);
                                view! {
                                    <div>
                                        <div class="terminal-box modal-terminal-box">{txt}</div>
                                        {(!urls.is_empty()).then(|| view! {
                                            <div style="margin-top: 0.55rem;">
                                                <div style="font-size: 0.78rem; font-weight: 600; color: var(--text-secondary);">
                                                    {format!("{}:", t(l_cur, "modal_details_detected_links"))}
                                                </div>
                                                <div class="details-links-row">
                                                    {urls.into_iter().map(|url| {
                                                        let url_open = url.clone();
                                                        view! {
                                                            <button
                                                                class="btn btn-secondary btn-sm"
                                                                on:click=move |_| {
                                                                    let u = url_open.clone();
                                                                    spawn_toast_listener(status_toast, async move {
                                                                        tauri_bridge::open_external_url(&u).await
                                                                    });
                                                                }
                                                            >
                                                                <Icon kind=IconKind::ExternalLink size=12 />
                                                                <span>{url}</span>
                                                            </button>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            </div>
                                        })}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    </ModalShell>
                }
            })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_http_urls_deduplicates_and_strips_punctuation() {
        let log = "Result URL: (https://browser.geekbench.com/v6/cpu/12345).\nAlso see https://browser.geekbench.com/v6/cpu/12345 and http://localhost:8080/report, ignore file:///etc/passwd.";
        let urls = extract_http_urls(log);
        assert_eq!(
            urls,
            vec![
                "https://browser.geekbench.com/v6/cpu/12345".to_string(),
                "http://localhost:8080/report".to_string(),
            ]
        );
    }

    #[test]
    fn test_format_score_edit_value_formats_cleanly() {
        assert_eq!(format_score_edit_value(None), "");
        assert_eq!(format_score_edit_value(Some(12345.0)), "12345");
        assert_eq!(format_score_edit_value(Some(42.195)), "42.20");
    }

    #[test]
    fn test_throttle_box_class_maps_variants() {
        assert_eq!(throttle_box_class("none"), "details-metric-box");
        assert_eq!(throttle_box_class("-"), "details-metric-box");
        assert_eq!(
            throttle_box_class("power_limit"),
            "details-metric-box throttle-warning"
        );
        assert_eq!(
            throttle_box_class("thermal"),
            "details-metric-box throttle-danger"
        );
    }
}
