//! Shared UI Building Blocks
//!
//! Reusable Leptos components and helpers shared across tabs and modal overlays
//! to eliminate duplicate DOM boilerplate while keeping visual styling consistent.

use super::icons::{Icon, IconKind};
use super::{scroll_terminal_to_bottom, setup_terminal_auto_follow, update_terminal_auto_follow};
use crate::i18n::{t, Language};
use benchhub_types::TerminalRingBuffer;
use leptos::prelude::*;

/// Display label source for an option inside [`LocalizedSelect`].
#[derive(Clone, Copy)]
pub enum SelectLabel {
    Key(&'static str),
    Literal(&'static str),
}

/// Option entry for [`LocalizedSelect`].
#[derive(Clone, Copy)]
pub struct SelectOption {
    pub value: &'static str,
    pub label: SelectLabel,
}

impl SelectOption {
    pub const fn key(value: &'static str, i18n_key: &'static str) -> Self {
        Self {
            value,
            label: SelectLabel::Key(i18n_key),
        }
    }

    pub const fn literal(value: &'static str, text: &'static str) -> Self {
        Self {
            value,
            label: SelectLabel::Literal(text),
        }
    }
}

/// Localized `<select>` dropdown bound to a reactive string value.
#[component]
pub fn LocalizedSelect(
    lang: RwSignal<Language>,
    options: &'static [SelectOption],
    #[prop(into)] selected_value: Signal<String>,
    on_change: Callback<String>,
    #[prop(optional)] style: Option<&'static str>,
) -> impl IntoView {
    view! {
        <select
            style=style
            prop:value=move || selected_value.get()
            on:change=move |ev| on_change.run(event_target_value(&ev))
        >
            {options
                .iter()
                .map(|opt| {
                    let val = opt.value;
                    let lbl = opt.label;
                    view! {
                        <option value=val selected=move || selected_value.get() == val>
                            {move || match lbl {
                                SelectLabel::Key(k) => t(lang.get(), k),
                                SelectLabel::Literal(s) => s,
                            }}
                        </option>
                    }
                })
                .collect_view()}
        </select>
    }
}

/// Shared "Select All / Deselect All" toggle button used in catalog and history toolbars.
#[component]
pub fn SelectAllButton(
    lang: RwSignal<Language>,
    all_selected: Memo<bool>,
    tooltip_key: &'static str,
    on_click: Callback<()>,
    #[prop(optional, default = Signal::derive(|| false))] disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <button
            class="btn btn-secondary btn-sm"
            disabled=move || disabled.get()
            on:click=move |_| on_click.run(())
            title=move || t(lang.get(), tooltip_key)
        >
            <Icon kind=IconKind::Check size=13 />
            <span>
                {move || {
                    let l = lang.get();
                    if all_selected.get() {
                        t(l, "deselect_all_btn")
                    } else {
                        t(l, "select_all_btn")
                    }
                }}
            </span>
        </button>
    }
}

/// Reactive badge displaying the current number of buffered terminal lines.
#[component]
pub fn TerminalLinesBadge(
    lang: RwSignal<Language>,
    terminal_buf: RwSignal<TerminalRingBuffer>,
) -> impl IntoView {
    view! {
        <span class="badge badge-info">
            {move || format!("{} {}", terminal_buf.with(|b| b.len()), t(lang.get(), "lines_label"))}
        </span>
    }
}

/// Conditional button that re-enables terminal auto-follow and scrolls to the bottom.
#[component]
pub fn TerminalFollowButton(
    lang: RwSignal<Language>,
    terminal_ref: NodeRef<leptos::html::Div>,
    auto_follow: RwSignal<bool>,
    #[prop(optional, default = 14)] icon_size: u32,
) -> impl IntoView {
    view! {
        {move || {
            if !auto_follow.get() {
                view! {
                    <button
                        class="btn btn-primary btn-sm"
                        on:click=move |_| {
                            auto_follow.set(true);
                            scroll_terminal_to_bottom(terminal_ref);
                        }
                    >
                        <Icon kind=IconKind::ArrowDown size=icon_size />
                        <span>{t(lang.get(), "terminal_follow_btn")}</span>
                    </button>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }
        }}
    }
}

/// Shared scrollable terminal log box with automatic bottom-follow behavior.
#[component]
pub fn LiveTerminalBox(
    lang: RwSignal<Language>,
    terminal_buf: RwSignal<TerminalRingBuffer>,
    terminal_ref: NodeRef<leptos::html::Div>,
    auto_follow: RwSignal<bool>,
    #[prop(optional, default = "terminal-box")] class: &'static str,
    #[prop(optional)] id: Option<&'static str>,
) -> impl IntoView {
    setup_terminal_auto_follow(terminal_ref, terminal_buf, auto_follow);

    view! {
        <div
            class=class
            id=id
            node_ref=terminal_ref
            on:scroll=move |_| {
                update_terminal_auto_follow(terminal_ref, auto_follow);
            }
        >
            {move || {
                let text = terminal_buf.with(|b| b.to_string());
                if text.is_empty() {
                    t(lang.get(), "no_logs_yet").to_string()
                } else {
                    text
                }
            }}
        </div>
    }
}

/// Shared modal overlay shell providing backdrop, header with close button, body, and footer.
#[component]
pub fn ModalShell(
    header: AnyView,
    footer: AnyView,
    on_close: Callback<()>,
    #[prop(into, optional, default = Signal::derive(|| "modal-panel".to_string()))]
    panel_class: Signal<String>,
    #[prop(optional)] panel_style: Option<&'static str>,
    #[prop(optional)] body_style: Option<&'static str>,
    #[prop(optional)] footer_style: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="modal-backdrop">
            <div class=move || panel_class.get() style=panel_style>
                <div class="modal-header">
                    {header}
                    <button class="btn btn-sm" on:click=move |_| on_close.run(())>
                        <Icon kind=IconKind::Close size=15 />
                    </button>
                </div>
                <div class="modal-body" style=body_style>
                    {children()}
                </div>
                <div class="modal-footer" style=footer_style>
                    {footer}
                </div>
            </div>
        </div>
    }
}
