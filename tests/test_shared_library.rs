use benchhub::config::AppSettings;
use benchhub::engine::BenchmarkEngine;
use benchhub::utils::{get_data_dir, resolve_shared_data_dir};
use std::path::PathBuf;

#[test]
fn test_benchmark_engine_runners_dir_override() {
    let base = PathBuf::from("/tmp/test_benchhub_base");
    let engine = BenchmarkEngine::new(base.clone());
    assert_eq!(engine.get_runners_dir(), base.join("runners"));

    let custom = PathBuf::from("/custom/path/runners");
    let engine_with_custom = engine.with_runners_dir(Some(custom.clone()));
    assert_eq!(engine_with_custom.get_runners_dir(), custom);

    // Empty custom dir should fallback to base/runners
    let engine_empty = BenchmarkEngine::new(base.clone()).with_runners_dir(Some(PathBuf::new()));
    assert_eq!(engine_empty.get_runners_dir(), base.join("runners"));
}

#[test]
fn test_app_settings_custom_runners_dir() {
    let mut settings = AppSettings::default();
    assert_eq!(settings.custom_runners_dir, "");

    settings.custom_runners_dir = "  /mnt/fast_nvme/runners  ".to_string();
    settings.sanitize();
    assert_eq!(settings.custom_runners_dir, "/mnt/fast_nvme/runners");

    // Test JSON serialization & deserialization
    let json = serde_json::to_string(&settings).expect("Failed to serialize AppSettings");
    let loaded: AppSettings =
        serde_json::from_str(&json).expect("Failed to deserialize AppSettings");
    assert_eq!(loaded.custom_runners_dir, "/mnt/fast_nvme/runners");
}

#[test]
fn test_resolve_shared_data_dir_smoke() {
    // When not running in Flatpak or Snap, returns None
    if std::env::var("FLATPAK_ID").is_err()
        && !std::path::Path::new("/.flatpak-info").exists()
        && std::env::var("SNAP").is_err()
    {
        assert!(resolve_shared_data_dir().is_none());
    }

    // get_data_dir() should always succeed and return a valid path
    let data_dir = get_data_dir().expect("get_data_dir failed");
    assert!(data_dir.is_absolute());
    assert!(data_dir.ends_with("benchhub"));
}

#[test]
fn test_dynamic_linking_detection_and_no_gui_spawn() {
    use benchhub::engine::{is_dynamically_broken, is_dynamically_linked_elf};
    use std::path::Path;

    // Standard system dynamic ELF
    let sh_path = Path::new("/bin/sh");
    if sh_path.exists() && is_dynamically_linked_elf(sh_path) {
        assert!(!is_dynamically_broken(sh_path));
    }

    // Superposition binary if installed
    let superposition = Path::new(
        "/home/user-0/.local/share/benchhub/runners/unigine/Superposition/extracted/bin/superposition",
    );
    if superposition.exists() {
        assert!(is_dynamically_linked_elf(superposition));
        // is_dynamically_broken must return false without spawning game GUI
        assert!(!is_dynamically_broken(superposition));
    }
}
