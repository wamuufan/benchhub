use anyhow::Context;
use std::path::Path;

pub use benchhub_types::models::*;

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

/// Atomically saves sanitized application settings to the specified path.
/// Writes to a temporary file and renames it to prevent corruption during unexpected shutdowns.
pub fn save_settings(path: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    let mut validated = settings.clone();
    validated.sanitize();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create settings directory: {:?}", parent))?;
    }

    let json_str = serde_json::to_string_pretty(&validated)
        .context("Failed to serialize AppSettings to JSON")?;

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp_path)
            .with_context(|| format!("Failed to create temporary settings file: {:?}", tmp_path))?;
        file.write_all(json_str.as_bytes()).with_context(|| {
            format!("Failed to write to temporary settings file: {:?}", tmp_path)
        })?;
        file.flush()
            .with_context(|| format!("Failed to flush temporary settings file: {:?}", tmp_path))?;
        file.sync_all()
            .with_context(|| format!("Failed to sync temporary settings file: {:?}", tmp_path))?;
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to rename temporary settings file {:?} -> {:?}",
            tmp_path, path
        )
    })?;

    tracing::debug!("Settings successfully saved: {:?}", path);
    Ok(())
}

/// Helper function to load, modify, and atomically persist settings.
pub fn update_settings<F>(path: &Path, updater: F) -> anyhow::Result<AppSettings>
where
    F: FnOnce(&mut AppSettings),
{
    let mut settings = load_settings(path);
    updater(&mut settings);
    save_settings(path, &settings)?;
    Ok(settings)
}
