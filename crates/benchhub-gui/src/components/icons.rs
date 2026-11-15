//! Inline Vector SVG Icon Components
//!
//! Provides crisp, theme-aware 24x24 geometric SVG icons inherited via `currentColor`,
//! replacing platform emoji glyphs and raster icons across all UI views.

use leptos::prelude::*;

/// Supported vector icon variants across the BenchHub UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    BrandBolt,
    Timer,
    Flame,
    Activity,
    History,
    Settings,
    Play,
    Stop,
    Download,
    Trash,
    Sliders,
    Terminal,
    ArrowDown,
    Copy,
    FileText,
    BarChart,
    Layers,
    Compare,
    Folder,
    Close,
    Check,
    AlertTriangle,
    Save,
    Eye,
    ExternalLink,
}

macro_rules! svg_icon {
    ($cls:ident, $px:ident, $($body:tt)*) => {
        view! {
            <svg
                class=$cls
                width=$px
                height=$px
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
            >
                $($body)*
            </svg>
        }
        .into_any()
    };
}

/// Renders an inline scalable vector icon synchronized with the surrounding text color.
#[component]
pub fn Icon(
    kind: IconKind,
    #[prop(default = 16)] size: u32,
    #[prop(default = "")] class: &'static str,
) -> impl IntoView {
    let cls = if class.is_empty() {
        "ui-icon".to_string()
    } else {
        format!("ui-icon {class}")
    };
    let px = size;

    match kind {
        IconKind::BrandBolt => svg_icon!(
            cls,
            px,
            <polygon
                points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"
                fill="currentColor"
                stroke="currentColor"
                stroke-width="1.5"
            />
        ),
        IconKind::Timer => svg_icon!(
            cls,
            px,
            <line x1="10" y1="2" x2="14" y2="2" />
            <line x1="12" y1="14" x2="15" y2="11" />
            <circle cx="12" cy="14" r="8" />
        ),
        IconKind::Flame => svg_icon!(
            cls,
            px,
            <path d="M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.072-2.143-.224-4.054 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.153.433-2.294 1-3a2.5 2.5 0 0 0 2.5 2.5z" />
        ),
        IconKind::Activity => svg_icon!(
            cls,
            px,
            <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
        ),
        IconKind::History => svg_icon!(
            cls,
            px,
            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
            <path d="M12 7v5l4 2" />
        ),
        IconKind::Settings => svg_icon!(
            cls,
            px,
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
        ),
        IconKind::Play => svg_icon!(
            cls,
            px,
            <polygon points="6 3 20 12 6 21 6 3" />
        ),
        IconKind::Stop => svg_icon!(
            cls,
            px,
            <rect x="6" y="6" width="12" height="12" rx="2" />
        ),
        IconKind::Download => svg_icon!(
            cls,
            px,
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" y1="15" x2="12" y2="3" />
        ),
        IconKind::Trash => svg_icon!(
            cls,
            px,
            <path d="M3 6h18" />
            <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
            <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
            <line x1="10" y1="11" x2="10" y2="17" />
            <line x1="14" y1="11" x2="14" y2="17" />
        ),
        IconKind::Sliders => svg_icon!(
            cls,
            px,
            <line x1="4" y1="21" x2="4" y2="14" />
            <line x1="4" y1="10" x2="4" y2="3" />
            <line x1="12" y1="21" x2="12" y2="12" />
            <line x1="12" y1="8" x2="12" y2="3" />
            <line x1="20" y1="21" x2="20" y2="16" />
            <line x1="20" y1="12" x2="20" y2="3" />
            <line x1="2" y1="14" x2="6" y2="14" />
            <line x1="10" y1="8" x2="14" y2="8" />
            <line x1="18" y1="16" x2="22" y2="16" />
        ),
        IconKind::Terminal => svg_icon!(
            cls,
            px,
            <rect x="2" y="4" width="20" height="16" rx="2" />
            <polyline points="7 9 10 12 7 15" />
            <line x1="12" y1="15" x2="17" y2="15" />
        ),
        IconKind::ArrowDown => svg_icon!(
            cls,
            px,
            <path d="M12 5v14" />
            <path d="m19 12-7 7-7-7" />
        ),
        IconKind::Copy => svg_icon!(
            cls,
            px,
            <rect width="13" height="13" x="9" y="9" rx="2" ry="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        ),
        IconKind::FileText => svg_icon!(
            cls,
            px,
            <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
            <path d="M14 2v4a2 2 0 0 0 2 2h4" />
            <path d="M10 9H8" />
            <path d="M16 13H8" />
            <path d="M16 17H8" />
        ),
        IconKind::BarChart => svg_icon!(
            cls,
            px,
            <path d="M3 3v18h18" />
            <path d="M18 17V9" />
            <path d="M13 17V5" />
            <path d="M8 17v-3" />
        ),
        IconKind::Layers => svg_icon!(
            cls,
            px,
            <path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z" />
            <path d="m2 12 10 4.5L22 12" />
            <path d="m2 17 10 4.5L22 17" />
        ),
        IconKind::Compare => svg_icon!(
            cls,
            px,
            <path d="m16 3 4 4-4 4" />
            <path d="M20 7H4" />
            <path d="m8 21-4-4 4-4" />
            <path d="M4 17h16" />
        ),
        IconKind::Folder => svg_icon!(
            cls,
            px,
            <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
        ),
        IconKind::Close => svg_icon!(
            cls,
            px,
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
        ),
        IconKind::Check => svg_icon!(
            cls,
            px,
            <polyline points="20 6 9 17 4 12" />
        ),
        IconKind::AlertTriangle => svg_icon!(
            cls,
            px,
            <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
            <path d="M12 9v4" />
            <path d="M12 17h.01" />
        ),
        IconKind::Save => svg_icon!(
            cls,
            px,
            <path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
            <path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7" />
            <path d="M7 3v4a1 1 0 0 0 1 1h7" />
        ),
        IconKind::Eye => svg_icon!(
            cls,
            px,
            <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" />
            <circle cx="12" cy="12" r="3" />
        ),
        IconKind::ExternalLink => svg_icon!(
            cls,
            px,
            <path d="M15 3h6v6" />
            <path d="M10 14 21 3" />
            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
        ),
    }
}
