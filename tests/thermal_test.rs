use benchhub::config::{
    parse_cooldown_temp, parse_thermal_mode, parse_thermal_timeout, parse_warmup_temp, AppSettings,
};
use benchhub::thermal::{run_thermal_conditioning, ThermalConditioningMode};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_thermal_mode_enum_parsing() {
    assert_eq!(
        ThermalConditioningMode::parse_mode("off"),
        ThermalConditioningMode::Off
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("OFF"),
        ThermalConditioningMode::Off
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("cooldown"),
        ThermalConditioningMode::Cooldown
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("Cooldown"),
        ThermalConditioningMode::Cooldown
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("warmup"),
        ThermalConditioningMode::Warmup
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("Warmup"),
        ThermalConditioningMode::Warmup
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("smart"),
        ThermalConditioningMode::Smart
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("SMART"),
        ThermalConditioningMode::Smart
    );
    assert_eq!(
        ThermalConditioningMode::parse_mode("invalid_mode"),
        ThermalConditioningMode::Off
    );
    // Test standard FromStr trait implementation
    assert_eq!(
        "cooldown".parse::<ThermalConditioningMode>().unwrap(),
        ThermalConditioningMode::Cooldown
    );
}

#[test]
fn test_config_thermal_parsing_helpers() {
    assert_eq!(parse_thermal_mode("Kapalı"), "off");
    assert_eq!(parse_thermal_mode("Off"), "off");
    assert_eq!(parse_thermal_mode("Ön Soğutma (Cooldown)"), "cooldown");
    assert_eq!(parse_thermal_mode("Cooldown"), "cooldown");
    assert_eq!(parse_thermal_mode("Ön Isıtma (Warmup)"), "warmup");
    assert_eq!(parse_thermal_mode("Warmup"), "warmup");
    assert_eq!(parse_thermal_mode("Akıllı Şartlandırma (Smart)"), "smart");
    assert_eq!(parse_thermal_mode("Smart"), "smart");

    assert_eq!(parse_cooldown_temp("40 °C"), 40);
    assert_eq!(parse_cooldown_temp("50 °C (Varsayılan)"), 50);
    assert_eq!(parse_cooldown_temp("60 °C"), 60);

    assert_eq!(parse_warmup_temp("55 °C"), 55);
    assert_eq!(parse_warmup_temp("65 °C (Varsayılan)"), 65);
    assert_eq!(parse_warmup_temp("75 °C"), 75);

    assert_eq!(parse_thermal_timeout("15 sn"), 15);
    assert_eq!(parse_thermal_timeout("30 sn (Varsayılan)"), 30);
    assert_eq!(parse_thermal_timeout("60 sn (1 dk)"), 60);
    assert_eq!(parse_thermal_timeout("120s"), 120);
}

#[test]
fn test_app_settings_thermal_sanitization() {
    let mut settings = AppSettings::default();
    assert_eq!(settings.thermal_mode, "off");
    assert_eq!(settings.thermal_cooldown_temp, 50);
    assert_eq!(settings.thermal_warmup_temp, 65);
    assert_eq!(settings.thermal_timeout_secs, 30);

    // Invalid mode should sanitize to "off"
    settings.thermal_mode = "super_cooling".to_string();
    settings.thermal_cooldown_temp = 20; // out of range (min 35)
    settings.thermal_warmup_temp = 120; // out of range (max 95)
    settings.thermal_timeout_secs = 2; // out of range (min 5)
    settings.sanitize();

    assert_eq!(settings.thermal_mode, "off");
    assert_eq!(settings.thermal_cooldown_temp, 35);
    assert_eq!(settings.thermal_warmup_temp, 95);
    assert_eq!(settings.thermal_timeout_secs, 5);
}

#[tokio::test]
async fn test_thermal_conditioning_off_returns_immediately() {
    let settings = AppSettings {
        thermal_mode: "off".to_string(),
        ..AppSettings::default()
    };

    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    let log_count = Arc::new(AtomicUsize::new(0));
    let lc = log_count.clone();

    let completed = run_thermal_conditioning(
        &settings,
        &mut cancel_rx,
        move |_log| {
            lc.fetch_add(1, Ordering::Relaxed);
        },
        move |_status| {},
    )
    .await;

    assert!(completed);
    assert_eq!(log_count.load(Ordering::Relaxed), 0);
    drop(cancel_tx);
}

#[tokio::test]
async fn test_thermal_conditioning_cancellation() {
    let settings = AppSettings {
        thermal_mode: "cooldown".to_string(),
        thermal_cooldown_temp: 30, // Very low target that won't be reached
        thermal_timeout_secs: 60,
        ..AppSettings::default()
    };

    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

    // Cancel after 100ms
    let tx_clone = cancel_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = tx_clone.send(true);
    });

    let completed =
        run_thermal_conditioning(&settings, &mut cancel_rx, |_log| {}, |_status| {}).await;

    // Should return false because it was cancelled
    assert!(!completed);
    drop(cancel_tx);
}
