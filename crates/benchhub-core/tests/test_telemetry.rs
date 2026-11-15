use benchhub_core::engine::BenchmarkEngine;
use benchhub_core::models::{BenchmarkProfile, TelemetryData};
use benchhub_core::telemetry::{calculate_power_watts, calculate_telemetry_summary};

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
    use benchhub_core::telemetry::get_sensor_diagnosis;

    let diag = get_sensor_diagnosis();
    assert!(
        !diag.is_empty(),
        "Sensor diagnosis string should not be empty"
    );
}

#[test]
fn test_live_telemetry_cpu_and_gpu_readings() {
    use benchhub_core::telemetry::TelemetryEngine;

    let mut engine = TelemetryEngine::new();
    std::thread::sleep(std::time::Duration::from_millis(150));
    let data = engine.read_current();
    println!("Live snapshot: CPU temp={:.1}C, freq={:.0}MHz, usage={:.1}%, power={:.1}W, GPU temp={:.1}C, power={:.1}W, AC power={:.1}W",
        data.cpu_temp, data.cpu_freq, data.cpu_usage, data.power_w, data.gpu_temp, data.gpu_power_w, data.ac_power_w);
    assert!(
        data.cpu_temp >= 0.0,
        "CPU temp should be non-negative on live hardware or virtualized runners"
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
        category: "Graphics (GPU)".to_string(),
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
    tokio::fs::write(&binary_path, "#!/bin/sh\necho Score: 5555\n")
        .await
        .unwrap();

    let profile = BenchmarkProfile {
        id: "unigine_heaven".to_string(),
        name: "Unigine Heaven".to_string(),
        category: "Graphics (GPU)".to_string(),
        binary_relative_path: Some("extracted/heaven".to_string()),
        ..Default::default()
    };

    let resolved = BenchmarkEngine::resolve_executable(&runner_dir, &profile)
        .expect("Should auto-discover .sh extension for heaven");

    assert_eq!(resolved.work_dir, extracted_dir);
    assert_eq!(resolved.executable, "./heaven.sh");
    assert_eq!(resolved.full_path, binary_path);

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[test]
fn test_system_info_summary_generation() {
    use benchhub_core::telemetry::get_system_info_summary;
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
            cpu_throttle: "none".to_string(),
            gpu_throttle: "none".to_string(),
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
            cpu_throttle: "thermal_throttle".to_string(),
            gpu_throttle: "power_limit".to_string(),
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
    assert_eq!(summary.cpu_throttling, "thermal_throttle (50%)");
    assert_eq!(summary.gpu_throttling, "power_limit (50%)");
}

#[test]
fn test_get_power_profile_detection() {
    use benchhub_core::telemetry::get_power_profile;

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
        cpu_throttle: "thermal_throttle".to_string(),
        gpu_throttle: "power_limit".to_string(),
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
    assert_eq!(summary.cpu_throttling, "thermal_throttle (100%)");
    assert_eq!(summary.gpu_throttling, "power_limit (100%)");
}

#[test]
fn test_battery_discharging_and_device_scope_filtering() {
    use benchhub_core::telemetry::{is_battery_discharging, is_non_system_power_supply};

    assert!(is_non_system_power_supply("hidpp_battery_0", None));
    assert!(is_non_system_power_supply("BAT0", Some("Device")));
    assert!(!is_non_system_power_supply("BAT1", Some("System")));
    assert!(!is_non_system_power_supply("BAT0", None));

    assert!(is_battery_discharging("Discharging"));
    assert!(is_battery_discharging(" discharging \n"));
    assert!(!is_battery_discharging("Charging"));
    assert!(!is_battery_discharging("Full"));
    assert!(!is_battery_discharging("Not charging"));
}

#[test]
fn test_ac_power_ignores_charging_battery_via_sysfs() {
    use benchhub_core::telemetry::{BatterySensor, TelemetryEngine};

    let dir = tempfile::tempdir().unwrap();
    let status_p = dir.path().join("status");
    let power_p = dir.path().join("power_now");
    let online_p = dir.path().join("online");

    // AC online = 1, battery is Charging at 5.34W -> must compute AC estimate, not 5.34W!
    std::fs::write(&status_p, "Charging\n").unwrap();
    std::fs::write(&power_p, "5340000\n").unwrap();
    std::fs::write(&online_p, "1\n").unwrap();

    let mut engine = TelemetryEngine::new();
    engine.sensor_cache_mut().battery_sensors = vec![BatterySensor {
        power_path: power_p.clone(),
        status_path: status_p.clone(),
    }];
    engine.sensor_cache_mut().ac_online_paths = vec![online_p.clone()];

    assert!((engine.sample_ac_power_w(30.0, 10.0) - 52.0).abs() < 0.01);

    // Unplugged & Discharging at 24.5W -> should read battery discharge directly
    std::fs::write(&status_p, "Discharging\n").unwrap();
    std::fs::write(&power_p, "24500000\n").unwrap();
    std::fs::write(&online_p, "0\n").unwrap();
    assert!((engine.sample_ac_power_w(30.0, 10.0) - 24.5).abs() < 0.01);
}

#[test]
fn test_gpu_mode_integrated_reads_sysfs_without_nvml_fallback() {
    use benchhub_core::models::GpuMode;
    use benchhub_core::telemetry::TelemetryEngine;

    let dir = tempfile::tempdir().unwrap();
    let busy_p = dir.path().join("gpu_busy_percent");
    let temp_p = dir.path().join("temp1_input");
    let vram_p = dir.path().join("mem_info_vram_used");
    std::fs::write(&busy_p, "42\n").unwrap();
    std::fs::write(&temp_p, "51000\n").unwrap();
    std::fs::write(&vram_p, "536870912\n").unwrap();

    let mut engine = TelemetryEngine::new();
    engine.sensor_cache_mut().gpu_usage_paths = vec![busy_p];
    engine.sensor_cache_mut().gpu_temp_paths = vec![temp_p];
    engine.sensor_cache_mut().gpu_power_paths.clear();
    engine.sensor_cache_mut().vram_used_paths = vec![vram_p];

    let sample = engine.read_current_for_mode(GpuMode::Integrated);
    assert!((sample.gpu_usage - 42.0).abs() < 0.01);
    assert!((sample.gpu_temp - 51.0).abs() < 0.01);
    assert!((sample.vram_usage_mb - 512.0).abs() < 0.1);
}

#[tokio::test]
async fn test_telemetry_loop_pauses_at_zero_and_resumes_on_update() {
    use benchhub_core::models::GpuMode;
    use benchhub_core::telemetry::start_telemetry_loop_with_mode;
    use std::time::Duration;

    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let (interval_tx, interval_rx) = tokio::sync::watch::channel(0_u64);
    let (gpu_tx, gpu_rx) = tokio::sync::watch::channel(GpuMode::Auto);

    let handle = tokio::spawn(start_telemetry_loop_with_mode(tx, interval_rx, gpu_rx));

    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(
        rx.try_recv().is_err(),
        "Paused loop (interval 0) should not emit samples"
    );

    gpu_tx.send(GpuMode::Integrated).unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        rx.try_recv().is_err(),
        "GPU mode change while paused must keep polling disabled"
    );

    interval_tx.send(150).unwrap();
    let sample = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Loop should emit promptly upon resume")
        .expect("Telemetry channel should stay open");
    assert!(sample.timestamp > 0);

    interval_tx.send(0).unwrap();
    while rx.try_recv().is_ok() {}
    tokio::time::sleep(Duration::from_millis(180)).await;
    assert!(
        rx.try_recv().is_err(),
        "Returning interval to 0 should pause polling again"
    );

    drop(rx);
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}
