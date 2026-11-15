pub mod history_tab;
pub mod icons;
pub mod modals;
pub mod monitor_tab;
pub mod nav_header;
pub mod profile_catalog;
pub mod settings_tab;
pub mod shared;

pub use history_tab::HistoryTab;
pub use modals::*;
pub use monitor_tab::MonitorTab;
pub use nav_header::NavHeader;
pub use profile_catalog::{ProfileCatalogKind, ProfileCatalogTab};
pub use settings_tab::SettingsTab;
pub use shared::*;

use crate::i18n::{detect_system_language, match_supported_language, Language};
use crate::tauri_bridge;
use benchhub_types::{
    normalize_run_status, BenchmarkProfile, GpuMode, QueueItem, TerminalRingBuffer,
};
use leptos::prelude::*;
use nav_header::AppTab;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::{JsCast, JsValue};

/// Detects the desktop locale asynchronously via Tauri and updates `lang`.
pub fn apply_system_locale_async(lang: RwSignal<Language>) {
    leptos::task::spawn_local(async move {
        if let Ok(locale) = tauri_bridge::get_system_locale().await {
            lang.set(match_supported_language(&locale).unwrap_or(Language::En));
        } else {
            lang.set(detect_system_language());
        }
    });
}

/// Maps a persisted run status string to its corresponding CSS badge class.
pub fn run_status_badge_class(status: &str) -> &'static str {
    match normalize_run_status(status) {
        "completed" => "badge badge-success",
        "stopped" | "cancelled" | "timeout" | "timeout_scored" => "badge badge-warning",
        _ => "badge badge-error",
    }
}

/// Formats elapsed task duration (`MM:SS`) from millisecond epoch timestamps.
pub fn format_elapsed_mmss(now_epoch_ms: i64, started_at_epoch_ms: i64) -> String {
    let secs = ((now_epoch_ms - started_at_epoch_ms).max(0) / 1000) as u64;
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Copies `text` to the browser clipboard and toggles `copied` true for 2 seconds.
pub fn copy_text_with_feedback(text: &str, copied: RwSignal<bool>) {
    if let Some(win) = web_sys::window() {
        let _ = win.navigator().clipboard().write_text(text);
        copied.set(true);
        leptos::task::spawn_local(async move {
            crate::theme::sleep_ms(2000).await;
            copied.set(false);
        });
    }
}

/// Spawns an async background listener/action and surfaces any error via `status_toast`.
pub fn spawn_toast_listener<Fut, T>(status_toast: RwSignal<Option<(String, bool)>>, fut: Fut)
where
    Fut: std::future::Future<Output = Result<T, String>> + 'static,
{
    leptos::task::spawn_local(async move {
        if let Err(err) = fut.await {
            status_toast.set(Some((err, true)));
        }
    });
}

/// Populates a map from profile ID to its default/fallback version string.
pub fn default_versions_map(profiles: &[BenchmarkProfile]) -> HashMap<String, String> {
    let mut ver_map = HashMap::with_capacity(profiles.len());
    for p in profiles {
        let default_ver = p
            .default_version
            .clone()
            .or_else(|| p.version.clone())
            .or_else(|| p.versions.first().map(|v| v.version.clone()))
            .unwrap_or_else(|| "default".to_string());
        ver_map.insert(p.id.clone(), default_ver);
    }
    ver_map
}

/// Checks whether a specific `(id, version)` pair is installed, falling back to `by_id`.
pub fn is_profile_version_installed(
    id: &str,
    cur_ver: &str,
    by_version: &HashMap<String, bool>,
    by_id: &HashMap<String, bool>,
) -> bool {
    let key = format!("{}::{}", id, cur_ver);
    if let Some(&installed) = by_version.get(&key) {
        return installed;
    }
    by_id.get(id).copied().unwrap_or(false)
}

/// Constructs a `QueueItem` from current card version, GPU mode, custom args, and optional duration.
pub fn build_queue_item(
    bench_id: String,
    versions: &HashMap<String, String>,
    gpu_mode: GpuMode,
    custom_args_map: &HashMap<String, String>,
    duration_secs_override: Option<u64>,
) -> QueueItem {
    let version = versions.get(&bench_id).cloned().unwrap_or_default();
    let custom_args = custom_args_map
        .get(&bench_id)
        .cloned()
        .filter(|s| !s.trim().is_empty());
    QueueItem {
        id: bench_id,
        version,
        gpu_mode,
        custom_args,
        duration_secs_override,
    }
}

/// Toggles all `visible_installed` IDs in `queue` while keeping items from other tabs untouched.
pub fn toggle_queue_selection(
    queue: &mut Vec<QueueItem>,
    visible_installed: &[String],
    versions: &HashMap<String, String>,
    gpu_mode: GpuMode,
    custom_args_map: &HashMap<String, String>,
    duration_secs_override: Option<u64>,
) -> bool {
    if visible_installed.is_empty() {
        return false;
    }
    let all_already_selected = visible_installed
        .iter()
        .all(|id| queue.iter().any(|item| &item.id == id));

    if all_already_selected {
        queue.retain(|item| !visible_installed.contains(&item.id));
    } else {
        for id in visible_installed {
            if !queue.iter().any(|item| &item.id == id) {
                queue.push(build_queue_item(
                    id.clone(),
                    versions,
                    gpu_mode,
                    custom_args_map,
                    duration_secs_override,
                ));
            }
        }
    }
    true
}

/// Removes all queue entries whose ID is in `ids`, returning whether `queue` changed.
pub fn remove_ids_from_queue(queue: &mut Vec<QueueItem>, ids: &HashSet<String>) -> bool {
    let before = queue.len();
    queue.retain(|item| !ids.contains(&item.id));
    queue.len() != before
}

/// Updates or removes a queued item when its card version changes; returns whether `queue` changed.
pub fn sync_queued_item_version(
    queue: &mut Vec<QueueItem>,
    bench_id: &str,
    new_ver: &str,
    is_installed: bool,
) -> bool {
    if !is_installed {
        let before = queue.len();
        queue.retain(|item| item.id != bench_id);
        return queue.len() != before;
    }
    let mut changed = false;
    for item in queue.iter_mut() {
        if item.id == bench_id && item.version != new_ver {
            item.version = new_ver.to_string();
            changed = true;
        }
    }
    changed
}

/// Optimistically updates `queue_items` and syncs with the daemon using a latest-wins sequence guard.
pub fn dispatch_queue_update(
    new_queue: Vec<QueueItem>,
    queue_items: RwSignal<Vec<QueueItem>>,
    queue_sync_seq: RwSignal<u64>,
    status_toast: RwSignal<Option<(String, bool)>>,
) {
    let seq = queue_sync_seq.get_untracked().wrapping_add(1);
    queue_sync_seq.set(seq);
    queue_items.set(new_queue.clone());
    leptos::task::spawn_local(async move {
        match tauri_bridge::set_queue(&new_queue).await {
            Ok(confirmed) => {
                if queue_sync_seq.get_untracked() == seq {
                    queue_items.set(confirmed);
                }
            }
            Err(err) => status_toast.set(Some((err, true))),
        }
    });
}

/// Clears the daemon execution queue and updates local UI state with a sequence guard.
pub fn dispatch_clear_queue(
    queue_items: RwSignal<Vec<QueueItem>>,
    queue_sync_seq: RwSignal<u64>,
    status_toast: RwSignal<Option<(String, bool)>>,
) {
    let seq = queue_sync_seq.get_untracked().wrapping_add(1);
    queue_sync_seq.set(seq);
    queue_items.set(Vec::new());
    leptos::task::spawn_local(async move {
        match tauri_bridge::clear_queue().await {
            Ok(updated) => {
                if queue_sync_seq.get_untracked() == seq {
                    queue_items.set(updated);
                }
            }
            Err(err) => status_toast.set(Some((err, true))),
        }
    });
}

/// Switches to Monitor and triggers daemon queue execution.
pub fn dispatch_run_queue(
    active_tab: RwSignal<AppTab>,
    status_toast: RwSignal<Option<(String, bool)>>,
) {
    active_tab.set(AppTab::Monitor);
    leptos::task::spawn_local(async move {
        if let Err(err) = tauri_bridge::run_queue().await {
            status_toast.set(Some((err, true)));
        }
    });
}

/// Parses the GPU selector option value and updates `selected_gpu_mode` plus any queued items.
pub fn apply_gpu_mode_change(
    val: &str,
    selected_gpu_mode: RwSignal<GpuMode>,
    queue_items: RwSignal<Vec<QueueItem>>,
    queue_sync_seq: RwSignal<u64>,
    is_queue_running: RwSignal<bool>,
    status_toast: RwSignal<Option<(String, bool)>>,
) {
    let mode = match val {
        "integrated" => GpuMode::Integrated,
        "auto" => GpuMode::Auto,
        _ => GpuMode::NvidiaDgpu,
    };
    selected_gpu_mode.set(mode);
    if !is_queue_running.get_untracked() {
        let mut updated = queue_items.get_untracked();
        let mut changed = false;
        for item in &mut updated {
            if item.gpu_mode != mode {
                item.gpu_mode = mode;
                changed = true;
            }
        }
        if changed {
            dispatch_queue_update(updated, queue_items, queue_sync_seq, status_toast);
        }
    }
}

/// Formats a Unix epoch timestamp in seconds using the browser/webview local timezone.
pub fn format_timestamp_local(timestamp_secs: i64) -> String {
    if timestamp_secs <= 0 {
        return "-".to_string();
    }
    let dt = js_sys::Date::new(&JsValue::from_f64((timestamp_secs as f64) * 1000.0));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        dt.get_full_year(),
        dt.get_month() + 1,
        dt.get_date(),
        dt.get_hours(),
        dt.get_minutes()
    )
}

/// Returns true when global keyboard shortcuts should be suppressed because
/// the active target is an interactive text input, textarea, select, or contenteditable element.
pub fn should_ignore_shortcut(ev: &web_sys::KeyboardEvent) -> bool {
    let Some(target) = ev.target() else {
        return false;
    };
    if let Ok(el) = target.clone().dyn_into::<web_sys::Element>() {
        let tag = el.tag_name().to_ascii_uppercase();
        if matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
            return true;
        }
    }
    if let Ok(html_el) = target.dyn_into::<web_sys::HtmlElement>() {
        if html_el.is_content_editable() {
            return true;
        }
    }
    false
}

/// Returns true when the keyboard event represents `Ctrl+A` (or `Cmd+A` on macOS).
pub fn is_select_all_shortcut(ev: &web_sys::KeyboardEvent) -> bool {
    (ev.ctrl_key() || ev.meta_key())
        && !ev.alt_key()
        && !ev.shift_key()
        && ev.key().eq_ignore_ascii_case("a")
}

/// Returns true when the keyboard event represents an unmodified `Escape` key press.
pub fn is_escape_shortcut(ev: &web_sys::KeyboardEvent) -> bool {
    !ev.ctrl_key() && !ev.meta_key() && !ev.alt_key() && ev.key() == "Escape"
}

/// Scrolls the referenced terminal container to its bottom edge on the next paint tick.
pub fn scroll_terminal_to_bottom(terminal_ref: NodeRef<leptos::html::Div>) {
    leptos::task::spawn_local(async move {
        crate::theme::sleep_ms(0).await;
        if let Some(el) = terminal_ref.get_untracked() {
            el.set_scroll_top(el.scroll_height());
        }
    });
}

/// Updates `auto_follow` based on whether the user scrolled near the bottom edge.
pub fn update_terminal_auto_follow(
    terminal_ref: NodeRef<leptos::html::Div>,
    auto_follow: RwSignal<bool>,
) {
    if let Some(el) = terminal_ref.get_untracked() {
        let near_bottom = (el.scroll_height() - el.scroll_top() - el.client_height()) <= 40;
        if auto_follow.get_untracked() != near_bottom {
            auto_follow.set(near_bottom);
        }
    }
}

/// Keeps `terminal_ref` pinned to the bottom while `auto_follow` is true, and resets
/// `auto_follow` back to `true` whenever `terminal_buf` is cleared empty.
pub fn setup_terminal_auto_follow(
    terminal_ref: NodeRef<leptos::html::Div>,
    terminal_buf: RwSignal<TerminalRingBuffer>,
    auto_follow: RwSignal<bool>,
) {
    Effect::new(move |_| {
        let is_empty = terminal_buf.with(|b| b.is_empty());
        if is_empty && !auto_follow.get_untracked() {
            auto_follow.set(true);
        }
        if auto_follow.get() {
            scroll_terminal_to_bottom(terminal_ref);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_profile_version_installed_prefers_version_map() {
        let mut by_ver = HashMap::new();
        by_ver.insert("furmark::vulkan".to_string(), true);
        by_ver.insert("furmark::opengl".to_string(), false);
        let mut by_id = HashMap::new();
        by_id.insert("furmark".to_string(), true);
        by_id.insert("7zip".to_string(), false);

        assert!(is_profile_version_installed(
            "furmark", "vulkan", &by_ver, &by_id
        ));
        assert!(!is_profile_version_installed(
            "furmark", "opengl", &by_ver, &by_id
        ));
        assert!(is_profile_version_installed(
            "furmark", "knot_vk", &by_ver, &by_id
        ));
        assert!(!is_profile_version_installed(
            "7zip", "24.08", &by_ver, &by_id
        ));
    }

    #[test]
    fn test_toggle_queue_selection_preserves_other_tabs_items() {
        let mut queue = vec![QueueItem {
            id: "7zip".to_string(),
            version: "24.08".to_string(),
            gpu_mode: GpuMode::NvidiaDgpu,
            custom_args: None,
            duration_secs_override: None,
        }];
        let mut versions = HashMap::new();
        versions.insert("furmark".to_string(), "vulkan".to_string());
        let args = HashMap::new();

        assert!(toggle_queue_selection(
            &mut queue,
            &["furmark".to_string()],
            &versions,
            GpuMode::NvidiaDgpu,
            &args,
            Some(900),
        ));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[1].id, "furmark");
        assert_eq!(queue[1].duration_secs_override, Some(900));

        assert!(toggle_queue_selection(
            &mut queue,
            &["furmark".to_string()],
            &versions,
            GpuMode::NvidiaDgpu,
            &args,
            Some(900),
        ));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "7zip");
    }

    #[test]
    fn test_remove_ids_and_sync_queued_version() {
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
                duration_secs_override: Some(300),
            },
        ];

        assert!(sync_queued_item_version(
            &mut queue, "furmark", "opengl", true
        ));
        assert_eq!(queue[1].version, "opengl");
        assert!(!sync_queued_item_version(
            &mut queue, "furmark", "opengl", true
        ));

        let stress_ids = HashSet::from(["furmark".to_string()]);
        assert!(remove_ids_from_queue(&mut queue, &stress_ids));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "7zip");
    }
}
