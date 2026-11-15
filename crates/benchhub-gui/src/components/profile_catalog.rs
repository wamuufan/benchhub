//! Shared Profile Catalog Template Component
//!
//! Renders both standard benchmark suites and system stress tests through a single
//! parameterized layout: header with GPU mode selector, toolbar with search/filters
//! and batch queue controls, keyboard shortcuts, and uniform interactive profile cards.

use super::icons::{Icon, IconKind};
use super::nav_header::AppTab;
use super::{
    apply_gpu_mode_change, build_queue_item, default_versions_map, dispatch_clear_queue,
    dispatch_queue_update, dispatch_run_queue, is_escape_shortcut, is_profile_version_installed,
    is_select_all_shortcut, remove_ids_from_queue, should_ignore_shortcut,
    sync_queued_item_version, toggle_queue_selection, LocalizedSelect, SelectAllButton,
    SelectOption,
};
use crate::i18n::{
    bench_desc, bench_name, t, translate_category, translate_gpu_mode_enum, translate_score_unit,
    translate_stress_version, Language,
};
use crate::tauri_bridge;
use benchhub_types::{ActiveTaskInfo, BenchmarkProfile, GpuMode, QueueItem};
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

/// Identifies which profile catalog view mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileCatalogKind {
    Benchmarks,
    StressTests,
}

#[derive(Clone, Copy)]
struct CatalogTemplateSpec {
    tab: AppTab,
    title_key: &'static str,
    subtitle_key: &'static str,
    search_placeholder_key: &'static str,
    empty_msg_key: &'static str,
    select_all_tooltip_key: &'static str,
    no_installed_to_select_key: &'static str,
    install_before_queue_key: &'static str,
    run_btn_key: &'static str,
    run_icon: IconKind,
    show_category_filter: bool,
    show_duration_selector: bool,
    translate_version_labels: bool,
    is_stress: bool,
}

impl ProfileCatalogKind {
    const fn spec(self) -> CatalogTemplateSpec {
        match self {
            Self::Benchmarks => CatalogTemplateSpec {
                tab: AppTab::Benchmarks,
                title_key: "catalog_title",
                subtitle_key: "catalog_subtitle",
                search_placeholder_key: "search_benchmarks",
                empty_msg_key: "no_benchmarks_found",
                select_all_tooltip_key: "select_all_tooltip",
                no_installed_to_select_key: "no_installed_to_select_msg",
                install_before_queue_key: "install_before_queue_msg",
                run_btn_key: "run_btn",
                run_icon: IconKind::Play,
                show_category_filter: true,
                show_duration_selector: false,
                translate_version_labels: false,
                is_stress: false,
            },
            Self::StressTests => CatalogTemplateSpec {
                tab: AppTab::StressTests,
                title_key: "stress_title",
                subtitle_key: "stress_subtitle",
                search_placeholder_key: "search_stress",
                empty_msg_key: "stress_empty_msg",
                select_all_tooltip_key: "stress_select_all_tooltip",
                no_installed_to_select_key: "no_stress_to_select_msg",
                install_before_queue_key: "install_stress_before_queue_msg",
                run_btn_key: "run_stress_btn",
                run_icon: IconKind::Flame,
                show_category_filter: false,
                show_duration_selector: true,
                translate_version_labels: true,
                is_stress: true,
            },
        }
    }
}

const GPU_MODE_OPTIONS: &[(&str, GpuMode)] = &[
    ("nvidia_dgpu", GpuMode::NvidiaDgpu),
    ("integrated", GpuMode::Integrated),
    ("auto", GpuMode::Auto),
];

const CATEGORY_FILTER_OPTIONS: &[SelectOption] = &[
    SelectOption::key("all", "all_categories"),
    SelectOption::key("cpu", "category_cpu"),
    SelectOption::key("gpu", "category_gpu"),
    SelectOption::key("render", "category_render"),
    SelectOption::key("storage", "category_disk"),
    SelectOption::key("memory", "category_memory"),
    SelectOption::key("ai", "category_ai"),
    SelectOption::key("system", "category_system"),
];

const STRESS_DURATION_OPTIONS: &[SelectOption] = &[
    SelectOption::key("60", "stress_duration_1m"),
    SelectOption::key("300", "stress_duration_5m"),
    SelectOption::key("900", "stress_duration_15m"),
    SelectOption::key("1800", "stress_duration_30m"),
    SelectOption::key("3600", "stress_duration_60m"),
    SelectOption::key("0", "stress_duration_unlimited"),
];

fn matches_category_filter(profile_category: &str, filter: &str) -> bool {
    let filter_lower = filter.trim().to_lowercase();
    if filter_lower.is_empty() || filter_lower == "all" {
        return true;
    }
    let cat_lower = profile_category.to_lowercase();
    match filter_lower.as_str() {
        "storage" | "disk" => cat_lower.contains("storage") || cat_lower.contains("disk"),
        _ => cat_lower.contains(&filter_lower),
    }
}

fn filtered_profiles(
    list: &[BenchmarkProfile],
    kind: ProfileCatalogKind,
    raw_query: &str,
    category_filter: &str,
    lang: Language,
) -> Vec<BenchmarkProfile> {
    let spec = kind.spec();
    let query = raw_query.to_lowercase();
    list.iter()
        .filter(|p| {
            if p.is_stress_test() != spec.is_stress {
                return false;
            }
            if spec.show_category_filter && !matches_category_filter(&p.category, category_filter) {
                return false;
            }
            if query.is_empty() {
                return true;
            }

            let fallback_desc = p.description.as_deref().unwrap_or_default();
            let loc_name = bench_name(lang, &p.id, &p.name).to_lowercase();
            let loc_desc = bench_desc(lang, &p.id, fallback_desc).to_lowercase();
            let loc_cat = translate_category(lang, &p.category).to_lowercase();

            p.name.to_lowercase().contains(&query)
                || loc_name.contains(&query)
                || p.id.to_lowercase().contains(&query)
                || p.category.to_lowercase().contains(&query)
                || loc_cat.contains(&query)
                || fallback_desc.to_lowercase().contains(&query)
                || loc_desc.contains(&query)
        })
        .cloned()
        .collect()
}

fn parse_duration_override(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

fn sync_stress_queue_durations(
    queue: &mut [QueueItem],
    stress_ids: &HashSet<String>,
    duration_override: Option<u64>,
) -> bool {
    let mut changed = false;
    for item in queue.iter_mut() {
        if stress_ids.contains(&item.id) && item.duration_secs_override != duration_override {
            item.duration_secs_override = duration_override;
            changed = true;
        }
    }
    changed
}

fn collect_visible_installed_ids(
    filtered: &[BenchmarkProfile],
    versions: &HashMap<String, String>,
    by_ver: &HashMap<String, bool>,
    by_id: &HashMap<String, bool>,
) -> Vec<String> {
    filtered
        .iter()
        .filter(|p| {
            let ver = versions.get(&p.id).cloned().unwrap_or_default();
            is_profile_version_installed(&p.id, &ver, by_ver, by_id)
        })
        .map(|p| p.id.clone())
        .collect()
}

#[component]
fn GpuModeSelector(
    lang: RwSignal<Language>,
    selected_gpu_mode: RwSignal<GpuMode>,
    queue_items: RwSignal<Vec<QueueItem>>,
    queue_sync_seq: RwSignal<u64>,
    is_queue_running: RwSignal<bool>,
    status_toast: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    view! {
        <div class="inline-control">
            <label class="inline-control-label">
                {move || t(lang.get(), "gpu_mode_label")}
            </label>
            <select
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    apply_gpu_mode_change(
                        &val,
                        selected_gpu_mode,
                        queue_items,
                        queue_sync_seq,
                        is_queue_running,
                        status_toast,
                    );
                }
            >
                {GPU_MODE_OPTIONS
                    .iter()
                    .map(|&(val, mode)| {
                        view! {
                            <option
                                value=val
                                selected=move || selected_gpu_mode.get() == mode
                            >
                                {move || translate_gpu_mode_enum(lang.get(), mode)}
                            </option>
                        }
                    })
                    .collect_view()}
            </select>
        </div>
    }
}

#[component]
fn ProfileFilterControls(
    spec: CatalogTemplateSpec,
    lang: RwSignal<Language>,
    search_query: RwSignal<String>,
    selected_category: RwSignal<String>,
    selected_duration: RwSignal<String>,
    on_duration_change: Callback<String>,
) -> impl IntoView {
    view! {
        <div class="filter-row">
            <input
                type="text"
                class="catalog-search-input"
                placeholder=move || t(lang.get(), spec.search_placeholder_key)
                prop:value=move || search_query.get()
                on:input=move |ev| search_query.set(event_target_value(&ev))
            />

            {spec.show_category_filter.then(|| view! {
                <LocalizedSelect
                    lang=lang
                    options=CATEGORY_FILTER_OPTIONS
                    selected_value=selected_category
                    on_change=Callback::new(move |val| selected_category.set(val))
                />
            })}

            {spec.show_duration_selector.then(|| view! {
                <div class="inline-control">
                    <label class="inline-control-label">
                        {move || t(lang.get(), "stress_duration_label")}
                    </label>
                    <LocalizedSelect
                        lang=lang
                        options=STRESS_DURATION_OPTIONS
                        selected_value=selected_duration
                        on_change=on_duration_change
                    />
                </div>
            })}
        </div>
    }
}

#[component]
fn ProfileQueueActions(
    spec: CatalogTemplateSpec,
    lang: RwSignal<Language>,
    active_run: RwSignal<Option<ActiveTaskInfo>>,
    queue_items: RwSignal<Vec<QueueItem>>,
    is_queue_running: RwSignal<bool>,
    all_visible_installed_selected: Memo<bool>,
    on_toggle_select_all: Callback<()>,
    on_run_queue: Callback<()>,
    on_clear_queue: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="toolbar-actions">
            <SelectAllButton
                lang=lang
                all_selected=all_visible_installed_selected
                tooltip_key=spec.select_all_tooltip_key
                on_click=on_toggle_select_all
                disabled=Signal::derive(move || is_queue_running.get())
            />

            {move || {
                let l = lang.get();
                let count = queue_items.get().len();
                if count > 0 {
                    view! {
                        <span class="badge badge-info">
                            {format!("{} {}", count, t(l, "in_queue_badge"))}
                        </span>
                        <button
                            class="btn btn-primary btn-sm"
                            disabled=move || is_queue_running.get() || active_run.get().is_some()
                            on:click=move |_| on_run_queue.run(())
                        >
                            <Icon kind=IconKind::Play size=13 />
                            <span>
                                {if is_queue_running.get() {
                                    t(l, "running_queue_btn")
                                } else {
                                    t(l, "run_queue_btn")
                                }}
                            </span>
                        </button>
                        <button
                            class="btn btn-secondary btn-sm"
                            disabled=move || is_queue_running.get()
                            on:click=move |_| on_clear_queue.run(())
                        >
                            <Icon kind=IconKind::Trash size=13 />
                            <span>{t(l, "clear_queue_btn")}</span>
                        </button>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn ProfileCard(
    profile: BenchmarkProfile,
    spec: CatalogTemplateSpec,
    lang: RwSignal<Language>,
    active_run: RwSignal<Option<ActiveTaskInfo>>,
    selected_versions: RwSignal<HashMap<String, String>>,
    installed_map: RwSignal<HashMap<String, bool>>,
    installed_versions_map: RwSignal<HashMap<String, bool>>,
    queue_items: RwSignal<Vec<QueueItem>>,
    on_toggle_card_selection: Callback<String>,
    on_version_change: Callback<(String, String)>,
    on_run_click: Callback<String>,
    on_install_click: Callback<String>,
    on_uninstall_click: Callback<String>,
    on_custom_args_click: Callback<String>,
) -> impl IntoView {
    let l = lang.get_untracked();
    let id = profile.id.clone();
    let card_id = StoredValue::new(id.clone());

    let versions = profile.versions.clone();
    let default_ver = profile
        .default_version
        .clone()
        .or_else(|| profile.version.clone())
        .or_else(|| versions.first().map(|v| v.version.clone()))
        .unwrap_or_default();
    let fallback_desc = profile.description.clone().unwrap_or_default();
    let display_name = bench_name(l, &id, &profile.name).to_string();
    let display_desc = bench_desc(l, &id, &fallback_desc).to_string();
    let display_cat = translate_category(l, &profile.category);
    let size_str = profile.display_download_size();
    let score_unit_display = profile
        .score_unit
        .as_deref()
        .map(|u| translate_score_unit(l, u).to_string());

    let is_installed = Memo::new(move |_| {
        let cur_id = card_id.get_value();
        let cur_ver = selected_versions
            .get()
            .get(&cur_id)
            .cloned()
            .unwrap_or_default();
        let by_ver = installed_versions_map.get();
        let by_id = installed_map.get();
        is_profile_version_installed(&cur_id, &cur_ver, &by_ver, &by_id)
    });

    let is_selected = Memo::new(move |_| {
        let cur_id = card_id.get_value();
        queue_items.with(|q| q.iter().any(|item| item.id == cur_id))
    });

    view! {
        <div
            class="catalog-card"
            class:selectable=move || is_installed.get()
            class:selected=move || is_selected.get()
            on:click=move |_| on_toggle_card_selection.run(card_id.get_value())
        >
            <div>
                <div class="card-header-row">
                    <h3>{display_name}</h3>
                    <div class="catalog-card-badges">
                        {move || {
                            if is_selected.get() {
                                view! {
                                    <span class="badge badge-selected">
                                        <Icon kind=IconKind::Check size=11 />
                                        <span>{t(lang.get(), "queued_selected_badge")}</span>
                                    </span>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                        <span class="badge badge-info">{display_cat}</span>
                    </div>
                </div>

                <p class="catalog-card-desc">{display_desc}</p>

                <div class="card-meta">
                    <div><strong>{t(l, "approx_size")}</strong> " " {size_str}</div>
                    {score_unit_display.map(|unit| view! {
                        <div class="catalog-card-meta-row">
                            <strong>{t(l, "score_unit_label")}</strong> " " {unit}
                        </div>
                    })}
                    <div
                        class="catalog-version-row"
                        on:click=move |ev| ev.stop_propagation()
                    >
                        <label><strong>{t(l, "version_label")}</strong></label>
                        <select
                            style="flex: 1;"
                            on:click=move |ev| ev.stop_propagation()
                            on:keydown=move |ev| ev.stop_propagation()
                            on:change=move |ev| {
                                on_version_change.run((card_id.get_value(), event_target_value(&ev)));
                            }
                        >
                            {versions.into_iter().map(|v| {
                                let ver_val = v.version;
                                let is_sel = ver_val == default_ver;
                                let ver_disp = if spec.translate_version_labels {
                                    translate_stress_version(l, &ver_val)
                                } else {
                                    ver_val.clone()
                                };
                                view! {
                                    <option value=ver_val selected=is_sel>{ver_disp}</option>
                                }
                            }).collect_view()}
                        </select>
                    </div>
                </div>
            </div>

            <div
                class="card-actions"
                on:click=move |ev| ev.stop_propagation()
            >
                {move || {
                    let l_curr = lang.get();
                    if is_installed.get() {
                        view! {
                            <button
                                class="btn btn-primary btn-run"
                                disabled=move || active_run.get().is_some()
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    on_run_click.run(card_id.get_value());
                                }
                            >
                                <Icon kind=spec.run_icon size=15 />
                                <span>{t(l_curr, spec.run_btn_key)}</span>
                            </button>
                        }.into_any()
                    } else {
                        view! {
                            <button
                                class="btn btn-primary btn-run"
                                disabled=move || active_run.get().is_some()
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    on_install_click.run(card_id.get_value());
                                }
                            >
                                <Icon kind=IconKind::Download size=15 />
                                <span>{t(l_curr, "install_btn")}</span>
                            </button>
                        }.into_any()
                    }
                }}

                <div class="card-actions-sub">
                    {move || {
                        let l_curr = lang.get();
                        if is_installed.get() {
                            Some(view! {
                                <button
                                    class="btn btn-secondary btn-sm"
                                    disabled=move || active_run.get().is_some()
                                    on:click=move |ev| {
                                        ev.stop_propagation();
                                        on_uninstall_click.run(card_id.get_value());
                                    }
                                    title=move || t(lang.get(), "uninstall_btn")
                                >
                                    <Icon kind=IconKind::Trash size=13 />
                                    <span>{t(l_curr, "uninstall_btn")}</span>
                                </button>
                            })
                        } else {
                            None
                        }
                    }}

                    <button
                        class="btn btn-secondary btn-sm"
                        on:click=move |ev| {
                            ev.stop_propagation();
                            on_custom_args_click.run(card_id.get_value());
                        }
                        title=move || t(lang.get(), "custom_config_btn")
                    >
                        <Icon kind=IconKind::Sliders size=13 />
                        <span>{move || t(lang.get(), "custom_config_btn")}</span>
                    </button>
                </div>
            </div>
        </div>
    }
}

/// Unified catalog view rendering Benchmarks or Stress Tests from one shared template.
#[component]
pub fn ProfileCatalogTab(
    kind: ProfileCatalogKind,
    lang: RwSignal<Language>,
    active_tab: RwSignal<AppTab>,
    active_run: RwSignal<Option<ActiveTaskInfo>>,
    status_toast: RwSignal<Option<(String, bool)>>,
    show_eula_modal: RwSignal<Option<(String, String)>>,
    show_custom_args_modal: RwSignal<Option<(String, String, String)>>,
    custom_args_map: RwSignal<HashMap<String, String>>,
    installed_map: RwSignal<HashMap<String, bool>>,
    installed_versions_map: RwSignal<HashMap<String, bool>>,
    queue_items: RwSignal<Vec<QueueItem>>,
    is_queue_running: RwSignal<bool>,
    selected_gpu_mode: RwSignal<GpuMode>,
) -> impl IntoView {
    let spec = kind.spec();
    let profiles = RwSignal::new(Vec::<BenchmarkProfile>::new());
    let search_query = RwSignal::new(String::new());
    let selected_category = RwSignal::new("all".to_string());
    let initial_duration = queue_items
        .with_untracked(|q| q.iter().find_map(|item| item.duration_secs_override))
        .map(|d| d.to_string())
        .unwrap_or_else(|| "900".to_string());
    let selected_duration = RwSignal::new(initial_duration);
    let selected_versions = RwSignal::new(HashMap::<String, String>::new());
    let queue_sync_seq = RwSignal::new(0u64);

    // Load catalog metadata for the active view subset from the daemon.
    leptos::task::spawn_local(async move {
        match tauri_bridge::get_catalog().await {
            Ok(list) => {
                let subset: Vec<BenchmarkProfile> = list
                    .into_iter()
                    .filter(|p| p.is_stress_test() == spec.is_stress)
                    .collect();
                selected_versions.set(default_versions_map(&subset));
                profiles.set(subset);
            }
            Err(err) => status_toast.set(Some((err, true))),
        }
    });

    let duration_override = move || -> Option<u64> {
        spec.show_duration_selector
            .then(|| parse_duration_override(&selected_duration.get_untracked()))
            .flatten()
    };

    let apply_queue_update = move |new_queue: Vec<QueueItem>| {
        dispatch_queue_update(new_queue, queue_items, queue_sync_seq, status_toast);
    };

    let on_version_change = Callback::new(move |(bench_id, new_ver): (String, String)| {
        selected_versions.update(|map| {
            map.insert(bench_id.clone(), new_ver.clone());
        });
        if !is_queue_running.get_untracked() {
            let by_ver = installed_versions_map.get_untracked();
            let by_id = installed_map.get_untracked();
            let installed = is_profile_version_installed(&bench_id, &new_ver, &by_ver, &by_id);
            let mut updated = queue_items.get_untracked();
            if sync_queued_item_version(&mut updated, &bench_id, &new_ver, installed) {
                apply_queue_update(updated);
            }
        }
    });

    let on_duration_change = Callback::new(move |new_val: String| {
        let parsed_dur = parse_duration_override(&new_val);
        selected_duration.set(new_val);
        if spec.is_stress && !is_queue_running.get_untracked() {
            let stress_ids: HashSet<String> =
                profiles.with_untracked(|list| list.iter().map(|p| p.id.clone()).collect());
            let mut updated = queue_items.get_untracked();
            if sync_stress_queue_durations(&mut updated, &stress_ids, parsed_dur) {
                apply_queue_update(updated);
            }
        }
    });

    let on_install_click = Callback::new(move |bench_id: String| {
        let ver = selected_versions
            .get_untracked()
            .get(&bench_id)
            .cloned()
            .unwrap_or_default();
        show_eula_modal.set(Some((bench_id, ver)));
    });

    let on_uninstall_click = Callback::new(move |bench_id: String| {
        let ver = selected_versions
            .get_untracked()
            .get(&bench_id)
            .cloned()
            .unwrap_or_default();
        leptos::task::spawn_local(async move {
            match tauri_bridge::uninstall_benchmark(&bench_id, &ver).await {
                Ok(()) => {
                    let mut updated = queue_items.get_untracked();
                    let target = HashSet::from([bench_id]);
                    if remove_ids_from_queue(&mut updated, &target) {
                        apply_queue_update(updated);
                    }
                }
                Err(err) => status_toast.set(Some((err, true))),
            }
        });
    });

    let on_custom_args_click = Callback::new(move |bench_id: String| {
        let ver = selected_versions
            .get_untracked()
            .get(&bench_id)
            .cloned()
            .unwrap_or_default();
        let existing = custom_args_map
            .get_untracked()
            .get(&bench_id)
            .cloned()
            .unwrap_or_default();
        show_custom_args_modal.set(Some((bench_id, ver, existing)));
    });

    let on_toggle_card_selection = Callback::new(move |bench_id: String| {
        if is_queue_running.get_untracked() {
            return;
        }
        let mut updated = queue_items.get_untracked();
        if updated.iter().any(|item| item.id == bench_id) {
            updated.retain(|item| item.id != bench_id);
            apply_queue_update(updated);
            return;
        }

        let versions = selected_versions.get_untracked();
        let cur_ver = versions.get(&bench_id).cloned().unwrap_or_default();
        let by_ver = installed_versions_map.get_untracked();
        let by_id = installed_map.get_untracked();
        if !is_profile_version_installed(&bench_id, &cur_ver, &by_ver, &by_id) {
            let msg = t(lang.get_untracked(), spec.install_before_queue_key).to_string();
            status_toast.set(Some((msg, false)));
            return;
        }

        let gpu = selected_gpu_mode.get_untracked();
        let custom_args = custom_args_map.get_untracked();
        updated.push(build_queue_item(
            bench_id,
            &versions,
            gpu,
            &custom_args,
            duration_override(),
        ));
        apply_queue_update(updated);
    });

    let all_visible_installed_selected = Memo::new(move |_| {
        let list = profiles.get();
        let query = search_query.get();
        let cat = selected_category.get();
        let l = lang.get();
        let versions = selected_versions.get();
        let by_ver = installed_versions_map.get();
        let by_id = installed_map.get();
        let current_queue = queue_items.get();

        let filtered = filtered_profiles(&list, kind, &query, &cat, l);
        let visible_installed =
            collect_visible_installed_ids(&filtered, &versions, &by_ver, &by_id);
        !visible_installed.is_empty()
            && visible_installed
                .iter()
                .all(|id| current_queue.iter().any(|item| &item.id == id))
    });

    let toggle_select_all = move || {
        if is_queue_running.get_untracked() {
            return;
        }
        let list = profiles.get_untracked();
        let query = search_query.get_untracked();
        let cat = selected_category.get_untracked();
        let l = lang.get_untracked();
        let versions = selected_versions.get_untracked();
        let by_ver = installed_versions_map.get_untracked();
        let by_id = installed_map.get_untracked();

        let filtered = filtered_profiles(&list, kind, &query, &cat, l);
        let visible_installed =
            collect_visible_installed_ids(&filtered, &versions, &by_ver, &by_id);
        if visible_installed.is_empty() {
            status_toast.set(Some((
                t(l, spec.no_installed_to_select_key).to_string(),
                false,
            )));
            return;
        }

        let mut updated = queue_items.get_untracked();
        let gpu = selected_gpu_mode.get_untracked();
        let custom_args = custom_args_map.get_untracked();
        toggle_queue_selection(
            &mut updated,
            &visible_installed,
            &versions,
            gpu,
            &custom_args,
            duration_override(),
        );
        apply_queue_update(updated);
    };

    let on_toggle_select_all_cb = Callback::new(move |_| toggle_select_all());

    let on_run_click = Callback::new(move |bench_id: String| {
        let ver = selected_versions
            .get_untracked()
            .get(&bench_id)
            .cloned()
            .unwrap_or_default();
        let gpu = selected_gpu_mode.get_untracked();
        let custom_args = custom_args_map
            .get_untracked()
            .get(&bench_id)
            .cloned()
            .filter(|s| !s.trim().is_empty());
        let duration = duration_override();

        active_tab.set(AppTab::Monitor);

        leptos::task::spawn_local(async move {
            if let Err(err) =
                tauri_bridge::run_benchmark(&bench_id, &ver, gpu, custom_args, duration).await
            {
                status_toast.set(Some((err, true)));
            }
        });
    });

    let on_run_queue = Callback::new(move |_| {
        if queue_items.get_untracked().is_empty() || is_queue_running.get_untracked() {
            return;
        }
        dispatch_run_queue(active_tab, status_toast);
    });

    let on_clear_queue = Callback::new(move |_| {
        dispatch_clear_queue(queue_items, queue_sync_seq, status_toast);
    });

    // Ctrl+A toggles visible installed items; Escape clears only this tab's queued selections.
    let keydown_handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if active_tab.get_untracked() != spec.tab
            || show_eula_modal.get_untracked().is_some()
            || show_custom_args_modal.get_untracked().is_some()
            || should_ignore_shortcut(&ev)
        {
            return;
        }
        if is_select_all_shortcut(&ev) {
            ev.prevent_default();
            toggle_select_all();
        } else if is_escape_shortcut(&ev) && !is_queue_running.get_untracked() {
            let tab_ids: HashSet<String> =
                profiles.with_untracked(|list| list.iter().map(|p| p.id.clone()).collect());
            let mut updated = queue_items.get_untracked();
            if remove_ids_from_queue(&mut updated, &tab_ids) {
                apply_queue_update(updated);
            }
        }
    });
    on_cleanup(move || drop(keydown_handle));

    view! {
        <div class="app-content">
            <div class="view-header">
                <div class="view-title">
                    <h1>{move || t(lang.get(), spec.title_key)}</h1>
                    <p>{move || t(lang.get(), spec.subtitle_key)}</p>
                </div>

                <GpuModeSelector
                    lang=lang
                    selected_gpu_mode=selected_gpu_mode
                    queue_items=queue_items
                    queue_sync_seq=queue_sync_seq
                    is_queue_running=is_queue_running
                    status_toast=status_toast
                />
            </div>

            <div class="table-toolbar">
                <ProfileFilterControls
                    spec=spec
                    lang=lang
                    search_query=search_query
                    selected_category=selected_category
                    selected_duration=selected_duration
                    on_duration_change=on_duration_change
                />

                <ProfileQueueActions
                    spec=spec
                    lang=lang
                    active_run=active_run
                    queue_items=queue_items
                    is_queue_running=is_queue_running
                    all_visible_installed_selected=all_visible_installed_selected
                    on_toggle_select_all=on_toggle_select_all_cb
                    on_run_queue=on_run_queue
                    on_clear_queue=on_clear_queue
                />
            </div>

            <div class="grid-catalog">
                {move || {
                    let list = profiles.get();
                    let query = search_query.get();
                    let cat = selected_category.get();
                    let l = lang.get();
                    let filtered = filtered_profiles(&list, kind, &query, &cat, l);

                    if filtered.is_empty() {
                        view! {
                            <div class="empty-grid-state">
                                {t(l, spec.empty_msg_key)}
                            </div>
                        }.into_any()
                    } else {
                        filtered
                            .into_iter()
                            .map(|profile| {
                                view! {
                                    <ProfileCard
                                        profile=profile
                                        spec=spec
                                        lang=lang
                                        active_run=active_run
                                        selected_versions=selected_versions
                                        installed_map=installed_map
                                        installed_versions_map=installed_versions_map
                                        queue_items=queue_items
                                        on_toggle_card_selection=on_toggle_card_selection
                                        on_version_change=on_version_change
                                        on_run_click=on_run_click
                                        on_install_click=on_install_click
                                        on_uninstall_click=on_uninstall_click
                                        on_custom_args_click=on_custom_args_click
                                    />
                                }
                            })
                            .collect_view()
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile(
        id: &str,
        name: &str,
        category: &str,
        test_type: Option<&str>,
    ) -> BenchmarkProfile {
        BenchmarkProfile {
            id: id.to_string(),
            name: name.to_string(),
            category: category.to_string(),
            version: Some("1.0".to_string()),
            default_version: Some("1.0".to_string()),
            test_type: test_type.map(str::to_string),
            ..BenchmarkProfile::default()
        }
    }

    #[test]
    fn test_filtered_profiles_separates_benchmarks_and_stress_modes() {
        let profiles = vec![
            sample_profile("7zip", "7-Zip", "CPU", None),
            sample_profile("fio", "FIO", "Storage", None),
            sample_profile("furmark", "FurMark", "GPU Stress", Some("stress")),
        ];

        let all_bench = filtered_profiles(
            &profiles,
            ProfileCatalogKind::Benchmarks,
            "",
            "all",
            Language::En,
        );
        assert_eq!(all_bench.len(), 2);
        assert!(all_bench.iter().all(|p| p.id != "furmark"));

        let disk = filtered_profiles(
            &profiles,
            ProfileCatalogKind::Benchmarks,
            "",
            "disk",
            Language::En,
        );
        assert_eq!(disk.len(), 1);
        assert_eq!(disk[0].id, "fio");

        let stress = filtered_profiles(
            &profiles,
            ProfileCatalogKind::StressTests,
            "",
            "cpu",
            Language::En,
        );
        assert_eq!(stress.len(), 1);
        assert_eq!(stress[0].id, "furmark");

        let stress_search_miss = filtered_profiles(
            &profiles,
            ProfileCatalogKind::StressTests,
            "7zip",
            "all",
            Language::En,
        );
        assert!(stress_search_miss.is_empty());
    }

    #[test]
    fn test_build_queue_item_uses_selected_version_and_trims_empty_args() {
        let mut versions = HashMap::new();
        versions.insert("7zip".to_string(), "24.08".to_string());
        let mut args = HashMap::new();
        args.insert("7zip".to_string(), "   ".to_string());

        let item = build_queue_item(
            "7zip".to_string(),
            &versions,
            GpuMode::NvidiaDgpu,
            &args,
            None,
        );
        assert_eq!(item.id, "7zip");
        assert_eq!(item.version, "24.08");
        assert_eq!(item.gpu_mode, GpuMode::NvidiaDgpu);
        assert_eq!(item.custom_args, None);
    }

    #[test]
    fn test_sync_stress_queue_durations_updates_only_stress_items() {
        let mut queue = vec![
            QueueItem {
                id: "7zip".to_string(),
                version: "24.08".to_string(),
                gpu_mode: GpuMode::Auto,
                custom_args: None,
                duration_secs_override: None,
            },
            QueueItem {
                id: "furmark".to_string(),
                version: "vulkan".to_string(),
                gpu_mode: GpuMode::NvidiaDgpu,
                custom_args: None,
                duration_secs_override: Some(900),
            },
        ];
        let stress_ids = HashSet::from(["furmark".to_string()]);

        assert!(sync_stress_queue_durations(
            &mut queue,
            &stress_ids,
            Some(300)
        ));
        assert_eq!(queue[0].duration_secs_override, None);
        assert_eq!(queue[1].duration_secs_override, Some(300));
        assert!(!sync_stress_queue_durations(
            &mut queue,
            &stress_ids,
            Some(300)
        ));
    }

    #[test]
    fn test_build_stress_queue_item_preserves_duration_override() {
        let mut versions = HashMap::new();
        versions.insert("furmark".to_string(), "vulkan".to_string());
        let args = HashMap::new();

        let timed = build_queue_item(
            "furmark".to_string(),
            &versions,
            GpuMode::NvidiaDgpu,
            &args,
            parse_duration_override("60"),
        );
        assert_eq!(timed.duration_secs_override, Some(60));

        let continuous = build_queue_item(
            "furmark".to_string(),
            &versions,
            GpuMode::NvidiaDgpu,
            &args,
            parse_duration_override("0"),
        );
        assert_eq!(continuous.duration_secs_override, Some(0));
    }
}
