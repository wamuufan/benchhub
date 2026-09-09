use benchhub::config::{
    load_settings, save_settings, update_settings, AppSettings, DEFAULT_SAVE_STOPPED_RUNS,
    DEFAULT_SELECTED_GPU_MODE, DEFAULT_TELEMETRY_INTERVAL_MS, DEFAULT_TERMINAL_BUFFER_LINES,
};
use tempfile::tempdir;

#[test]
fn test_config_defaults() {
    let settings = AppSettings::default();
    assert_eq!(
        settings.telemetry_interval_ms,
        DEFAULT_TELEMETRY_INTERVAL_MS
    );
    assert_eq!(
        settings.terminal_buffer_lines,
        DEFAULT_TERMINAL_BUFFER_LINES
    );
    assert_eq!(settings.save_stopped_runs, DEFAULT_SAVE_STOPPED_RUNS);
    assert_eq!(settings.selected_gpu_mode, DEFAULT_SELECTED_GPU_MODE);
    assert_eq!(settings.max_test_duration_secs, 300);
}

#[test]
fn test_config_boundary_validations_and_sanitization() {
    let mut settings = AppSettings {
        telemetry_interval_ms: 12345, // Invalid value (valid: 250, 500, 1000, 2000)
        terminal_buffer_lines: 10,    // Below 50
        save_stopped_runs: true,
        selected_gpu_mode: "custom_unknown".into(),
        language: "en".into(),
        max_test_duration_secs: 9999,
    };

    settings.sanitize();

    assert_eq!(settings.telemetry_interval_ms, 500);
    assert_eq!(settings.terminal_buffer_lines, 50);
    assert_eq!(settings.selected_gpu_mode, "System Default");
    assert_eq!(settings.max_test_duration_secs, 300);

    let mut upper_settings = AppSettings {
        telemetry_interval_ms: 2000,
        terminal_buffer_lines: 5000, // Above 1000
        save_stopped_runs: false,
        selected_gpu_mode: "Dahili GPU (iGPU)".into(),
        language: "tr".into(),
        max_test_duration_secs: 180,
    };

    upper_settings.sanitize();

    assert_eq!(upper_settings.telemetry_interval_ms, 2000);
    assert_eq!(upper_settings.terminal_buffer_lines, 1000);
    assert_eq!(upper_settings.selected_gpu_mode, "Dahili GPU (iGPU)");
    assert_eq!(upper_settings.max_test_duration_secs, 180);
}

#[test]
fn test_config_persistence_with_tempfile() {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("config.json");

    let custom = AppSettings {
        telemetry_interval_ms: 250,
        terminal_buffer_lines: 500,
        save_stopped_runs: false,
        selected_gpu_mode: "Dahili GPU (iGPU)".into(),
        language: "tr".into(),
        max_test_duration_secs: 120,
    };

    save_settings(&config_path, &custom).expect("Failed to save settings");
    assert!(config_path.exists());

    let loaded = load_settings(&config_path);
    assert_eq!(loaded, custom);
}

#[test]
fn test_config_atomic_update_helper() {
    let dir = tempdir().expect("Failed to create tempdir");
    let config_path = dir.path().join("config_atomic.json");

    let updated = update_settings(&config_path, |s| {
        s.telemetry_interval_ms = 1000;
        s.terminal_buffer_lines = 300;
    })
    .expect("update_settings failed");

    assert_eq!(updated.telemetry_interval_ms, 1000);
    assert_eq!(updated.terminal_buffer_lines, 300);

    let reloaded = load_settings(&config_path);
    assert_eq!(reloaded.telemetry_interval_ms, 1000);
    assert_eq!(reloaded.terminal_buffer_lines, 300);
}

#[test]
fn test_config_recovery_from_corrupted_json() {
    let dir = tempdir().expect("Failed to create tempdir");
    let corrupted_path = dir.path().join("corrupted.json");

    std::fs::write(&corrupted_path, "{ broken json ... not closed }").unwrap();

    let loaded = load_settings(&corrupted_path);
    assert_eq!(loaded, AppSettings::default());
}

#[test]
fn test_config_recovery_from_partial_json() {
    let dir = tempdir().expect("Failed to create tempdir");
    let partial_path = dir.path().join("partial.json");

    std::fs::write(&partial_path, r#"{"telemetry_interval_ms": 250}"#).unwrap();

    let loaded = load_settings(&partial_path);
    assert_eq!(loaded.telemetry_interval_ms, 250);
    assert_eq!(loaded.terminal_buffer_lines, DEFAULT_TERMINAL_BUFFER_LINES);
    assert_eq!(loaded.save_stopped_runs, DEFAULT_SAVE_STOPPED_RUNS);
    assert_eq!(loaded.selected_gpu_mode, DEFAULT_SELECTED_GPU_MODE);
}
