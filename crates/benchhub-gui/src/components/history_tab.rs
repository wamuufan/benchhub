//! History Tab Component
//!
//! Renders past benchmark results in an interactive data table with multi-select,
//! filtering, sorting, methodology aggregation, daemon-computed run comparisons,
//! CSV/PNG reporting exports, group assignments, and deletion.

use super::icons::{Icon, IconKind};
use super::nav_header::AppTab;
use super::{
    format_timestamp_local, is_escape_shortcut, is_select_all_shortcut, run_status_badge_class,
    should_ignore_shortcut, spawn_toast_listener, LocalizedSelect, SelectAllButton, SelectOption,
};
use crate::i18n::{bench_name, t, translate_gpu_mode, translate_status, Language};
use crate::tauri_bridge;
use benchhub_types::{HistorySortOrder, RunComparisonDiff, RunResult};
use leptos::prelude::*;
use std::collections::HashSet;

const HISTORY_CATEGORY_OPTIONS: &[SelectOption] = &[
    SelectOption::key("all", "all_categories"),
    SelectOption::key("CPU", "category_cpu"),
    SelectOption::key("GPU", "category_gpu"),
    SelectOption::key("Render", "category_render"),
    SelectOption::key("Storage", "category_disk"),
    SelectOption::key("Memory", "category_memory"),
    SelectOption::key("AI", "category_ai"),
    SelectOption::key("System", "category_system"),
];

const HISTORY_GPU_OPTIONS: &[SelectOption] = &[
    SelectOption::key("all", "gpu_filter_all"),
    SelectOption::key("nvidia_dgpu", "gpu_nvidia"),
    SelectOption::key("integrated", "gpu_integrated"),
    SelectOption::key("auto", "gpu_auto"),
];

const HISTORY_STATUS_OPTIONS: &[SelectOption] = &[
    SelectOption::key("all", "status_filter_all"),
    SelectOption::key("completed", "status_completed"),
    SelectOption::key("cancelled", "status_cancelled"),
    SelectOption::key("timeout", "status_timeout"),
    SelectOption::key("failed", "status_failed"),
];

const HISTORY_SORT_OPTIONS: &[SelectOption] = &[
    SelectOption::key("date_desc", "sort_date_desc"),
    SelectOption::key("date_asc", "sort_date_asc"),
    SelectOption::key("score_desc", "sort_score_desc"),
    SelectOption::key("score_asc", "sort_score_asc"),
    SelectOption::key("cpu_temp_peak", "sort_cpu_temp"),
    SelectOption::key("gpu_temp_peak", "sort_gpu_temp"),
    SelectOption::key("power_avg", "sort_power"),
];

fn sort_order_to_value(order: HistorySortOrder) -> &'static str {
    match order {
        HistorySortOrder::DateDesc => "date_desc",
        HistorySortOrder::DateAsc => "date_asc",
        HistorySortOrder::ScoreDesc => "score_desc",
        HistorySortOrder::ScoreAsc => "score_asc",
        HistorySortOrder::CpuTempPeak => "cpu_temp_peak",
        HistorySortOrder::GpuTempPeak => "gpu_temp_peak",
        HistorySortOrder::PowerAvg => "power_avg",
    }
}

fn parse_sort_order(val: &str) -> HistorySortOrder {
    match val {
        "date_asc" => HistorySortOrder::DateAsc,
        "score_desc" => HistorySortOrder::ScoreDesc,
        "score_asc" => HistorySortOrder::ScoreAsc,
        "cpu_temp_peak" => HistorySortOrder::CpuTempPeak,
        "gpu_temp_peak" => HistorySortOrder::GpuTempPeak,
        "power_avg" => HistorySortOrder::PowerAvg,
        _ => HistorySortOrder::DateDesc,
    }
}

fn toggle_selected_id(selected: &mut HashSet<i64>, id: i64) {
    if !selected.remove(&id) {
        selected.insert(id);
    }
}

fn toggle_select_all_ids(selected: &mut HashSet<i64>, visible_ids: &[i64]) {
    if visible_ids.is_empty() {
        return;
    }
    let all_selected = visible_ids.iter().all(|id| selected.contains(id));
    if all_selected {
        for id in visible_ids {
            selected.remove(id);
        }
    } else {
        for &id in visible_ids {
            selected.insert(id);
        }
    }
}

fn retain_existing_selected_ids(selected: &mut HashSet<i64>, loaded_runs: &[RunResult]) {
    let valid_ids: HashSet<i64> = loaded_runs.iter().map(|r| r.id).collect();
    selected.retain(|id| valid_ids.contains(id));
}

#[component]
pub fn HistoryTab(
    lang: RwSignal<Language>,
    active_tab: RwSignal<AppTab>,
    status_toast: RwSignal<Option<(String, bool)>>,
    refresh_tick: RwSignal<u64>,
    show_details_modal: RwSignal<Option<RunResult>>,
    show_compare_modal: RwSignal<Option<RunComparisonDiff>>,
    show_group_modal: RwSignal<Option<Vec<i64>>>,
    show_confirm_modal: RwSignal<Option<(String, String)>>,
) -> impl IntoView {
    let runs = RwSignal::new(Vec::<RunResult>::new());
    let selected_ids = RwSignal::new(HashSet::<i64>::new());

    let search_query = RwSignal::new(String::new());
    let category_filter = RwSignal::new("all".to_string());
    let gpu_mode_filter = RwSignal::new("all".to_string());
    let status_filter = RwSignal::new("all".to_string());
    let sort_order = RwSignal::new(HistorySortOrder::DateDesc);

    let fetch_history = move || {
        let s = search_query.get_untracked();
        let cat = category_filter.get_untracked();
        let gpu = gpu_mode_filter.get_untracked();
        let st = status_filter.get_untracked();
        let sort = sort_order.get_untracked();

        leptos::task::spawn_local(async move {
            match tauri_bridge::get_history(&s, &cat, &gpu, &st, sort).await {
                Ok(list) => {
                    selected_ids.update(|set| retain_existing_selected_ids(set, &list));
                    runs.set(list);
                }
                Err(err) => status_toast.set(Some((err, true))),
            }
        });
    };

    // Initial load and automatic reload whenever App bumps `refresh_tick`
    Effect::new(move |_| {
        let _ = refresh_tick.get();
        fetch_history();
    });

    let all_visible_selected = Memo::new(move |_| {
        let list = runs.get();
        let selected = selected_ids.get();
        !list.is_empty() && list.iter().all(|r| selected.contains(&r.id))
    });

    let on_toggle_select_all = move || {
        let visible_ids: Vec<i64> = runs.with_untracked(|list| list.iter().map(|r| r.id).collect());
        if visible_ids.is_empty() {
            let l = lang.get_untracked();
            status_toast.set(Some((t(l, "no_history_to_select_msg").to_string(), false)));
            return;
        }
        selected_ids.update(|set| toggle_select_all_ids(set, &visible_ids));
    };

    let on_toggle_row = move |id: i64| {
        selected_ids.update(|set| toggle_selected_id(set, id));
    };

    // Keyboard shortcut: Ctrl+A selects/toggles all visible history rows; Escape closes details or clears selection
    let keydown_handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if active_tab.get_untracked() != AppTab::History {
            return;
        }
        if is_escape_shortcut(&ev) && show_details_modal.get_untracked().is_some() {
            show_details_modal.set(None);
            return;
        }
        if show_details_modal.get_untracked().is_some()
            || show_compare_modal.get_untracked().is_some()
            || show_group_modal.get_untracked().is_some()
            || show_confirm_modal.get_untracked().is_some()
            || should_ignore_shortcut(&ev)
        {
            return;
        }
        if is_select_all_shortcut(&ev) {
            ev.prevent_default();
            on_toggle_select_all();
        } else if is_escape_shortcut(&ev) && !selected_ids.with_untracked(|s| s.is_empty()) {
            selected_ids.set(HashSet::new());
        }
    });
    on_cleanup(move || {
        drop(keydown_handle);
    });

    let on_create_methodology = move |_| {
        let ids: Vec<i64> = selected_ids.get_untracked().into_iter().collect();
        if ids.len() < 2 {
            return;
        }

        leptos::task::spawn_local(async move {
            match tauri_bridge::create_methodology(&ids).await {
                Ok(_) => {
                    fetch_history();
                    selected_ids.set(HashSet::new());
                }
                Err(err) => status_toast.set(Some((err, true))),
            }
        });
    };

    let on_compare_runs = move |_| {
        let mut ids: Vec<i64> = selected_ids.get_untracked().into_iter().collect();
        if ids.len() != 2 {
            return;
        }
        ids.sort_unstable();
        let id_a = ids[0];
        let id_b = ids[1];

        leptos::task::spawn_local(async move {
            match tauri_bridge::compare_runs(id_a, id_b).await {
                Ok(Some(diff)) => show_compare_modal.set(Some(diff)),
                Ok(None) => {}
                Err(err) => status_toast.set(Some((err, true))),
            }
        });
    };

    let on_export_csv = move |_| {
        spawn_toast_listener(status_toast, tauri_bridge::export_history_csv());
    };

    let on_export_png = move |_| {
        let ids: Vec<i64> = selected_ids.get_untracked().into_iter().collect();
        if !ids.is_empty() {
            spawn_toast_listener(status_toast, async move {
                tauri_bridge::export_comparison_png(&ids).await
            });
        }
    };

    let on_group_click = move |_| {
        let ids: Vec<i64> = selected_ids.get_untracked().into_iter().collect();
        if !ids.is_empty() {
            show_group_modal.set(Some(ids));
        }
    };

    let on_delete_selected = move |_| {
        let ids: Vec<i64> = selected_ids.get_untracked().into_iter().collect();
        if ids.is_empty() {
            return;
        }

        leptos::task::spawn_local(async move {
            for id in ids {
                if let Err(err) = tauri_bridge::delete_history(id).await {
                    status_toast.set(Some((err, true)));
                    fetch_history();
                    return;
                }
            }
            fetch_history();
            selected_ids.set(HashSet::new());
            let l = lang.get_untracked();
            status_toast.set(Some((t(l, "selected_deleted_msg").to_string(), false)));
        });
    };

    let on_clear_all_history = move |_| {
        let l = lang.get_untracked();
        show_confirm_modal.set(Some((
            t(l, "clear_history_confirm_title").to_string(),
            t(l, "clear_history_confirm_msg").to_string(),
        )));
    };

    view! {
        <div class="app-content">
            <div class="view-header">
                <div class="view-title">
                    <h1>{move || t(lang.get(), "history_title")}</h1>
                    <p>{move || t(lang.get(), "history_subtitle")}</p>
                </div>

                <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
                    <button class="btn btn-secondary btn-sm" on:click=on_export_csv>
                        <Icon kind=IconKind::FileText size=14 />
                        <span>{move || t(lang.get(), "action_export_csv")}</span>
                    </button>
                    <button
                        class="btn btn-secondary btn-sm"
                        disabled=move || selected_ids.get().is_empty()
                        on:click=on_export_png
                    >
                        <Icon kind=IconKind::BarChart size=14 />
                        <span>{move || t(lang.get(), "action_export_png")}</span>
                    </button>
                    <button
                        class="btn btn-secondary btn-sm"
                        disabled=move || selected_ids.get().len() < 2
                        on:click=on_create_methodology
                    >
                        <Icon kind=IconKind::Layers size=14 />
                        <span>{move || t(lang.get(), "action_methodology")}</span>
                    </button>
                    <button
                        class="btn btn-primary btn-sm"
                        disabled=move || selected_ids.get().len() != 2
                        on:click=on_compare_runs
                    >
                        <Icon kind=IconKind::Compare size=14 />
                        <span>{move || t(lang.get(), "action_compare")}</span>
                    </button>
                    <button
                        class="btn btn-secondary btn-sm"
                        disabled=move || selected_ids.get().is_empty()
                        on:click=on_group_click
                    >
                        <Icon kind=IconKind::Folder size=14 />
                        <span>{move || t(lang.get(), "action_group")}</span>
                    </button>
                    <button
                        class="btn btn-danger btn-sm"
                        disabled=move || selected_ids.get().is_empty()
                        on:click=on_delete_selected
                    >
                        <Icon kind=IconKind::Trash size=14 />
                        <span>{move || t(lang.get(), "action_delete")}</span>
                    </button>
                    <button
                        class="btn btn-danger btn-sm"
                        on:click=on_clear_all_history
                    >
                        <Icon kind=IconKind::Trash size=14 />
                        <span>{move || t(lang.get(), "action_clear_all")}</span>
                    </button>
                </div>
            </div>

            // Filter and Search Toolbar
            <div class="table-toolbar">
                <div class="filter-row">
                    <input
                        type="text"
                        style="min-width: 240px;"
                        placeholder=move || t(lang.get(), "search_history")
                        prop:value=move || search_query.get()
                        on:input=move |ev| {
                            search_query.set(event_target_value(&ev));
                            fetch_history();
                        }
                    />

                    <LocalizedSelect
                        lang=lang
                        options=HISTORY_CATEGORY_OPTIONS
                        selected_value=category_filter
                        on_change=Callback::new(move |val: String| {
                            category_filter.set(val);
                            fetch_history();
                        })
                    />

                    <LocalizedSelect
                        lang=lang
                        options=HISTORY_GPU_OPTIONS
                        selected_value=gpu_mode_filter
                        on_change=Callback::new(move |val: String| {
                            gpu_mode_filter.set(val);
                            fetch_history();
                        })
                    />

                    <LocalizedSelect
                        lang=lang
                        options=HISTORY_STATUS_OPTIONS
                        selected_value=status_filter
                        on_change=Callback::new(move |val: String| {
                            status_filter.set(val);
                            fetch_history();
                        })
                    />

                    <LocalizedSelect
                        lang=lang
                        options=HISTORY_SORT_OPTIONS
                        selected_value=Signal::derive(move || sort_order_to_value(sort_order.get()).to_string())
                        on_change=Callback::new(move |val: String| {
                            sort_order.set(parse_sort_order(&val));
                            fetch_history();
                        })
                    />
                </div>

                <div style="display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap;">
                    {move || {
                        let count = selected_ids.get().len();
                        (count > 0).then(|| view! {
                            <span class="badge badge-info">
                                {format!("{} {}", count, t(lang.get(), "runs_selected"))}
                            </span>
                        })
                    }}
                    <SelectAllButton
                        lang=lang
                        all_selected=all_visible_selected
                        tooltip_key="history_select_all_tooltip"
                        on_click=Callback::new(move |_| on_toggle_select_all())
                    />
                </div>
            </div>

            // Data Table
            <div class="data-table-container">
                <table class="data-table history-table">
                    <thead>
                        <tr>
                            <th>{move || t(lang.get(), "col_id")}</th>
                            <th>{move || t(lang.get(), "col_benchmark")}</th>
                            <th>{move || t(lang.get(), "col_version")}</th>
                            <th>{move || t(lang.get(), "col_gpu")}</th>
                            <th>{move || t(lang.get(), "col_status")}</th>
                            <th>{move || t(lang.get(), "col_score")}</th>
                            <th>{move || t(lang.get(), "col_duration")}</th>
                            <th>{move || t(lang.get(), "col_cpu_temp")}</th>
                            <th>{move || t(lang.get(), "col_gpu_temp")}</th>
                            <th>{move || t(lang.get(), "col_power")}</th>
                            <th>{move || t(lang.get(), "col_group")}</th>
                            <th>{move || t(lang.get(), "col_date")}</th>
                            <th style="text-align: right;">{move || t(lang.get(), "col_actions")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let list = runs.get();
                            let l = lang.get();
                            let selected = selected_ids.get();
                            let unit_s = t(l, "unit_secs");
                            let unit_c = t(l, "unit_celsius");
                            let unit_w = t(l, "unit_watt");

                            if list.is_empty() {
                                view! {
                                    <tr>
                                        <td colspan="13" style="text-align: center; padding: 2.5rem; color: var(--text-secondary);">
                                            {t(l, "no_history_records")}
                                        </td>
                                    </tr>
                                }.into_any()
                            } else {
                                list.into_iter().map(|run| {
                                    let id = run.id;
                                    let is_checked = selected.contains(&id);
                                    let is_methodology = run.is_methodology;
                                    let group_opt = run.group_name.clone();
                                    let display_name = bench_name(l, &run.benchmark_id, &run.benchmark_id).to_string();
                                    let gpu_display = translate_gpu_mode(l, &run.gpu_mode);
                                    let status_display = translate_status(l, &run.status);
                                    let status_cls = run_status_badge_class(&run.status);

                                    let score_display = run.score
                                        .map(|s| format!("{:.2}", s))
                                        .unwrap_or_else(|| "-".into());

                                    let date_display = format_timestamp_local(run.timestamp);
                                    let version_display = run.preset_or_version.clone();
                                    let duration_secs = run.duration_secs;
                                    let peak_cpu_temp = run.peak_cpu_temp;
                                    let peak_gpu_temp = run.peak_gpu_temp;
                                    let avg_power_w = run.avg_power_w;

                                    let run_for_details = run;
                                    let on_show_details = move |ev: web_sys::MouseEvent| {
                                        ev.stop_propagation();
                                        show_details_modal.set(Some(run_for_details.clone()));
                                    };

                                    view! {
                                        <tr
                                            class="history-row"
                                            class:selected=is_checked
                                            on:click=move |_| on_toggle_row(id)
                                        >
                                            <td>
                                                <div style="display: flex; align-items: center; gap: 0.4rem; white-space: nowrap;">
                                                    <span>{format!("#{}", id)}</span>
                                                    {is_checked.then(|| view! {
                                                        <span
                                                            class="badge badge-selected"
                                                            title=t(l, "history_row_selected_badge")
                                                        >
                                                            <Icon kind=IconKind::Check size=11 />
                                                            <span>{t(l, "history_row_selected_badge")}</span>
                                                        </span>
                                                    })}
                                                </div>
                                            </td>
                                            <td>
                                                <div style="display: flex; align-items: center; gap: 0.4rem;">
                                                    <strong>{display_name}</strong>
                                                    {if is_methodology {
                                                        view! {
                                                            <span class="badge badge-warning">
                                                                {t(l, "methodology_badge")}
                                                            </span>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                </div>
                                            </td>
                                            <td>{version_display}</td>
                                            <td>
                                                <span class="badge badge-info">{gpu_display}</span>
                                            </td>
                                            <td>
                                                <span class=status_cls>{status_display}</span>
                                            </td>
                                            <td style="font-weight: 700; color: var(--accent-color);">
                                                {score_display}
                                            </td>
                                            <td>{format!("{:.1} {}", duration_secs, unit_s)}</td>
                                            <td>{format!("{:.1} {}", peak_cpu_temp, unit_c)}</td>
                                            <td>{format!("{:.1} {}", peak_gpu_temp, unit_c)}</td>
                                            <td>{format!("{:.1} {}", avg_power_w, unit_w)}</td>
                                            <td>
                                                {group_opt.map(|grp| view! {
                                                    <span class="badge badge-info">{grp}</span>
                                                })}
                                            </td>
                                            <td style="font-size: 0.8rem; color: var(--text-secondary);">{date_display}</td>
                                            <td style="text-align: right;">
                                                <button
                                                    class="btn btn-secondary btn-sm"
                                                    title=move || t(lang.get(), "details_run_tooltip")
                                                    on:click=on_show_details
                                                >
                                                    <Icon kind=IconKind::Eye size=14 />
                                                    <span>{move || t(lang.get(), "details_btn")}</span>
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }
                        }}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_selected_id_adds_and_removes() {
        let mut set = HashSet::new();
        toggle_selected_id(&mut set, 42);
        assert!(set.contains(&42));
        toggle_selected_id(&mut set, 42);
        assert!(!set.contains(&42));
    }

    #[test]
    fn test_toggle_select_all_ids_selects_and_clears_visible_subset() {
        let mut set = HashSet::from([99]);
        toggle_select_all_ids(&mut set, &[1, 2, 3]);
        assert!(set.contains(&1) && set.contains(&2) && set.contains(&3));
        assert!(set.contains(&99));

        toggle_select_all_ids(&mut set, &[1, 2, 3]);
        assert!(!set.contains(&1) && !set.contains(&2) && !set.contains(&3));
        assert!(set.contains(&99));
    }

    #[test]
    fn test_retain_existing_selected_ids_prunes_missing_runs() {
        let mut set = HashSet::from([1, 2, 3]);
        let loaded = vec![
            RunResult {
                id: 1,
                ..Default::default()
            },
            RunResult {
                id: 3,
                ..Default::default()
            },
        ];
        retain_existing_selected_ids(&mut set, &loaded);
        assert_eq!(set, HashSet::from([1, 3]));
    }
}
