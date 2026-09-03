use crate::models::RunResult;
use chrono::{DateTime, Utc};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{fontdb, Options, Tree};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

type ValFn<'a> = Box<dyn Fn(&RunResult) -> (String, bool, bool) + 'a>;

/// Generates an SVG string representation of the comparison matrix for N runs
pub fn generate_comparison_svg(runs: &[RunResult]) -> String {
    let num_runs = runs.len().max(1);
    let label_width = 220;
    let col_width = 220;
    let padding_x = 24;
    let content_width = label_width + (num_runs * col_width);
    let total_width = content_width + (padding_x * 2);

    let row_height = 36;
    let section_gap = 18;
    let section_header_height = 26;

    // Calculate dynamic height
    let header_height = 80;
    let test_overview_height = 96;

    let cpu_rows = 5;
    let gpu_rows = 6;
    let mem_rows = 2;
    let general_rows = 5;

    let total_rows = cpu_rows + gpu_rows + mem_rows + general_rows;
    let total_sections = 4;
    let footer_height = 40;

    let total_height = header_height
        + test_overview_height
        + section_gap
        + (total_sections * section_header_height)
        + (total_rows * (row_height + 4))
        + (total_sections * (section_gap - 4))
        + footer_height;

    let mut svg = String::with_capacity(16 * 1024);

    // SVG Header with embedded styles and modern dark theme
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_width}" height="{total_height}" viewBox="0 0 {total_width} {total_height}">
<style>
    .bg {{ fill: #0b0f19; }}
    .card-bg {{ fill: #111827; stroke: #1f293d; stroke-width: 1; rx: 8; }}
    .row-bg {{ fill: #131d31; stroke: #1e293b; stroke-width: 1; rx: 6; }}
    .header-title {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 20px; font-weight: 800; fill: #f8fafc; }}
    .header-subtitle {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 11px; fill: #94a3b8; }}
    .badge-bg {{ fill: #1e293b; stroke: #38bdf8; stroke-width: 1; rx: 4; }}
    .badge-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 11px; font-weight: 700; fill: #38bdf8; }}
    .section-title {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 13px; font-weight: 700; }}
    .label-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 12px; fill: #94a3b8; }}
    .val-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 12px; font-weight: 700; text-anchor: middle; }}
    .score-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 15px; font-weight: 800; fill: #10b981; text-anchor: middle; }}
    .title-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 13px; font-weight: 700; fill: #38bdf8; text-anchor: middle; }}
    .sub-text {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, sans-serif; font-size: 10px; fill: #64748b; text-anchor: middle; }}
    .divider {{ stroke: #1f293d; stroke-width: 1; }}
    .v-divider {{ stroke: #1e293b; stroke-width: 1; }}
    .throttle-border {{ stroke: #ef4444; stroke-width: 1.5; fill: #1e1520; rx: 4; }}
    .power-border {{ stroke: #f59e0b; stroke-width: 1.5; fill: #1e1c15; rx: 4; }}
</style>
<rect width="{total_width}" height="{total_height}" class="bg" />
"##
    ));

    // Top Header Banner
    let now_str = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let header_title = crate::i18n::t("export_svg_header_title");
    let header_subtitle = crate::i18n::t("export_svg_header_subtitle").replace("{}", &now_str);
    let tests_compared_text = format!("{} {}", runs.len(), crate::i18n::t("tests_compared"));
    let peak_pfx = crate::i18n::t("peak_prefix")
        .trim_end_matches(':')
        .to_string();
    let is_en = crate::i18n::get_language() == "en";

    let badge_width = (tests_compared_text.len() * 8 + 24).max(170);
    let badge_x = content_width.saturating_sub(badge_width);

    svg.push_str(&format!(
        r##"<g transform="translate({padding_x}, 24)">
    <text x="0" y="20" class="header-title">{}</text>
    <text x="0" y="38" class="header-subtitle">{}</text>
    <g transform="translate({badge_x}, 4)">
        <rect x="0" y="0" width="{badge_width}" height="26" class="badge-bg" />
        <text x="{}" y="17" class="badge-text" text-anchor="middle">{}</text>
    </g>
</g>
"##,
        escape_xml(&header_title),
        escape_xml(&header_subtitle),
        badge_width / 2,
        escape_xml(&tests_compared_text)
    ));

    let mut current_y = header_height;

    // Test Overview Top Card
    let test_params_title = crate::i18n::t("test_parameters");
    svg.push_str(&format!(
        r##"<g transform="translate({padding_x}, {current_y})">
    <rect width="{content_width}" height="{test_overview_height}" class="card-bg" />
    <text x="16" y="52" class="header-title" style="font-size: 14px; fill: #38bdf8;">{}</text>
"##,
        escape_xml(&test_params_title)
    ));

    let score_lbl = crate::i18n::t("export_svg_score_prefix");
    for (idx, r) in runs.iter().enumerate() {
        let col_x = label_width + (idx * col_width);
        let center_x = col_x + (col_width / 2);

        // Vertical divider
        svg.push_str(&format!(
            r##"    <line x1="{col_x}" y1="0" x2="{col_x}" y2="{test_overview_height}" class="v-divider" />
"##
        ));

        let date_str = DateTime::from_timestamp(r.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let score_str = r
            .score
            .map(|s| format!("{:.1}", s))
            .unwrap_or_else(|| "-".to_string());
        let sub_str = format!(
            "{} • {}",
            escape_xml(&r.preset_or_version),
            escape_xml(&crate::i18n::localize_gpu_mode(&r.gpu_mode))
        );

        svg.push_str(&format!(
            r##"    <text x="{center_x}" y="24" class="title-text">[{}] {}</text>
    <text x="{center_x}" y="40" class="sub-text">{}</text>
    <text x="{center_x}" y="62" class="score-text"><tspan fill="#94a3b8" font-size="12px" font-weight="600">{}: </tspan>{}</text>
    <text x="{center_x}" y="80" class="sub-text">{}</text>
"##,
            idx + 1,
            escape_xml(&r.benchmark_id),
            sub_str,
            escape_xml(&score_lbl),
            score_str,
            date_str
        ));
    }
    svg.push_str("</g>\n");
    current_y += test_overview_height + section_gap;

    // Helper closure to render a section
    let mut render_section = |title: &str, color: &str, rows: &[(String, &str, ValFn)]| {
        svg.push_str(&format!(
            r##"<g transform="translate({padding_x}, {current_y})">
    <text x="4" y="16" class="section-title" fill="{color}">{}</text>
</g>
"##,
            escape_xml(title)
        ));
        current_y += section_header_height;

        for (row_name, val_color, val_fn) in rows {
            svg.push_str(&format!(
                r##"<g transform="translate({padding_x}, {current_y})">
    <rect width="{content_width}" height="{row_height}" class="row-bg" />
    <text x="14" y="23" class="label-text">{}</text>
"##,
                escape_xml(row_name)
            ));

            for (idx, r) in runs.iter().enumerate() {
                let col_x = label_width + (idx * col_width);
                let center_x = col_x + (col_width / 2);
                let (val_str, is_danger, is_warning) = val_fn(r);

                // Vertical separator
                svg.push_str(&format!(
                        r##"    <line x1="{col_x}" y1="0" x2="{col_x}" y2="{row_height}" class="v-divider" />
"##
                    ));

                if is_danger {
                    svg.push_str(&format!(
                        r##"    <rect x="{}" y="4" width="{}" height="{}" class="throttle-border" />
"##,
                        col_x + 6,
                        col_width - 12,
                        row_height - 8
                    ));
                } else if is_warning {
                    svg.push_str(&format!(
                        r##"    <rect x="{}" y="4" width="{}" height="{}" class="power-border" />
"##,
                        col_x + 6,
                        col_width - 12,
                        row_height - 8
                    ));
                }

                let fill = if is_danger {
                    "#ef4444"
                } else if is_warning {
                    "#f59e0b"
                } else {
                    val_color
                };

                svg.push_str(&format!(
                    r##"    <text x="{center_x}" y="23" class="val-text" fill="{fill}">{}</text>
"##,
                    escape_xml(&val_str)
                ));
            }

            svg.push_str("</g>\n");
            current_y += row_height + 4;
        }
        current_y += section_gap - 4;
    };

    // 1. CPU Section
    let pfx_cpu = peak_pfx.clone();
    let pfx_cpu2 = peak_pfx.clone();
    let cpu_def: [(String, &str, ValFn); 5] = [
        (
            crate::i18n::t("cpu_usage_row"),
            "#38bdf8",
            Box::new(move |r| {
                let s = if is_en {
                    format!(
                        "{:.0}% ({} {:.0}%)",
                        r.avg_cpu_usage, pfx_cpu, r.peak_cpu_usage
                    )
                } else {
                    format!(
                        "%{:.0} ({} %{:.0})",
                        r.avg_cpu_usage, pfx_cpu, r.peak_cpu_usage
                    )
                };
                (s, false, false)
            }),
        ),
        (
            crate::i18n::t("cpu_temp_row"),
            "#fb923c",
            Box::new(|r| {
                (
                    format!("{:.1}°C / {:.1}°C", r.avg_cpu_temp, r.peak_cpu_temp),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("cpu_freq_row"),
            "#818cf8",
            Box::new(|r| {
                (
                    format!("{} MHz / {} MHz", r.avg_cpu_freq_mhz, r.peak_cpu_freq_mhz),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("cpu_power_row"),
            "#34d399",
            Box::new(move |r| {
                (
                    format!(
                        "{:.1}W ({} {:.1}W)",
                        r.avg_power_w, pfx_cpu2, r.peak_power_w
                    ),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("cpu_throttling_row"),
            "#f8fafc",
            Box::new(|r| {
                let has_t = r.cpu_throttling != "Yok"
                    && r.cpu_throttling != "None"
                    && r.cpu_throttling != "Hayır"
                    && r.cpu_throttling != "-"
                    && !r.cpu_throttling.is_empty();
                let loc_t = crate::i18n::localize_throttling(&r.cpu_throttling);
                let loc_t = if is_en && loc_t.contains("(%") {
                    loc_t.replace("(%", "(").replace(")", "%)")
                } else {
                    loc_t
                };
                (loc_t, has_t, false)
            }),
        ),
    ];
    render_section(&crate::i18n::t("cpu_metrics_group"), "#38bdf8", &cpu_def);

    // 2. GPU Section
    let pfx_gpu = peak_pfx.clone();
    let pfx_gpu2 = peak_pfx.clone();
    let gpu_def: [(String, &str, ValFn); 6] = [
        (
            crate::i18n::t("gpu_usage_row"),
            "#38bdf8",
            Box::new(move |r| {
                let s = if is_en {
                    format!(
                        "{:.0}% ({} {:.0}%)",
                        r.avg_gpu_usage, pfx_gpu, r.peak_gpu_usage
                    )
                } else {
                    format!(
                        "%{:.0} ({} %{:.0})",
                        r.avg_gpu_usage, pfx_gpu, r.peak_gpu_usage
                    )
                };
                (s, false, false)
            }),
        ),
        (
            crate::i18n::t("gpu_temp_row"),
            "#fb923c",
            Box::new(|r| {
                (
                    format!("{:.1}°C / {:.1}°C", r.avg_gpu_temp, r.peak_gpu_temp),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("gpu_freq_row"),
            "#818cf8",
            Box::new(|r| {
                (
                    format!("{} MHz / {} MHz", r.avg_gpu_freq_mhz, r.peak_gpu_freq_mhz),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("vram_freq_row"),
            "#818cf8",
            Box::new(|r| {
                (
                    format!("{} MHz / {} MHz", r.avg_vram_freq_mhz, r.peak_vram_freq_mhz),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("gpu_power_row"),
            "#34d399",
            Box::new(move |r| {
                (
                    format!(
                        "{:.1}W ({} {:.1}W)",
                        r.avg_gpu_power_w, pfx_gpu2, r.peak_gpu_power_w
                    ),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("gpu_throttling_row"),
            "#f8fafc",
            Box::new(|r| {
                let is_power =
                    r.gpu_throttling.contains("Güç") || r.gpu_throttling.contains("Power");
                let has_t = r.gpu_throttling != "Yok"
                    && r.gpu_throttling != "None"
                    && r.gpu_throttling != "Hayır"
                    && r.gpu_throttling != "-"
                    && !r.gpu_throttling.is_empty();
                let loc_t = crate::i18n::localize_throttling(&r.gpu_throttling);
                let loc_t = if is_en && loc_t.contains("(%") {
                    loc_t.replace("(%", "(").replace(")", "%)")
                } else {
                    loc_t
                };
                (loc_t, has_t && !is_power, is_power)
            }),
        ),
    ];
    render_section(&crate::i18n::t("gpu_metrics_group"), "#fbbf24", &gpu_def);

    // 3. Memory Section
    let pfx_mem = peak_pfx.clone();
    let pfx_mem2 = peak_pfx.clone();
    let mem_def: [(String, &str, ValFn); 2] = [
        (
            crate::i18n::t("ram_row"),
            "#38bdf8",
            Box::new(move |r| {
                (
                    format!(
                        "{:.1} GB ({} {:.1} GB)",
                        r.avg_ram_gb, pfx_mem, r.peak_ram_gb
                    ),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("vram_row"),
            "#c084fc",
            Box::new(|r| {
                (
                    format!(
                        "{:.1} GB ({} {:.1} GB)",
                        r.avg_vram_gb, pfx_mem2, r.peak_vram_gb
                    ),
                    false,
                    false,
                )
            }),
        ),
    ];
    render_section(&crate::i18n::t("memory_metrics_group"), "#a855f7", &mem_def);

    // 4. General Section
    let pfx_gen = peak_pfx.clone();
    let gen_def: [(String, &str, ValFn); 5] = [
        (
            crate::i18n::t("duration_row"),
            "#f8fafc",
            Box::new(|r| (format!("{:.1}s", r.duration_secs), false, false)),
        ),
        (
            crate::i18n::t("gpu_mode"),
            "#38bdf8",
            Box::new(|r| (crate::i18n::localize_gpu_mode(&r.gpu_mode), false, false)),
        ),
        (
            crate::i18n::t("power_profile_row"),
            "#38bdf8",
            Box::new(|r| {
                (
                    crate::i18n::localize_power_profile(&r.power_profile),
                    false,
                    false,
                )
            }),
        ),
        (
            crate::i18n::t("ac_power_row"),
            "#34d399",
            Box::new(move |r| {
                let s = if r.avg_ac_power_w > 0.0 {
                    format!(
                        "{:.1}W ({} {:.1}W)",
                        r.avg_ac_power_w, pfx_gen, r.peak_ac_power_w
                    )
                } else {
                    crate::i18n::t("none")
                };
                (s, false, false)
            }),
        ),
        (
            crate::i18n::t("system_info_row"),
            "#94a3b8",
            Box::new(|r| {
                let summary = if r.system_info_summary.chars().count() > 26 {
                    let truncated: String = r.system_info_summary.chars().take(24).collect();
                    format!("{}...", truncated)
                } else if r.system_info_summary.is_empty() {
                    "-".to_string()
                } else {
                    r.system_info_summary.clone()
                };
                (summary, false, false)
            }),
        ),
    ];
    render_section(&crate::i18n::t("other_metrics_group"), "#94a3b8", &gen_def);

    // Footer
    let footer_text = crate::i18n::t("export_svg_footer");
    svg.push_str(&format!(
        r##"<g transform="translate({padding_x}, {current_y})">
    <line x1="0" y1="0" x2="{content_width}" y2="0" class="divider" />
    <text x="{}" y="24" class="sub-text" style="font-size: 11px;">{}</text>
</g>
</svg>
"##,
        content_width / 2,
        escape_xml(&footer_text)
    ));

    svg
}

/// Renders the generated SVG of the runs comparison into a crisp PNG file and saves it
pub fn export_comparison_png(runs: &[RunResult]) -> Result<PathBuf, String> {
    if runs.is_empty() {
        return Err(crate::i18n::t("status_error_no_runs_to_compare"));
    }

    let svg_str = generate_comparison_svg(runs);

    static FONT_DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    let font_db = FONT_DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });

    let opt = Options {
        fontdb: Arc::clone(font_db),
        font_family: "sans-serif".to_string(),
        ..Options::default()
    };

    let tree = Tree::from_str(&svg_str, &opt)
        .map_err(|e| crate::i18n::t("error_svg_parse").replace("{}", &e.to_string()))?;

    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .ok_or_else(|| crate::i18n::t("error_pixmap_alloc"))?;

    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());

    // Determine target output directory (~/Pictures/BenchHub or ~/Resimler/BenchHub or fallback to current dir)
    let pictures_dir = dirs::picture_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Pictures")
    });
    let benchhub_dir = pictures_dir.join("BenchHub");

    if let Err(e) = fs::create_dir_all(&benchhub_dir) {
        return Err(crate::i18n::t("error_target_dir").replace("{}", &e.to_string()));
    }

    let timestamp_str = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let file_path = benchhub_dir.join(format!("benchhub_comparison_{}.png", timestamp_str));

    pixmap
        .save_png(&file_path)
        .map_err(|e| crate::i18n::t("error_png_save").replace("{}", &e.to_string()))?;

    Ok(file_path)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Escapes a CSV cell value following RFC 4180 rules while preventing CSV formula injection.
/// If the value begins with '=', '+', '-', '@', '\t', or '\r', a single quote is prefixed.
pub fn escape_csv_cell(s: &str) -> String {
    let mut val = s.to_string();
    if val.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        val = format!("'{}", val);
    }
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val
    }
}

/// Generates a CSV string representation of a slice of RunResult records with proper RFC 4180 escaping.
pub fn generate_history_csv(runs: &[RunResult]) -> String {
    let mut csv = String::new();
    csv.push_str(&format!("{}\n", crate::i18n::t("csv_header")));

    for run in runs {
        let date_str = DateTime::from_timestamp(run.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "-".to_string());
        let score_str = run.score.map(|s| s.to_string()).unwrap_or_default();

        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{:.1},{:.1},{:.1},{:.1},{:.1},{},{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.2},{:.2},{},{}\n",
            run.id,
            escape_csv_cell(&run.benchmark_id),
            escape_csv_cell(&run.category),
            escape_csv_cell(&run.preset_or_version),
            escape_csv_cell(&run.gpu_mode),
            escape_csv_cell(&run.power_profile),
            score_str,
            escape_csv_cell(&run.status),
            escape_csv_cell(&date_str),
            run.duration_secs,
            run.avg_cpu_usage,
            run.peak_cpu_usage,
            run.avg_cpu_temp,
            run.peak_cpu_temp,
            run.avg_cpu_freq_mhz,
            run.peak_cpu_freq_mhz,
            run.avg_gpu_usage,
            run.peak_gpu_usage,
            run.avg_gpu_temp,
            run.peak_gpu_temp,
            run.avg_power_w,
            run.peak_power_w,
            run.avg_ram_gb,
            run.peak_ram_gb,
            escape_csv_cell(&run.system_info_summary),
            escape_csv_cell(&run.log_path),
        ));
    }
    csv
}

pub fn export_history_csv(runs: &[RunResult], output_path: &std::path::Path) -> anyhow::Result<()> {
    let csv = generate_history_csv(runs);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, csv)?;
    Ok(())
}
