//! Sidebar Navigation & Global Header Component
//!
//! Renders the desktop sidebar with tab navigation, brand identity,
//! live execution indicator, and benchmark cancellation controls.

use super::icons::{Icon, IconKind};
use super::{format_elapsed_mmss, spawn_toast_listener};
use crate::i18n::{bench_name, t, Language};
use crate::tauri_bridge;
use benchhub_types::{ActiveTaskInfo, SystemThemeInfo};
use leptos::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTab {
    #[default]
    Benchmarks,
    StressTests,
    Monitor,
    History,
    Settings,
}

const NAV_ITEMS: [(AppTab, IconKind, &str); 5] = [
    (AppTab::Benchmarks, IconKind::Timer, "tab_benchmarks"),
    (AppTab::StressTests, IconKind::Flame, "tab_stress"),
    (AppTab::Monitor, IconKind::Activity, "tab_monitor"),
    (AppTab::History, IconKind::History, "tab_history"),
    (AppTab::Settings, IconKind::Settings, "tab_settings"),
];

#[component]
pub fn NavHeader(
    active_tab: RwSignal<AppTab>,
    lang: RwSignal<Language>,
    active_run: RwSignal<Option<ActiveTaskInfo>>,
    sys_theme: RwSignal<SystemThemeInfo>,
    status_toast: RwSignal<Option<(String, bool)>>,
    now_epoch_ms: RwSignal<i64>,
) -> impl IntoView {
    let on_cancel = move |_| {
        spawn_toast_listener(status_toast, tauri_bridge::cancel_active_run());
    };

    view! {
        <aside class="app-sidebar">
            <div>
                <div class="app-brand">
                    <div class="app-brand-icon">
                        <Icon kind=IconKind::BrandBolt size=18 />
                    </div>
                    <div class="app-brand-info">
                        <h2>{move || t(lang.get(), "app_title")}</h2>
                        <p>{move || t(lang.get(), "app_subtitle")}</p>
                    </div>
                </div>

                <nav class="nav-list">
                    {NAV_ITEMS
                        .into_iter()
                        .map(|(tab, icon_kind, label_key)| {
                            view! {
                                <div
                                    class="nav-item"
                                    class:active=move || active_tab.get() == tab
                                    on:click=move |_| active_tab.set(tab)
                                >
                                    <Icon kind=icon_kind size=17 />
                                    <span>{move || t(lang.get(), label_key)}</span>
                                </div>
                            }
                        })
                        .collect_view()}
                </nav>
            </div>

            <div class="sidebar-footer">
                {move || {
                    let l = lang.get();
                    if let Some(ref current) = active_run.get() {
                        let status_text = if current.is_install {
                            t(l, "status_installing")
                        } else {
                            t(l, "status_running")
                        };

                        view! {
                            <div style="display: flex; flex-direction: column; gap: 0.5rem;">
                                <div class="status-pill">
                                    <div class="status-dot running"></div>
                                    <span style="font-weight: 600; color: var(--color-warning);">
                                        {status_text}
                                    </span>
                                </div>
                                <div style="font-size: 0.78rem; color: var(--text-secondary); word-break: break-all;">
                                    {format!(
                                        "{} ({})",
                                        bench_name(l, &current.benchmark_id, &current.benchmark_id),
                                        current.version
                                    )}
                                </div>
                                {
                                    let started = current.started_at_epoch_ms;
                                    view! {
                                        <div style="font-size: 0.75rem; color: var(--text-secondary);">
                                            {move || format!(
                                                "{} {}",
                                                t(lang.get(), "elapsed_label"),
                                                format_elapsed_mmss(now_epoch_ms.get(), started)
                                            )}
                                        </div>
                                    }
                                }
                                <button
                                    class="btn btn-danger btn-sm"
                                    style="width: 100%; margin-top: 0.25rem;"
                                    on:click=on_cancel
                                >
                                    <Icon kind=IconKind::Stop size=14 />
                                    <span>{t(l, "cancel_active")}</span>
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        let de_name = sys_theme.get().desktop_env;
                        view! {
                            <div class="status-pill">
                                <div class="status-dot"></div>
                                <span>{t(l, "status_ready")}{format!(" ({})", de_name)}</span>
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </aside>
    }
}
