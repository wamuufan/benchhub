use anyhow::Context;
use std::path::Path;
use std::sync::Mutex;

pub use benchhub_types::models::*;

static SETTINGS_LOCK: Mutex<()> = Mutex::new(());

/// Loads and validates application settings from the specified JSON file path.
/// If the file does not exist, or contains invalid data/JSON, logs a warning
/// and gracefully falls back to sanitized default settings without overwriting user data.
pub fn load_settings(path: &Path) -> AppSettings {
    if !path.exists() {
        tracing::debug!(
            "Settings file not found ({:?}), loading default settings.",
            path
        );
        return AppSettings::default();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse settings file ({:?}): {}. Falling back to default settings.",
                    path,
                    e
                );
                AppSettings::default()
            }
        },
        Err(e) => {
            tracing::warn!(
                "Failed to read settings file ({:?}): {}. Falling back to default settings.",
                path,
                e
            );
            AppSettings::default()
        }
    }
}

fn save_settings_unlocked(path: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    let mut validated = settings.clone();
    validated.sanitize();

    let parent_dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(parent_dir)
        .with_context(|| format!("Failed to create settings directory: {:?}", parent_dir))?;

    let json_str = serde_json::to_string_pretty(&validated)
        .context("Failed to serialize AppSettings to JSON")?;

    let mut tmp_file = tempfile::NamedTempFile::new_in(parent_dir).with_context(|| {
        format!(
            "Failed to create temporary settings file in {:?}",
            parent_dir
        )
    })?;
    {
        use std::io::Write;
        tmp_file.write_all(json_str.as_bytes()).with_context(|| {
            format!(
                "Failed to write to temporary settings file: {:?}",
                tmp_file.path()
            )
        })?;
        tmp_file.flush().with_context(|| {
            format!(
                "Failed to flush temporary settings file: {:?}",
                tmp_file.path()
            )
        })?;
        tmp_file.as_file().sync_all().with_context(|| {
            format!(
                "Failed to sync temporary settings file: {:?}",
                tmp_file.path()
            )
        })?;
    }

    tmp_file
        .persist(path)
        .map_err(|e| anyhow::anyhow!(e.error))
        .with_context(|| format!("Failed to persist settings file to {:?}", path))?;

    tracing::debug!("Settings successfully saved: {:?}", path);
    Ok(())
}

/// Atomically saves sanitized application settings to the specified path.
/// Writes to a unique temporary file in the target directory and renames it to prevent corruption.
pub fn save_settings(path: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    let _guard = SETTINGS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    save_settings_unlocked(path, settings)
}

/// Helper function to load, modify, and atomically persist settings.
pub fn update_settings<F>(path: &Path, updater: F) -> anyhow::Result<AppSettings>
where
    F: FnOnce(&mut AppSettings),
{
    let _guard = SETTINGS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = load_settings(path);
    updater(&mut settings);
    save_settings_unlocked(path, &settings)?;
    Ok(settings)
}
