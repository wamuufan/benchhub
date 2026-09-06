use benchhub::models::{BenchmarkProfile, BenchmarkVersion, GpuMode, RunComparisonDiff, RunResult};

#[test]
fn test_models_serialization() {
    let profile = BenchmarkProfile {
        id: "7zip".to_string(),
        name: "7-Zip Compression".to_string(),
        category: "CPU".to_string(),
        download_size: Some("~1.8 MB".to_string()),
        description: Some("Test description".to_string()),
        download_url: None,
        download_cmd: Some("echo download".to_string()),
        run_cmd: "./7zz".to_string(),
        run_args: vec!["b".to_string()],
        score_regex: Some(r"Tot:\s+\d+\s+(\d+)".to_string()),
        versions: vec![BenchmarkVersion {
            version: "23.01".to_string(),
            download_size: Some("~1.8 MB".to_string()),
            download_cmd: Some("echo dl 23.01".to_string()),
            run_cmd: Some("./7zz".to_string()),
            run_args: Some(vec!["b".to_string()]),
            score_regex: None,
            ..Default::default()
        }],
        default_version: Some("23.01".to_string()),
        ..Default::default()
    };

    let serialized = toml::to_string(&profile).expect("Failed to serialize");
    let deserialized: BenchmarkProfile =
        toml::from_str(&serialized).expect("Failed to deserialize");
    assert_eq!(profile, deserialized);
}

#[test]
fn test_benchmark_version_override() {
    let profile = BenchmarkProfile {
        id: "geekbench".to_string(),
        name: "Geekbench".to_string(),
        category: "System".to_string(),
        download_size: Some("~100 MB".to_string()),
        description: None,
        download_url: None,
        download_cmd: Some("echo base".to_string()),
        run_cmd: "./geekbench6".to_string(),
        run_args: vec![],
        score_regex: Some("Score (\\d+)".to_string()),
        versions: vec![BenchmarkVersion {
            version: "5.5.1".to_string(),
            download_size: Some("~80 MB".to_string()),
            download_cmd: Some("echo v5".to_string()),
            run_cmd: Some("./geekbench5".to_string()),
            run_args: Some(vec!["--cpu".to_string()]),
            score_regex: Some("V5 Score (\\d+)".to_string()),
            ..Default::default()
        }],
        default_version: Some("6.2.2".to_string()),
        ..Default::default()
    };

    let v5_profile = profile.for_version("5.5.1");
    assert_eq!(v5_profile.name, "Geekbench (5.5.1)");
    assert_eq!(v5_profile.download_size, Some("~80 MB".to_string()));
    assert_eq!(v5_profile.download_cmd, Some("echo v5".to_string()));
    assert_eq!(v5_profile.run_cmd, "./geekbench5");
    assert_eq!(v5_profile.run_args, vec!["--cpu".to_string()]);
    assert_eq!(v5_profile.score_regex, Some("V5 Score (\\d+)".to_string()));

    let non_existent = profile.for_version("unknown");
    assert_eq!(non_existent.name, "Geekbench");
    assert_eq!(non_existent.run_cmd, "./geekbench6");
}

#[test]
fn test_all_benchmarks_folder_files_valid() {
    let benchmarks_dir = std::path::PathBuf::from("benchmarks");
    let entries = std::fs::read_dir(&benchmarks_dir).expect("benchmarks directory must exist");
    let mut count = 0;
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("toml") {
            let content =
                std::fs::read_to_string(entry.path()).expect("Failed to read benchmark toml");
            let profile: BenchmarkProfile =
                toml::from_str(&content).expect("Failed to parse benchmark toml");
            assert!(!profile.id.is_empty());
            assert!(!profile.name.is_empty());
            assert!(!profile.category.is_empty());
            count += 1;
        }
    }
    assert!(
        count >= 5,
        "Expected at least 5 benchmark profiles (including Unigine)"
    );
}

#[test]
fn test_app_settings_serialization() {
    use benchhub::models::AppSettings;

    let default_settings = AppSettings::default();
    assert_eq!(default_settings.telemetry_interval_ms, 500);
    assert_eq!(default_settings.terminal_buffer_lines, 150);
    assert!(default_settings.save_stopped_runs);
    assert_eq!(default_settings.selected_gpu_mode, "Harici GPU (NVIDIA)");

    let custom = AppSettings {
        telemetry_interval_ms: 250,
        terminal_buffer_lines: 300,
        save_stopped_runs: false,
        selected_gpu_mode: "Dahili GPU (iGPU)".to_string(),
        language: "tr".to_string(),
        max_test_duration_secs: 180,
    };

    let serialized = serde_json::to_string(&custom).expect("Failed to serialize AppSettings");
    let deserialized: AppSettings =
        serde_json::from_str(&serialized).expect("Failed to deserialize AppSettings");
    assert_eq!(custom, deserialized);
    assert!(!deserialized.save_stopped_runs);
    assert_eq!(deserialized.selected_gpu_mode, "Dahili GPU (iGPU)");
}

#[test]
fn test_gpu_mode_serialization_and_display() {
    assert_eq!(
        GpuMode::from_display_str("Harici GPU (NVIDIA)"),
        GpuMode::NvidiaDgpu
    );
    assert_eq!(
        GpuMode::from_display_str("Dahili GPU (iGPU)"),
        GpuMode::Integrated
    );
    assert_eq!(
        GpuMode::from_display_str("Sistem Varsayılanı"),
        GpuMode::Auto
    );

    assert_eq!(GpuMode::NvidiaDgpu.to_display_str(), "Harici GPU (NVIDIA)");
    assert_eq!(GpuMode::Integrated.to_display_str(), "Dahili GPU (iGPU)");
    assert_eq!(GpuMode::Auto.to_display_str(), "Sistem Varsayılanı");
}

#[tokio::test]
async fn test_gpu_mode_environment_injection() {
    use benchhub::engine::inject_gpu_env;
    use tokio::process::Command;

    // 1. NvidiaDgpu Mode
    let mut cmd_nvidia = Command::new("sh");
    inject_gpu_env(&mut cmd_nvidia, GpuMode::NvidiaDgpu);
    let envs: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)> = cmd_nvidia
        .as_std()
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(|s| s.to_os_string())))
        .collect();

    assert!(envs.iter().any(|(k, v)| k == "__NV_PRIME_RENDER_OFFLOAD"
        && v.as_deref() == Some(std::ffi::OsStr::new("1"))));
    assert!(envs.iter().any(|(k, v)| k == "__GLX_VENDOR_LIBRARY_NAME"
        && v.as_deref() == Some(std::ffi::OsStr::new("nvidia"))));
    assert!(envs.iter().any(|(k, v)| k == "__VK_LAYER_NV_optimus"
        && v.as_deref() == Some(std::ffi::OsStr::new("NVIDIA_only"))));
    assert!(envs
        .iter()
        .any(|(k, v)| k == "DRI_PRIME" && v.as_deref() == Some(std::ffi::OsStr::new("1"))));

    // 2. Integrated Mode
    let mut cmd_igpu = Command::new("sh");
    inject_gpu_env(&mut cmd_igpu, GpuMode::Integrated);
    let envs_igpu: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)> = cmd_igpu
        .as_std()
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(|s| s.to_os_string())))
        .collect();

    assert!(envs_igpu
        .iter()
        .any(|(k, v)| k == "__NV_PRIME_RENDER_OFFLOAD"
            && v.as_deref() == Some(std::ffi::OsStr::new("0"))));
    assert!(envs_igpu
        .iter()
        .any(|(k, v)| k == "DRI_PRIME" && v.as_deref() == Some(std::ffi::OsStr::new("0"))));

    // 3. Auto Mode
    let mut cmd_auto = Command::new("sh");
    inject_gpu_env(&mut cmd_auto, GpuMode::Auto);
    let envs_auto: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)> = cmd_auto
        .as_std()
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(|s| s.to_os_string())))
        .collect();
    assert!(!envs_auto
        .iter()
        .any(|(k, _)| k == "__NV_PRIME_RENDER_OFFLOAD"));
}

#[test]
fn test_run_comparison_diff_calculation() {
    let run_a = RunResult {
        id: 1,
        benchmark_id: "unigine_superposition".to_string(),
        category: "GPU".to_string(),
        preset_or_version: "1080p Extreme".to_string(),
        gpu_mode: "Harici GPU (NVIDIA)".to_string(),
        score: Some(10000.0),
        status: "Başarılı".to_string(),
        timestamp: 1724500000,
        duration_secs: 60.0,
        avg_cpu_usage: 40.0,
        peak_cpu_usage: 60.0,
        avg_cpu_temp: 50.0,
        peak_cpu_temp: 60.0,
        avg_cpu_freq_mhz: 3800,
        peak_cpu_freq_mhz: 4200,
        avg_gpu_usage: 95.0,
        peak_gpu_usage: 98.0,
        avg_gpu_temp: 65.0,
        peak_gpu_temp: 75.0,
        avg_gpu_freq_mhz: 1950,
        peak_gpu_freq_mhz: 2100,
        avg_vram_freq_mhz: 7000,
        peak_vram_freq_mhz: 7000,
        avg_gpu_power_w: 80.0,
        peak_gpu_power_w: 95.0,
        avg_power_w: 120.0,
        peak_power_w: 140.0,
        avg_ac_power_w: 215.0,
        peak_ac_power_w: 245.0,
        avg_ram_gb: 8.0,
        peak_ram_gb: 10.0,
        avg_vram_gb: 4.0,
        peak_vram_gb: 6.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "PC Reference".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/a.log".to_string(),
        is_methodology: false,
        methodology_parent_id: None,
    };

    let run_b = RunResult {
        id: 2,
        benchmark_id: "unigine_superposition".to_string(),
        category: "GPU".to_string(),
        preset_or_version: "1080p Extreme".to_string(),
        gpu_mode: "Harici GPU (NVIDIA)".to_string(),
        score: Some(10740.0),
        status: "Başarılı".to_string(),
        timestamp: 1724500100,
        duration_secs: 58.5,
        avg_cpu_usage: 45.0,
        peak_cpu_usage: 70.0,
        avg_cpu_temp: 52.5,
        peak_cpu_temp: 63.0,
        avg_cpu_freq_mhz: 4000,
        peak_cpu_freq_mhz: 4500,
        avg_gpu_usage: 99.0,
        peak_gpu_usage: 100.0,
        avg_gpu_temp: 68.0,
        peak_gpu_temp: 79.0,
        avg_gpu_freq_mhz: 2050,
        peak_gpu_freq_mhz: 2200,
        avg_vram_freq_mhz: 7500,
        peak_vram_freq_mhz: 7500,
        avg_gpu_power_w: 90.0,
        peak_gpu_power_w: 110.0,
        avg_power_w: 130.0,
        peak_power_w: 155.0,
        avg_ac_power_w: 232.0,
        peak_ac_power_w: 277.0,
        avg_ram_gb: 9.5,
        peak_ram_gb: 12.0,
        avg_vram_gb: 4.5,
        peak_vram_gb: 7.0,
        cpu_throttling: "Termal Kısma (%10)".to_string(),
        gpu_throttling: "Güç Limiti (%15)".to_string(),
        system_info_summary: "PC Overclocked".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/b.log".to_string(),
        is_methodology: false,
        methodology_parent_id: None,
    };

    let diff = RunComparisonDiff::calculate(run_a, run_b);

    assert_eq!(diff.run_a.id, 1);
    assert_eq!(diff.run_b.id, 2);
    assert_eq!(diff.score_diff, Some(740.0));
    assert!((diff.score_diff_pct.unwrap() - 7.4).abs() < 0.01);
    assert!((diff.duration_diff_secs - (-1.5)).abs() < 0.01);
    assert!((diff.avg_cpu_usage_diff - 5.0).abs() < 0.01);
    assert!((diff.peak_cpu_usage_diff - 10.0).abs() < 0.01);
    assert!((diff.avg_cpu_temp_diff - 2.5).abs() < 0.01);
    assert!((diff.peak_cpu_temp_diff - 3.0).abs() < 0.01);
    assert_eq!(diff.avg_cpu_freq_diff_mhz, 200);
    assert_eq!(diff.peak_cpu_freq_diff_mhz, 300);
    assert!((diff.avg_gpu_usage_diff - 4.0).abs() < 0.01);
    assert!((diff.avg_gpu_power_diff_w - 10.0).abs() < 0.01);
    assert!((diff.avg_ac_power_diff_w - 17.0).abs() < 0.01);
    assert!((diff.peak_gpu_usage_diff - 2.0).abs() < 0.01);
    assert!((diff.avg_gpu_temp_diff - 3.0).abs() < 0.01);
    assert!((diff.peak_gpu_temp_diff - 4.0).abs() < 0.01);
    assert!((diff.avg_power_diff_w - 10.0).abs() < 0.01);
    assert!((diff.peak_power_diff_w - 15.0).abs() < 0.01);
    assert!((diff.avg_ram_diff_gb - 1.5).abs() < 0.01);
    assert!((diff.peak_ram_diff_gb - 2.0).abs() < 0.01);
    assert!((diff.avg_vram_diff_gb - 0.5).abs() < 0.01);
    assert!((diff.peak_vram_diff_gb - 1.0).abs() < 0.01);
}
