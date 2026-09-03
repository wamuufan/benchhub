use crate::AppWindow;
use benchhub::models::TelemetryData;
use std::path::PathBuf;
use tokio::sync::{mpsc, watch};

use anyhow::{Context, Result};
use tokio::fs;

pub fn get_data_dir() -> Result<std::path::PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to determine user data directory"))?;
    let path = base.join("benchhub");
    std::fs::create_dir_all(&path)
        .with_context(|| format!("Failed to create data directory: {:?}", path))?;
    Ok(path)
}

pub async fn ensure_data_dir() -> Result<std::path::PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to determine user data directory"))?;
    let path = base.join("benchhub");
    fs::create_dir_all(&path)
        .await
        .with_context(|| format!("Failed to create data directory: {:?}", path))?;
    Ok(path)
}

pub use benchhub::config::{load_settings, save_settings, update_settings};

pub async fn calculate_dir_size(path: &std::path::Path) -> u64 {
    calculate_dir_size_result(path).await.unwrap_or(0)
}

pub async fn calculate_dir_size_result(path: &std::path::Path) -> Result<u64> {
    if !fs::try_exists(path).await.unwrap_or(false) {
        return Ok(0);
    }
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let mut entries = fs::read_dir(&current_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let p = entry.path();
            let meta = entry.metadata().await?;
            if meta.is_dir() {
                stack.push(p);
            } else {
                total += meta.len();
            }
        }
    }
    Ok(total)
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub async fn load_profiles() -> Result<Vec<benchhub::models::BenchmarkProfile>> {
    let mut profiles = Vec::new();
    let dir = std::path::Path::new("benchmarks");
    if !fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(profiles);
    }
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            let content = fs::read_to_string(&path).await?;
            if let Ok(profile) = toml::from_str::<benchhub::models::BenchmarkProfile>(&content) {
                profiles.push(profile);
            }
        }
    }
    Ok(profiles)
}

pub fn load_profiles_sync() -> Vec<benchhub::models::BenchmarkProfile> {
    let mut profiles = Vec::new();
    let dir = std::path::Path::new("benchmarks");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(profile) =
                        toml::from_str::<benchhub::models::BenchmarkProfile>(&content)
                    {
                        profiles.push(profile);
                    }
                }
            }
        }
    }
    profiles
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let wl_result = (|| -> anyhow::Result<()> {
        let mut child = Command::new("wl-copy").stdin(Stdio::piped()).spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }

        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("wl-copy exited with status {}", status);
        }
        Ok(())
    })();

    if let Err(e) = wl_result {
        tracing::debug!("wl-copy failed, falling back to xclip: {}", e);

        let mut child = Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-in")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|err| {
                tracing::error!("Failed to spawn xclip: {}", err);
                anyhow::anyhow!("Failed to spawn xclip: {}", err)
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }

        let status = child.wait()?;
        if !status.success() {
            tracing::error!("xclip failed with status {}", status);
            anyhow::bail!("xclip exited with status {}", status);
        }
    }

    Ok(())
}

pub use benchhub::export::export_history_csv;

/// Background task that consumes telemetry samples, writes them to DB if recording,
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryUpdateDiff {
    pub update_cpu_temp: bool,
    pub update_gpu_temp: bool,
    pub update_cpu_freq: bool,
    pub update_power_w: bool,
    pub update_cpu_throttle: bool,
    pub update_gpu_throttle: bool,
}

impl TelemetryUpdateDiff {
    pub fn has_updates(&self) -> bool {
        self.update_cpu_temp
            || self.update_gpu_temp
            || self.update_cpu_freq
            || self.update_power_w
            || self.update_cpu_throttle
            || self.update_gpu_throttle
    }
}

pub const TELEMETRY_TEMP_THRESHOLD: f32 = 0.5;
pub const TELEMETRY_FREQ_THRESHOLD: f32 = 10.0;
pub const TELEMETRY_POWER_THRESHOLD: f32 = 0.5;

#[allow(clippy::too_many_arguments)]
pub fn compute_telemetry_ui_diff(
    current_cpu_temp: f32,
    current_gpu_temp: f32,
    current_cpu_freq: f32,
    current_power_w: f32,
    current_cpu_throttle: &str,
    current_gpu_throttle: &str,
    data: &TelemetryData,
    localized_cpu_throttle: &str,
    localized_gpu_throttle: &str,
) -> TelemetryUpdateDiff {
    TelemetryUpdateDiff {
        update_cpu_temp: (current_cpu_temp - data.cpu_temp).abs() > TELEMETRY_TEMP_THRESHOLD,
        update_gpu_temp: (current_gpu_temp - data.gpu_temp).abs() > TELEMETRY_TEMP_THRESHOLD,
        update_cpu_freq: (current_cpu_freq - data.cpu_freq).abs() > TELEMETRY_FREQ_THRESHOLD,
        update_power_w: (current_power_w - data.power_w).abs() > TELEMETRY_POWER_THRESHOLD,
        update_cpu_throttle: current_cpu_throttle != localized_cpu_throttle,
        update_gpu_throttle: current_gpu_throttle != localized_gpu_throttle,
    }
}

/// Background task that consumes telemetry samples, writes them to DB if recording,
/// and throttles Slint UI updates to 500ms intervals.
pub fn spawn_telemetry_worker(
    ui_weak: slint::Weak<AppWindow>,
    app_state: crate::state::AppState,
    mut telemetry_rx: mpsc::Receiver<TelemetryData>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_ui_update = std::time::Instant::now();
        let ui_throttle_interval = std::time::Duration::from_millis(500);

        while let Some(data) = telemetry_rx.recv().await {
            // Ingest and record samples at full speed without dropping
            if app_state
                .is_recording_samples
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                app_state.current_samples.lock().await.push(data.clone());
                let rid = app_state
                    .active_run_id
                    .load(std::sync::atomic::Ordering::Relaxed);
                if rid > 0 {
                    let db = app_state.service.db.clone();
                    let d = data.clone();
                    tokio::task::spawn_blocking(move || {
                        let _ = db.insert_telemetry_sample(rid, &d);
                    });
                }
            }

            // Decoupled, throttled UI update to avoid overwhelming Slint event loop
            if last_ui_update.elapsed() >= ui_throttle_interval {
                last_ui_update = std::time::Instant::now();
                let ui_w = ui_weak.clone();
                let ui_data = data;
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui_instance) = ui_w.upgrade() {
                        // Only dirty and re-render telemetry cards if the user is actually viewing the Live Monitor tab (tab 1)
                        if ui_instance.get_active_tab() == 1 {
                            let cpu_throttle_loc =
                                benchhub::i18n::localize_throttling(&ui_data.cpu_throttle);
                            let gpu_throttle_loc =
                                benchhub::i18n::localize_throttling(&ui_data.gpu_throttle);

                            let diff = compute_telemetry_ui_diff(
                                ui_instance.get_cpu_temp(),
                                ui_instance.get_gpu_temp(),
                                ui_instance.get_cpu_freq(),
                                ui_instance.get_power_w(),
                                &ui_instance.get_cpu_throttling_status(),
                                &ui_instance.get_gpu_throttling_status(),
                                &ui_data,
                                &cpu_throttle_loc,
                                &gpu_throttle_loc,
                            );

                            if diff.update_cpu_temp {
                                ui_instance.set_cpu_temp(ui_data.cpu_temp);
                            }
                            if diff.update_gpu_temp {
                                ui_instance.set_gpu_temp(ui_data.gpu_temp);
                            }
                            if diff.update_cpu_freq {
                                ui_instance.set_cpu_freq(ui_data.cpu_freq);
                            }
                            if diff.update_power_w {
                                ui_instance.set_power_w(ui_data.power_w);
                            }
                            if diff.update_cpu_throttle {
                                ui_instance.set_cpu_throttling_status(cpu_throttle_loc.into());
                            }
                            if diff.update_gpu_throttle {
                                ui_instance.set_gpu_throttling_status(gpu_throttle_loc.into());
                            }
                        }
                    }
                });
            }
        }
    })
}

/// Forwards tracing logs from the mpsc channel to the terminal flusher channel.
pub fn spawn_log_forwarder(
    mut log_rx: mpsc::Receiver<String>,
    terminal_tx: mpsc::Sender<crate::state::TerminalMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(msg) = log_rx.recv().await {
            let _ = terminal_tx.try_send(crate::state::TerminalMsg::Append(msg));
        }
    })
}

/// High performance batched terminal flusher ring buffer task.
pub fn spawn_terminal_flusher(
    ui_weak: slint::Weak<AppWindow>,
    console_log_path: PathBuf,
    initial_console_log: String,
    mut terminal_limit_rx: watch::Receiver<usize>,
    mut terminal_rx: mpsc::Receiver<crate::state::TerminalMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut max_lines = *terminal_limit_rx.borrow_and_update();
        let mut ring_buffer = benchhub::logging::TerminalRingBuffer::new(max_lines);
        if !initial_console_log.is_empty() {
            ring_buffer.push_str(&initial_console_log);
        }

        let mut file_buffer = String::with_capacity(8192);
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(250));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut ui_dirty = false;
        let mut last_pushed_len = initial_console_log.len();

        let file_res = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&console_log_path)
            .await;

        let mut opt_writer = match file_res {
            Ok(f) => Some(tokio::io::BufWriter::new(f)),
            Err(e) => {
                tracing::error!("Failed to open benchhub_console.log: {}", e);
                None
            }
        };

        loop {
            tokio::select! {
                _ = terminal_limit_rx.changed() => {
                    max_lines = *terminal_limit_rx.borrow();
                    ring_buffer.set_max_lines(max_lines);
                    ui_dirty = true;
                }
                Some(msg) = terminal_rx.recv() => {
                    match msg {
                        crate::state::TerminalMsg::Append(chunk) => {
                            file_buffer.push_str(&chunk);
                            ring_buffer.push_str(&chunk);
                            ui_dirty = true;

                            if file_buffer.len() > 8192 {
                                let to_write = std::mem::take(&mut file_buffer);
                                if let Some(writer) = opt_writer.as_mut() {
                                    use tokio::io::AsyncWriteExt;
                                    let _ = writer.write_all(to_write.as_bytes()).await;
                                    let _ = writer.flush().await;
                                }
                            }
                        }
                        crate::state::TerminalMsg::Clear => {
                            file_buffer.clear();
                            ring_buffer.clear();
                            ui_dirty = false;
                            last_pushed_len = 0;
                            let _ = tokio::fs::write(&console_log_path, "").await;
                            let ui_w = ui_weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = ui_w.upgrade() {
                                    ui.set_terminal_output("".into());
                                }
                            });
                        }
                    }
                }
                _ = interval.tick() => {
                    if !file_buffer.is_empty() {
                        let to_write = std::mem::take(&mut file_buffer);
                        if let Some(writer) = opt_writer.as_mut() {
                            use tokio::io::AsyncWriteExt;
                            let _ = writer.write_all(to_write.as_bytes()).await;
                            let _ = writer.flush().await;
                        }
                    }

                    if ui_dirty {
                        let ui_w = ui_weak.clone();
                        let text_to_set = ring_buffer.to_string();
                        let new_len = text_to_set.len();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_w.upgrade() {
                                ui.set_terminal_output(text_to_set.into());
                            }
                        });
                        if new_len != last_pushed_len {
                            last_pushed_len = new_len;
                        }
                        ui_dirty = false;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_telemetry_ui_diff_thresholds() {
        let sample = TelemetryData {
            timestamp: 1000,
            cpu_temp: 75.0,
            gpu_temp: 65.0,
            cpu_usage: 50.0,
            gpu_usage: 40.0,
            cpu_freq: 3500.0,
            gpu_freq_mhz: 1800.0,
            vram_freq_mhz: 7000.0,
            power_w: 120.0,
            gpu_power_w: 60.0,
            ac_power_w: 150.0,
            ram_usage_mb: 8000.0,
            vram_usage_mb: 4000.0,
            cpu_throttle: "None".to_string(),
            gpu_throttle: "None".to_string(),
        };

        // 1. Identical values: no updates
        let diff_same = compute_telemetry_ui_diff(
            75.0, 65.0, 3500.0, 120.0, "Yok", "Yok", &sample, "Yok", "Yok",
        );
        assert!(!diff_same.has_updates());

        // 2. Changes within thresholds (abs <= threshold): no updates
        let diff_small = compute_telemetry_ui_diff(
            75.4,   // diff 0.4 <= 0.5
            64.6,   // diff 0.4 <= 0.5
            3509.0, // diff 9.0 <= 10.0
            120.5,  // diff 0.5 <= 0.5 (exact boundary)
            "Yok", "Yok", &sample, "Yok", "Yok",
        );
        assert!(!diff_small.has_updates());
        assert!(!diff_small.update_cpu_temp);
        assert!(!diff_small.update_gpu_temp);
        assert!(!diff_small.update_cpu_freq);
        assert!(!diff_small.update_power_w);

        // 3. Changes exceeding thresholds (abs > threshold): updates triggered
        let diff_large = compute_telemetry_ui_diff(
            74.4,   // diff 0.6 > 0.5
            65.6,   // diff 0.6 > 0.5
            3485.0, // diff 15.0 > 10.0
            120.6,  // diff 0.6 > 0.5
            "Termal",
            "Güç Limiti",
            &sample,
            "Yok",
            "Yok",
        );
        assert!(diff_large.has_updates());
        assert!(diff_large.update_cpu_temp);
        assert!(diff_large.update_gpu_temp);
        assert!(diff_large.update_cpu_freq);
        assert!(diff_large.update_power_w);
        assert!(diff_large.update_cpu_throttle);
        assert!(diff_large.update_gpu_throttle);
    }

    #[tokio::test]
    async fn test_get_data_dir_cross_platform() {
        let dir = get_data_dir().expect("get_data_dir should succeed on standard environments");
        assert!(dir.ends_with("benchhub"));
        assert!(dir.exists());

        let async_dir = ensure_data_dir()
            .await
            .expect("ensure_data_dir should succeed");
        assert_eq!(dir, async_dir);
    }
}
