use benchhub::export::{
    escape_csv_cell, export_comparison_png, export_history_csv, generate_comparison_svg,
    generate_history_csv,
};
use benchhub::models::RunResult;
use std::fs;
use tempfile::tempdir;

fn create_mock_run(id: i64, benchmark_id: &str, score: f64, category: &str) -> RunResult {
    RunResult {
        id,
        benchmark_id: benchmark_id.to_string(),
        category: category.to_string(),
        preset_or_version: "Standard 1080p".to_string(),
        gpu_mode: "Hybrid".to_string(),
        score: Some(score),
        status: "Tamamlandı".to_string(),
        duration_secs: 65.4,
        avg_cpu_usage: 45.2,
        peak_cpu_usage: 89.0,
        avg_cpu_temp: 68.5,
        peak_cpu_temp: 82.1,
        avg_cpu_freq_mhz: 3800,
        peak_cpu_freq_mhz: 4500,
        avg_gpu_usage: 98.4,
        peak_gpu_usage: 100.0,
        avg_gpu_temp: 72.3,
        peak_gpu_temp: 78.9,
        avg_gpu_freq_mhz: 1850,
        peak_gpu_freq_mhz: 1950,
        avg_vram_freq_mhz: 7000,
        peak_vram_freq_mhz: 7000,
        avg_power_w: 65.0,
        peak_power_w: 95.0,
        avg_gpu_power_w: 120.0,
        peak_gpu_power_w: 140.0,
        avg_ram_gb: 8.5,
        peak_ram_gb: 12.2,
        avg_vram_gb: 4.2,
        peak_vram_gb: 5.8,
        avg_ac_power_w: 180.0,
        peak_ac_power_w: 220.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "AMD Ryzen 7 5800H • RTX 3060".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/test.log".to_string(),
        timestamp: 1724700000,
    }
}

#[test]
fn test_generate_comparison_svg_validity() {
    let run1 = create_mock_run(1, "Unigine Heaven", 3450.0, "GPU");
    let mut run2 = create_mock_run(2, "Unigine Heaven", 3620.0, "GPU");
    run2.cpu_throttling = "Termal Kısma".to_string();
    run2.gpu_throttling = "Güç Limiti".to_string();

    let runs = vec![run1, run2];
    let svg = generate_comparison_svg(&runs);

    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>\n"));
    assert!(svg.contains("BenchHub - Test Karşılaştırma Raporu"));
    assert!(svg.contains("2 Test Karşılaştırılıyor"));
    assert!(svg.contains("Unigine Heaven"));
    assert!(svg.contains("3450.0"));
    assert!(svg.contains("3620.0"));
    assert!(svg.contains("GPU Modu"));
    assert!(svg.contains("Güç Profili"));
    assert!(svg.contains("throttle-border"));
    assert!(svg.contains("power-border"));

    // Test with English language set
    benchhub::i18n::set_language("en");
    let svg_en = generate_comparison_svg(&runs);
    assert!(svg_en.contains("BenchHub - Test Comparison Report"));
    assert!(svg_en.contains("2 Tests Being Compared"));
    assert!(svg_en.contains("GPU Mode"));
    assert!(svg_en.contains("Power Profile"));
    assert!(svg_en.contains("Thermal Throttling"));
    assert!(svg_en.contains("Power Limit"));
    benchhub::i18n::set_language("tr");
}

#[test]
fn test_export_comparison_png_file_creation() {
    let run1 = create_mock_run(1, "7-Zip", 45000.0, "CPU");
    let run2 = create_mock_run(2, "7-Zip", 48500.0, "CPU");
    let run3 = create_mock_run(3, "7-Zip", 49200.0, "CPU");

    let runs = vec![run1, run2, run3];
    let result = export_comparison_png(&runs);

    assert!(result.is_ok(), "PNG export failed: {:?}", result.err());
    let path = result.unwrap();
    assert!(path.exists());

    let bytes = fs::read(&path).expect("Failed to read generated PNG file");
    assert!(bytes.len() > 1000, "PNG file is too small");

    // Check PNG magic header: 0x89 0x50 0x4E 0x47 0x0D 0x0A 0x1A 0x0A
    assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n");

    // Clean up
    let _ = fs::remove_file(path);
}

#[test]
fn test_export_comparison_png_empty_error() {
    let empty_runs: Vec<RunResult> = Vec::new();
    let result = export_comparison_png(&empty_runs);
    assert!(result.is_err());
}

fn create_sample_run(
    id: i64,
    bench: &str,
    category: &str,
    score: Option<f64>,
    sys_info: &str,
) -> RunResult {
    RunResult {
        id,
        benchmark_id: bench.to_string(),
        category: category.to_string(),
        preset_or_version: "Standard, 1080p \"Extreme\"".to_string(),
        gpu_mode: "Harici GPU (NVIDIA)".to_string(),
        score,
        status: "Başarılı".to_string(),
        timestamp: 1700000000,
        duration_secs: 123.456,
        avg_cpu_usage: 85.123,
        peak_cpu_usage: 99.9,
        avg_cpu_temp: 70.45,
        peak_cpu_temp: 82.1,
        avg_cpu_freq_mhz: 3800,
        peak_cpu_freq_mhz: 4350,
        avg_gpu_usage: 98.4,
        peak_gpu_usage: 100.0,
        avg_gpu_temp: 65.2,
        peak_gpu_temp: 72.0,
        avg_gpu_freq_mhz: 1950,
        peak_gpu_freq_mhz: 2200,
        avg_vram_freq_mhz: 7000,
        peak_vram_freq_mhz: 7000,
        avg_gpu_power_w: 75.5,
        peak_gpu_power_w: 105.0,
        avg_power_w: 115.4,
        peak_power_w: 145.0,
        avg_ac_power_w: 160.2,
        peak_ac_power_w: 210.0,
        avg_ram_gb: 12.345,
        peak_ram_gb: 15.678,
        avg_vram_gb: 5.4,
        peak_vram_gb: 7.2,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: sys_info.to_string(),
        power_profile: "Performans".to_string(),
        log_path: "/tmp/test,path/with \"quotes\" and\nnewlines.log".to_string(),
    }
}

#[test]
fn test_csv_generation_headers_and_escaping() {
    let run = create_sample_run(
        1,
        "unigine_heaven",
        "Grafik, (GPU)",
        Some(4500.5),
        "AMD Ryzen 7, \"7840HS\"\nLenovo Legion",
    );

    let csv_output = generate_history_csv(&[run]);

    // Check Header line
    assert!(csv_output.starts_with(&format!("{}\n", benchhub::i18n::t("csv_header"))));

    // Check quotes escaping: "Standard, 1080p ""Extreme"""
    assert!(csv_output.contains("\"Standard, 1080p \"\"Extreme\"\"\""));

    // Check comma in category escaping: "Grafik, (GPU)"
    assert!(csv_output.contains("\"Grafik, (GPU)\""));

    // Check multiline / quotes in system info
    assert!(csv_output.contains("\"AMD Ryzen 7, \"\"7840HS\"\"\nLenovo Legion\""));

    // Check multiline / quotes in log path
    assert!(csv_output.contains("\"/tmp/test,path/with \"\"quotes\"\" and\nnewlines.log\""));

    // Check formatted floats
    assert!(csv_output.contains("123.5")); // duration
    assert!(csv_output.contains("85.1")); // cpu usage
    assert!(csv_output.contains("12.35")); // ram gb
    assert!(csv_output.contains("15.68")); // peak ram gb
}

#[test]
fn test_csv_export_file_creation_with_tempfile() {
    let dir = tempdir().expect("Failed to create tempdir");
    let export_path = dir.path().join("exports").join("history.csv");

    let run1 = create_sample_run(10, "7zip", "CPU", Some(92000.0), "Intel Core i7");
    let run2 = create_sample_run(11, "cray", "CPU", None, "AMD Ryzen 7");

    export_history_csv(&[run1, run2], &export_path).expect("CSV export failed");

    assert!(export_path.exists());
    let file_content = std::fs::read_to_string(&export_path).expect("Failed to read exported CSV");
    assert!(file_content.contains("92000"));
    assert!(file_content.contains("7zip"));
    assert!(file_content.contains("cray"));
}

#[test]
fn test_svg_generation_empty_slice() {
    let svg = generate_comparison_svg(&[]);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
}

#[test]
fn test_csv_formula_injection_prevention() {
    // Individual cell escaping tests for dangerous prefix characters: '=', '+', '-', '@', '\t', '\r'
    assert_eq!(escape_csv_cell("=SUM(A1:A10)"), "'=SUM(A1:A10)");
    assert_eq!(escape_csv_cell("+12345"), "'+12345");
    assert_eq!(escape_csv_cell("-2+5"), "'-2+5");
    assert_eq!(escape_csv_cell("@cmd"), "'@cmd");
    assert_eq!(escape_csv_cell("\tcalc"), "'\tcalc");
    assert_eq!(escape_csv_cell("\rcalc"), "\"'\rcalc\"");

    // Combined formula prefix and RFC 4180 quotes/comma escaping
    assert_eq!(escape_csv_cell("=1+1,2"), "\"'=1+1,2\"");
    assert_eq!(escape_csv_cell("+foo\"bar\""), "\"'+foo\"\"bar\"\"\"");

    // Safe strings should not have single quote prepended
    assert_eq!(escape_csv_cell("Geekbench 6"), "Geekbench 6");
    assert_eq!(escape_csv_cell("Standard, 1080p"), "\"Standard, 1080p\"");

    // Full RunResult CSV export test with formula injection attempts in fields
    let mut malicious_run = create_sample_run(
        99,
        "=cmd|' /C calc'!A0",
        "+malicious_cat",
        None,
        "@sys_info",
    );
    malicious_run.status = "-failed".to_string();
    malicious_run.preset_or_version = "\tpreset_tab".to_string();

    let csv = generate_history_csv(&[malicious_run]);

    // Ensure all injected fields are neutralized with single quote
    assert!(csv.contains("'=cmd|' /C calc'!A0") || csv.contains("\"'=cmd|' /C calc'!A0\""));
    assert!(csv.contains("'+malicious_cat"));
    assert!(csv.contains("'@sys_info"));
    assert!(csv.contains("'-failed"));
    assert!(csv.contains("'\tpreset_tab"));
}
