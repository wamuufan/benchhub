//! Main Application View & State Orchestrator
//!
//! Synchronizes application state, active runs, execution queues, settings,
//! theme preferences, hardware telemetry, and localized notifications with `benchhubd`.

use crate::components::nav_header::AppTab;
use crate::components::{
    apply_system_locale_async, spawn_toast_listener, CompareModal, ConfirmModal, CustomArgsModal,
    EulaModal, GroupModal, HistoryTab, MonitorTab, NavHeader, ProfileCatalogKind,
    ProfileCatalogTab, RunDetailsModal, SettingsTab,
};
use crate::i18n::{detect_system_language, t, translate_notification, Language};
use crate::tauri_bridge;
use crate::theme::ThemeState;
use benchhub_types::{
    ActiveTaskInfo, DaemonNotification, DaemonStateSnapshot, GpuMode, QueueItem, RunComparisonDiff,
    RunResult, TelemetryData, TerminalRingBuffer,
};
use leptos::either::Either;
use leptos::prelude::*;
use std::collections::HashMap;

#[component]
pub fn App() -> impl IntoView {
    // Desktop Theme State & Polling Engine
    let theme_state = ThemeState::new();
    theme_state.start_sync();

    // Global Reactive Application Signals
    let lang = RwSignal::new(detect_system_language());
    let active_tab = RwSignal::new(AppTab::Benchmarks);
    let active_run = RwSignal::new(Option::<ActiveTaskInfo>::None);
    let status_toast = RwSignal::new(Option::<(String, bool)>::None);
    let toast_seq = RwSignal::new(0u64);
    let live_telemetry = RwSignal::new(Option::<TelemetryData>::None);
    let terminal_buf = RwSignal::new(TerminalRingBuffer::new(500));
    let initial_logs_hydrated = RwSignal::new(false);
    let now_epoch_ms = RwSignal::new(js_sys::Date::now() as i64);
    let custom_args_map = RwSignal::new(HashMap::<String, String>::new());
    let installed_map = RwSignal::new(HashMap::<String, bool>::new());
    let installed_versions_map = RwSignal::new(HashMap::<String, bool>::new());
    let queue_items = RwSignal::new(Vec::<QueueItem>::new());
    let is_queue_running = RwSignal::new(false);
    let selected_gpu_mode = RwSignal::new(GpuMode::NvidiaDgpu);
    let history_refresh_tick = RwSignal::new(0u64);
    let last_stopped_task = RwSignal::new((0u64, String::new()));

    // Modal Visibility Signals
    let show_eula_modal = RwSignal::new(Option::<(String, String)>::None);
    let show_custom_args_modal = RwSignal::new(Option::<(String, String, String)>::None);
    let show_details_modal = RwSignal::new(Option::<RunResult>::None);
    let show_compare_modal = RwSignal::new(Option::<RunComparisonDiff>::None);
    let show_group_modal = RwSignal::new(Option::<Vec<i64>>::None);
    let show_confirm_modal = RwSignal::new(Option::<(String, String)>::None);

    let show_toast = move |msg: String, is_err: bool| {
        let next = toast_seq.get_untracked().wrapping_add(1);
        toast_seq.set(next);
        status_toast.set(Some((msg, is_err)));
        if !is_err {
            leptos::task::spawn_local(async move {
                crate::theme::sleep_ms(5000).await;
                if toast_seq.get_untracked() == next {
                    status_toast.set(None);
                }
            });
        }
    };

    let apply_snapshot = move |snapshot: DaemonStateSnapshot, sync_lang_and_theme: bool| {
        installed_map.set(snapshot.installed_map);
        installed_versions_map.set(snapshot.installed_versions_map);
        active_run.set(snapshot.active_task);
        queue_items.set(snapshot.queue);
        is_queue_running.set(snapshot.is_queue_running);
        let limit = snapshot.settings.terminal_buffer_lines.clamp(50, 1000);
        if terminal_buf.with_untracked(|buf| buf.max_lines() != limit) {
            terminal_buf.update(|buf| buf.set_max_lines(limit));
        }

        if !initial_logs_hydrated.get_untracked() {
            initial_logs_hydrated.set(true);
            if !snapshot.recent_terminal_logs.is_empty()
                && terminal_buf.with_untracked(|b| b.is_empty())
            {
                terminal_buf.update(|buf| {
                    buf.replace_from_snapshot_lines(&snapshot.recent_terminal_logs);
                });
            }
        }

        if sync_lang_and_theme {
            theme_state.apply_settings(&snapshot.settings.theme);
            selected_gpu_mode.set(snapshot.settings.selected_gpu_mode);
            let cfg_lang = snapshot.settings.language.clone();
            if cfg_lang == "system" || cfg_lang == "auto" || cfg_lang.is_empty() {
                apply_system_locale_async(lang);
            } else {
                lang.set(Language::from_config(&cfg_lang));
            }
        } else if active_tab.get_untracked() != AppTab::Settings {
            theme_state.apply_settings(&snapshot.settings.theme);
        }
    };

    // Initial daemon state snapshot load
    leptos::task::spawn_local(async move {
        match tauri_bridge::get_daemon_snapshot().await {
            Ok(snapshot) => apply_snapshot(snapshot, true),
            Err(err) => {
                apply_system_locale_async(lang);
                show_toast(err, true);
            }
        }
    });

    // Subscribe to daemon state, notifications, telemetry, and terminal streams
    spawn_toast_listener(
        status_toast,
        tauri_bridge::listen_daemon_state(move |snapshot| {
            apply_snapshot(snapshot, false);
        }),
    );

    spawn_toast_listener(
        status_toast,
        tauri_bridge::listen_daemon_notification(move |notif| {
            let (msg, is_err) = translate_notification(lang.get_untracked(), &notif);
            show_toast(msg, is_err);
            if let DaemonNotification::TaskStopped { ref benchmark_id } = notif {
                let next_seq = last_stopped_task.with_untracked(|(seq, _)| seq.wrapping_add(1));
                last_stopped_task.set((next_seq, benchmark_id.clone()));
            }
            if matches!(
                notif,
                DaemonNotification::BenchmarkCompleted { .. }
                    | DaemonNotification::TaskStopped { .. }
                    | DaemonNotification::TaskFailed { .. }
            ) {
                history_refresh_tick.update(|v| *v = v.wrapping_add(1));
            }
        }),
    );

    let mut was_monitor_active = false;
    Effect::new(move |_| {
        let is_monitor = active_tab.get() == AppTab::Monitor;
        if is_monitor != was_monitor_active {
            was_monitor_active = is_monitor;
            leptos::task::spawn_local(async move {
                let _ = tauri_bridge::set_monitor_active(is_monitor).await;
            });
        }
    });

    spawn_toast_listener(
        status_toast,
        tauri_bridge::listen_telemetry(move |data| {
            if active_tab.get_untracked() == AppTab::Monitor {
                live_telemetry.set(Some(data));
            }
        }),
    );

    spawn_toast_listener(
        status_toast,
        tauri_bridge::listen_terminal_log(move |chunk| {
            terminal_buf.update(|buf| buf.push_str(&chunk));
        }),
    );

    spawn_toast_listener(
        status_toast,
        tauri_bridge::listen_terminal_clear(move || {
            terminal_buf.update(|buf| buf.clear());
        }),
    );

    // Lightweight 1-second clock ticker while any task is active
    leptos::task::spawn_local(async move {
        loop {
            crate::theme::sleep_ms(1000).await;
            if active_run.get_untracked().is_some() {
                now_epoch_ms.set(js_sys::Date::now() as i64);
            }
        }
    });

    // Modal Callback Handlers
    let on_custom_args_save = move |id: String, _ver: String, args: String| {
        let trimmed = (!args.trim().is_empty()).then(|| args.clone());
        custom_args_map.update(|map| {
            map.insert(id.clone(), args);
        });
        if !is_queue_running.get_untracked() {
            let mut updated = queue_items.get_untracked();
            let mut changed = false;
            for item in &mut updated {
                if item.id == id && item.custom_args != trimmed {
                    item.custom_args = trimmed.clone();
                    changed = true;
                }
            }
            if changed {
                queue_items.set(updated.clone());
                leptos::task::spawn_local(async move {
                    if let Err(err) = tauri_bridge::set_queue(&updated).await {
                        show_toast(err, true);
                    }
                });
            }
        }
        show_toast(
            t(lang.get_untracked(), "custom_args_saved_msg").to_string(),
            false,
        );
    };

    let on_group_assign = move |run_ids: Vec<i64>, group_name: String| {
        leptos::task::spawn_local(async move {
            match tauri_bridge::group_runs(&run_ids, &group_name).await {
                Ok(_) => history_refresh_tick.update(|v| *v = v.wrapping_add(1)),
                Err(err) => show_toast(err, true),
            }
        });
    };

    let on_group_ungroup = move |run_ids: Vec<i64>| {
        leptos::task::spawn_local(async move {
            match tauri_bridge::ungroup_runs(&run_ids).await {
                Ok(_) => history_refresh_tick.update(|v| *v = v.wrapping_add(1)),
                Err(err) => show_toast(err, true),
            }
        });
    };

    let on_confirm_action = move || {
        leptos::task::spawn_local(async move {
            match tauri_bridge::clear_history().await {
                Ok(_) => {
                    history_refresh_tick.update(|v| *v = v.wrapping_add(1));
                    show_toast(
                        t(lang.get_untracked(), "history_cleared_msg").to_string(),
                        false,
                    );
                }
                Err(err) => show_toast(err, true),
            }
        });
    };

    view! {
        <div
            class=move || theme_state.compute_classes()
            style=move || theme_state.compute_styles()
        >
            // Sidebar Navigation
            <NavHeader
                active_tab=active_tab
                lang=lang
                active_run=active_run
                sys_theme=theme_state.sys_theme_info
                status_toast=status_toast
                now_epoch_ms=now_epoch_ms
            />

            // Main Tab Content Router
            <main style="flex: 1; display: flex; flex-direction: column; overflow: hidden;">
                {move || {
                    let catalog_view = |kind: ProfileCatalogKind| {
                        view! {
                            <ProfileCatalogTab
                                kind=kind
                                lang=lang
                                active_tab=active_tab
                                active_run=active_run
                                status_toast=status_toast
                                show_eula_modal=show_eula_modal
                                show_custom_args_modal=show_custom_args_modal
                                custom_args_map=custom_args_map
                                installed_map=installed_map
                                installed_versions_map=installed_versions_map
                                queue_items=queue_items
                                is_queue_running=is_queue_running
                                selected_gpu_mode=selected_gpu_mode
                            />
                        }
                    };
                    // Use distinct concrete Either types so AnyView always unmounts/remounts cleanly
                    // when switching directly between Benchmarks and StressTests.
                    match active_tab.get() {
                        AppTab::Benchmarks => Either::<_, ()>::Left(catalog_view(
                            ProfileCatalogKind::Benchmarks,
                        ))
                        .into_any(),
                        AppTab::StressTests => Either::<(), _>::Right(catalog_view(
                            ProfileCatalogKind::StressTests,
                        ))
                        .into_any(),

                    AppTab::Monitor => view! {
                        <MonitorTab
                            lang=lang
                            active_run=active_run
                            live_telemetry=live_telemetry
                            terminal_buf=terminal_buf
                            status_toast=status_toast
                            now_epoch_ms=now_epoch_ms
                        />
                    }.into_any(),

                    AppTab::History => view! {
                        <HistoryTab
                            lang=lang
                            active_tab=active_tab
                            status_toast=status_toast
                            refresh_tick=history_refresh_tick
                            show_details_modal=show_details_modal
                            show_compare_modal=show_compare_modal
                            show_group_modal=show_group_modal
                            show_confirm_modal=show_confirm_modal
                        />
                    }.into_any(),

                    AppTab::Settings => view! {
                        <SettingsTab
                            lang=lang
                            theme_state=theme_state
                            status_toast=status_toast
                        />
                    }.into_any(),
                    }
                }}
            </main>

            // Non-intrusive Toast Banner
            {move || {
                status_toast.get().map(|(msg, is_err)| {
                    let l = lang.get();
                    view! {
                        <div class="status-toast" class:error=is_err>
                            <span style="flex: 1;">{msg}</span>
                            <button
                                class="btn btn-sm btn-secondary"
                                on:click=move |_| status_toast.set(None)
                            >
                                {t(l, "dismiss_toast")}
                            </button>
                        </div>
                    }
                })
            }}

            // Modals
            <EulaModal
                target=show_eula_modal
                lang=lang
                active_tab=active_tab
                terminal_buf=terminal_buf
                status_toast=status_toast
                now_epoch_ms=now_epoch_ms
                last_stopped_task=last_stopped_task
            />

            <CustomArgsModal
                target=show_custom_args_modal
                lang=lang
                on_save=on_custom_args_save
            />

            <CompareModal
                diff=show_compare_modal
                lang=lang
            />

            <GroupModal
                target=show_group_modal
                lang=lang
                on_assign=on_group_assign
                on_ungroup=on_group_ungroup
            />

            <RunDetailsModal
                target=show_details_modal
                lang=lang
                status_toast=status_toast
                on_score_updated=move |_| history_refresh_tick.update(|v| *v = v.wrapping_add(1))
            />

            <ConfirmModal
                target=show_confirm_modal
                lang=lang
                on_confirm=on_confirm_action
            />
        </div>
    }
}
