use benchhub::engine::BenchmarkEngine;
use benchhub::models::{BenchmarkProfile, TelemetryData};
use benchhub::telemetry::{calculate_power_watts, calculate_telemetry_summary};

#[test]
fn test_power_calculation() {
    // 15,000,000 uJ in 0.5s = 15 J / 0.5s = 30 Watts
    let delta_uj = 15_000_000;
    let elapsed = 0.5;
    let power = calculate_power_watts(delta_uj, elapsed);
    assert!((power - 30.0).abs() < 0.001);
}

#[test]
fn test_sensor_diagnosis_non_empty() {
    use benchhub::telemetry::get_sensor_diagnosis;

    let diag = get_sensor_diagnosis();
    assert!(
        !diag.is_empty(),
        "Sensor diagnosis string should not be empty"
    );
}

#[test]
fn test_live_telemetry_cpu_and_gpu_readings() {
    use benchhub::telemetry::TelemetryEngine;

    let mut engine = TelemetryEngine::new();
    std::thread::sleep(std::time::Duration::from_millis(150));
    let data = engine.read_current();
    println!("Live snapshot: CPU temp={:.1}C, freq={:.0}MHz, usage={:.1}%, power={:.1}W, GPU temp={:.1}C, power={:.1}W, AC power={:.1}W",
        data.cpu_temp, data.cpu_freq, data.cpu_usage, data.power_w, data.gpu_temp, data.gpu_power_w, data.ac_power_w);
    assert!(
        data.cpu_temp > 0.0,
        "CPU temp should be detected on live hardware"
    );
    assert!(
        data.cpu_freq > 0.0,
        "CPU freq should be detected on live hardware"
    );
}

#[tokio::test]
async fn test_unigine_nested_subdirectory_auto_discovery() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_nested_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let runner_dir = tmp_dir
        .join("runners")
        .join("unigine_superposition")
        .join("1.1");
    let nested_dir = runner_dir
        .join("extracted")
        .join("Unigine_Superposition-1.1");
    tokio::fs::create_dir_all(&nested_dir).await.unwrap();

    let binary_path = nested_dir.join("Superposition");
    tokio::fs::write(&binary_path, "#!/bin/sh\necho SCORE: 9999\n")
        .await
        .unwrap();

    let profile = BenchmarkProfile {
        id: "unigine_superposition".to_string(),
        name: "Unigine Superposition".to_string(),
        category: "Grafik (GPU)".to_string(),
        binary_relative_path: Some("extracted/Superposition".to_string()),
        ..Default::default()
    };

    let resolved = BenchmarkEngine::resolve_executable(&runner_dir, &profile)
        .expect("Should auto-discover nested Superposition executable");

    assert_eq!(resolved.work_dir, nested_dir);
    assert_eq!(resolved.executable, "./Superposition");
    assert_eq!(resolved.full_path, binary_path);

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[tokio::test]
async fn test_unigine_sh_extension_auto_discovery() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_sh_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let runner_dir = tmp_dir.join("runners").join("unigine_heaven").join("4.0");
    let extracted_dir = runner_dir.join("extracted");
    tokio::fs::create_dir_all(&extracted_dir).await.unwrap();

    let binary_path = extracted_dir.join("heaven.sh");
    tokio::fs::write(&binary_path, "#!/bin/sh\necho Score: 1234\n")
        .await
        .unwrap();

    let profile = BenchmarkProfile {
        id: "unigine_heaven".to_string(),
        name: "Unigine Heaven".to_string(),
        category: "Grafik (GPU)".to_string(),
        binary_relative_path: Some("extracted/heaven".to_string()),
        ..Default::default()
    };

    let resolved = BenchmarkEngine::resolve_executable(&runner_dir, &profile)
        .expect("Should auto-discover heaven.sh executable");

    assert_eq!(resolved.work_dir, extracted_dir);
    assert_eq!(resolved.executable, "./heaven.sh");
    assert_eq!(resolved.full_path, binary_path);

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[tokio::test]
async fn test_missing_executable_diagnostics_message() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_missing_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let runner_dir = tmp_dir.join("runners").join("dummy").join("1.0");
    let sub_dir = runner_dir.join("extracted").join("data");
    tokio::fs::create_dir_all(&sub_dir).await.unwrap();
    tokio::fs::write(sub_dir.join("textures.bin"), b"some texture data")
        .await
        .unwrap();

    let profile = BenchmarkProfile {
        id: "dummy".to_string(),
        name: "Dummy Benchmark".to_string(),
        category: "Test".to_string(),
        binary_relative_path: Some("extracted/dummy_launcher".to_string()),
        ..Default::default()
    };

    let err = BenchmarkEngine::resolve_executable(&runner_dir, &profile)
        .expect_err("Should return error when executable is missing");

    assert!(err.contains("Çalıştırılabilir dosya bulunamadı!"));
    assert!(err.contains("Mevcut Dizin İçeriği:"));
    assert!(err.contains("extracted/data/textures.bin"));

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[tokio::test]
async fn test_auto_discovery_excludes_run_installer_file() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_exclude_run_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let runner_dir = tmp_dir
        .join("runners")
        .join("unigine_superposition")
        .join("1.1");
    let extracted_dir = runner_dir.join("extracted");
    tokio::fs::create_dir_all(&extracted_dir).await.unwrap();

    // Place .run installer in runner_dir
    let installer_file = runner_dir.join("Unigine_Superposition-1.1.run");
    tokio::fs::write(&installer_file, "#!/bin/sh\necho Makeself installer\n")
        .await
        .unwrap();

    // Place actual extracted binary in extracted/
    let extracted_binary = extracted_dir.join("Superposition");
    tokio::fs::write(&extracted_binary, "#!/bin/sh\necho SCORE: 10500\n")
        .await
        .unwrap();

    let profile = BenchmarkProfile {
        id: "unigine_superposition".to_string(),
        name: "Unigine Superposition".to_string(),
        category: "Grafik (GPU)".to_string(),
        binary_relative_path: Some("extracted/Superposition".to_string()),
        ..Default::default()
    };

    let resolved = BenchmarkEngine::resolve_executable(&runner_dir, &profile)
        .expect("Should resolve extracted binary, NOT installer file");

    assert_eq!(resolved.work_dir, extracted_dir);
    assert_eq!(resolved.executable, "./Superposition");
    assert_eq!(resolved.full_path, extracted_binary);
    assert_ne!(resolved.full_path, installer_file);

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[test]
fn test_system_info_summary_generation() {
    use benchhub::telemetry::get_system_info_summary;
    let sys_info = get_system_info_summary();
    assert!(
        !sys_info.is_empty(),
        "System info summary must not be empty"
    );
    assert!(
        sys_info.contains('|'),
        "System info summary should contain delimiter pipes"
    );
}

#[test]
fn test_telemetry_summary_calculation() {
    use benchhub::models::TelemetryData;
    use benchhub::telemetry::calculate_telemetry_summary;

    let empty_summary = calculate_telemetry_summary(&[]);
    assert_eq!(empty_summary.avg_cpu_usage, 0.0);
    assert_eq!(empty_summary.peak_cpu_temp, 0.0);

    let samples = vec![
        TelemetryData {
            timestamp: 1000,
            cpu_usage: 80.0,
            cpu_temp: 70.0,
            cpu_freq: 4000.0,
            gpu_usage: 90.0,
            gpu_temp: 65.0,
            gpu_freq_mhz: 1900.0,
            vram_freq_mhz: 7000.0,
            power_w: 100.0,
            gpu_power_w: 75.0,
            ac_power_w: 187.0,
            ram_usage_mb: 8192.0,
            vram_usage_mb: 4096.0,
            cpu_throttle: "Yok".to_string(),
            gpu_throttle: "Yok".to_string(),
        },
        TelemetryData {
            timestamp: 2000,
            cpu_usage: 100.0,
            cpu_temp: 80.0,
            cpu_freq: 4200.0,
            gpu_usage: 100.0,
            gpu_temp: 75.0,
            gpu_freq_mhz: 2100.0,
            vram_freq_mhz: 7000.0,
            power_w: 120.0,
            gpu_power_w: 95.0,
            ac_power_w: 227.0,
            ram_usage_mb: 10240.0,
            vram_usage_mb: 6144.0,
            cpu_throttle: "Termal Kısma".to_string(),
            gpu_throttle: "Güç Limiti".to_string(),
        },
    ];

    let summary = calculate_telemetry_summary(&samples);
    assert_eq!(summary.avg_cpu_usage, 90.0);
    assert_eq!(summary.peak_cpu_usage, 100.0);
    assert_eq!(summary.avg_cpu_temp, 75.0);
    assert_eq!(summary.peak_cpu_temp, 80.0);
    assert_eq!(summary.avg_cpu_freq_mhz, 4100);
    assert_eq!(summary.peak_cpu_freq_mhz, 4200);
    assert_eq!(summary.avg_gpu_usage, 95.0);
    assert_eq!(summary.peak_gpu_usage, 100.0);
    assert_eq!(summary.avg_gpu_temp, 70.0);
    assert_eq!(summary.peak_gpu_temp, 75.0);
    assert_eq!(summary.avg_gpu_freq_mhz, 2000);
    assert_eq!(summary.peak_gpu_freq_mhz, 2100);
    assert_eq!(summary.avg_vram_freq_mhz, 7000);
    assert_eq!(summary.peak_vram_freq_mhz, 7000);
    assert_eq!(summary.avg_gpu_power_w, 85.0);
    assert_eq!(summary.peak_gpu_power_w, 95.0);
    assert_eq!(summary.avg_power_w, 110.0);
    assert_eq!(summary.peak_power_w, 120.0);
    assert_eq!(summary.avg_ac_power_w, 207.0);
    assert_eq!(summary.peak_ac_power_w, 227.0);
    assert!((summary.avg_ram_gb - 9.0).abs() < 0.01);
    assert!((summary.peak_ram_gb - 10.0).abs() < 0.01);
    assert!((summary.avg_vram_gb - 5.0).abs() < 0.01);
    assert!((summary.peak_vram_gb - 6.0).abs() < 0.01);
    assert_eq!(summary.cpu_throttling, "Termal Kısma (%50)");
    assert_eq!(summary.gpu_throttling, "Güç Limiti (%50)");
}

#[test]
fn test_get_power_profile_detection() {
    use benchhub::telemetry::get_power_profile;

    let profile = get_power_profile();
    assert!(!profile.is_empty(), "Power profile should not be empty");
    println!("Detected power profile: {}", profile);
}

#[test]
fn test_power_calculation_edge_cases() {
    // Zero delta
    assert_eq!(calculate_power_watts(0, 1.0), 0.0);

    // Zero elapsed time should return 0.0 without division by zero panics
    assert_eq!(calculate_power_watts(15_000_000, 0.0), 0.0);

    // Negative elapsed time
    assert_eq!(calculate_power_watts(15_000_000, -1.0), 0.0);
}

#[test]
fn test_telemetry_summary_calculation_single_sample() {
    let sample = TelemetryData {
        timestamp: 1700000000,
        cpu_temp: 70.0,
        gpu_temp: 60.0,
        cpu_usage: 85.0,
        gpu_usage: 95.0,
        cpu_freq: 3800.0,
        gpu_freq_mhz: 1900.0,
        vram_freq_mhz: 7000.0,
        power_w: 65.0,
        gpu_power_w: 40.0,
        ac_power_w: 110.0,
        ram_usage_mb: 8192.0,  // 8.0 GB
        vram_usage_mb: 4096.0, // 4.0 GB
        cpu_throttle: "Termal Kısma".to_string(),
        gpu_throttle: "Güç Sınırı".to_string(),
    };

    let summary = calculate_telemetry_summary(&[sample]);
    assert_eq!(summary.avg_cpu_temp, 70.0);
    assert_eq!(summary.peak_cpu_temp, 70.0);
    assert_eq!(summary.avg_gpu_temp, 60.0);
    assert_eq!(summary.peak_gpu_temp, 60.0);
    assert_eq!(summary.avg_cpu_freq_mhz, 3800);
    assert_eq!(summary.peak_cpu_freq_mhz, 3800);
    assert_eq!(summary.avg_gpu_freq_mhz, 1900);
    assert_eq!(summary.peak_gpu_freq_mhz, 1900);
    assert_eq!(summary.avg_vram_freq_mhz, 7000);
    assert_eq!(summary.peak_vram_freq_mhz, 7000);
    assert_eq!(summary.avg_power_w, 65.0);
    assert_eq!(summary.peak_power_w, 65.0);
    assert_eq!(summary.avg_gpu_power_w, 40.0);
    assert_eq!(summary.peak_gpu_power_w, 40.0);
    assert_eq!(summary.avg_ac_power_w, 110.0);
    assert_eq!(summary.peak_ac_power_w, 110.0);
    assert_eq!(summary.cpu_throttling, "Termal Kısma (%100)");
    assert_eq!(summary.gpu_throttling, "Güç Limiti (%100)");
}
