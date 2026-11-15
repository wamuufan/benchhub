use crate::config::AppSettings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalConditioningMode {
    Off,
    Cooldown,
    Warmup,
    Smart,
}

impl std::str::FromStr for ThermalConditioningMode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "cooldown" => Self::Cooldown,
            "warmup" => Self::Warmup,
            "smart" => Self::Smart,
            _ => Self::Off,
        })
    }
}

impl ThermalConditioningMode {
    pub fn parse_mode(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cooldown" => Self::Cooldown,
            "warmup" => Self::Warmup,
            "smart" => Self::Smart,
            _ => Self::Off,
        }
    }
}

/// Runs thermal conditioning (Cooldown or Warmup) according to AppSettings.
/// Returns true if conditioning completed (or was disabled), false if cancelled by user.
pub async fn run_thermal_conditioning<F, S>(
    settings: &AppSettings,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
    mut on_log: F,
    mut on_status: S,
) -> bool
where
    F: FnMut(String) + Send,
    S: FnMut(String) + Send,
{
    let mode = ThermalConditioningMode::parse_mode(&settings.thermal_mode);
    if mode == ThermalConditioningMode::Off {
        return true;
    }

    let cooldown_target = settings.thermal_cooldown_temp as f32;
    let warmup_target = settings.thermal_warmup_temp as f32;
    let max_timeout = settings.thermal_timeout_secs as u64;

    let get_temp = || -> f32 {
        let t = crate::telemetry::get_instant_cpu_temp();
        if t > 0.0 {
            t
        } else {
            45.0 // fallback default if no hardware sensor is accessible
        }
    };

    let initial_temp = get_temp();

    // Determine actual conditioning mode to execute
    let active_mode = match mode {
        ThermalConditioningMode::Cooldown => ThermalConditioningMode::Cooldown,
        ThermalConditioningMode::Warmup => ThermalConditioningMode::Warmup,
        ThermalConditioningMode::Smart => {
            if initial_temp > cooldown_target + 3.0 {
                ThermalConditioningMode::Cooldown
            } else if initial_temp < warmup_target - 5.0 {
                ThermalConditioningMode::Warmup
            } else {
                let temp_str = format!("{:.1}°C", initial_temp);
                let done_msg = format!("[BenchHub] Thermal target reached ({})\n", temp_str);
                on_log(done_msg);
                return true;
            }
        }
        ThermalConditioningMode::Off => return true,
    };

    let start = Instant::now();
    let max_duration = Duration::from_secs(max_timeout);

    match active_mode {
        ThermalConditioningMode::Cooldown => {
            let start_msg = format!(
                "\n[BenchHub] Cooldown (Target: <={:.0}°C, Current: {:.1}°C, Timeout: {}s)\n",
                cooldown_target, initial_temp, max_timeout
            );
            on_log(start_msg);

            while start.elapsed() < max_duration {
                if *cancel_rx.borrow() {
                    let cancel_msg = "\n[BenchHub] Cooldown cancelled by user.\n".to_string();
                    on_log(cancel_msg);
                    return false;
                }

                let current_temp = get_temp();
                let remaining_secs = max_timeout.saturating_sub(start.elapsed().as_secs());

                let status_text = format!(
                    "Cooldown - CPU Temp: {:.0}°C -> Target: <={:.0}°C ({}s)",
                    current_temp, cooldown_target, remaining_secs
                );
                on_status(status_text);

                if current_temp <= cooldown_target {
                    let done_msg =
                        format!("[BenchHub] Cooldown completed ({:.1}°C)\n", current_temp);
                    on_log(done_msg);
                    return true;
                }

                tokio::select! {
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() {
                            return false;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
            }

            let timeout_msg = format!(
                "[BenchHub] WARNING: Cooldown timed out ({:.1}°C)\n",
                get_temp()
            );
            on_log(timeout_msg);
            true
        }
        ThermalConditioningMode::Warmup => {
            let start_msg = format!(
                "\n[BenchHub] Warmup (Target: >={:.0}°C, Current: {:.1}°C, Timeout: {}s)\n",
                warmup_target, initial_temp, max_timeout
            );
            on_log(start_msg);

            // Launch lightweight arithmetic stress threads to warm up CPU cores
            let is_running = Arc::new(AtomicBool::new(true));
            let num_threads = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(2);
            let mut handles = Vec::new();

            for _ in 0..num_threads {
                let running_flag = is_running.clone();
                let handle = std::thread::spawn(move || {
                    let mut val: u64 = 0x123456789abcdef0;
                    while running_flag.load(Ordering::Relaxed) {
                        for _ in 0..10_000 {
                            val = val.wrapping_mul(6364136223846793005).wrapping_add(1);
                        }
                        std::hint::black_box(val);
                        std::thread::yield_now();
                    }
                });
                handles.push(handle);
            }

            let mut success = true;

            while start.elapsed() < max_duration {
                if *cancel_rx.borrow() {
                    let cancel_msg = "\n[BenchHub] Warmup cancelled by user.\n".to_string();
                    on_log(cancel_msg);
                    success = false;
                    break;
                }

                let current_temp = get_temp();
                let remaining_secs = max_timeout.saturating_sub(start.elapsed().as_secs());

                let status_text = format!(
                    "Warmup - CPU Temp: {:.0}°C -> Target: >={:.0}°C ({}s)",
                    current_temp, warmup_target, remaining_secs
                );
                on_status(status_text);

                if current_temp >= warmup_target {
                    let done_msg = format!("[BenchHub] Warmup completed ({:.1}°C)\n", current_temp);
                    on_log(done_msg);
                    break;
                }

                tokio::select! {
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() {
                            success = false;
                            break;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(400)) => {}
                }
            }

            // Stop background stress threads
            is_running.store(false, Ordering::Relaxed);
            for h in handles {
                let _ = h.join();
            }

            if success && start.elapsed() >= max_duration && get_temp() < warmup_target {
                let timeout_msg = format!(
                    "[BenchHub] WARNING: Warmup timed out ({:.1}°C)\n",
                    get_temp()
                );
                on_log(timeout_msg);
            }

            success
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thermal_mode_parsing() {
        assert_eq!(
            ThermalConditioningMode::parse_mode("off"),
            ThermalConditioningMode::Off
        );
        assert_eq!(
            ThermalConditioningMode::parse_mode("cooldown"),
            ThermalConditioningMode::Cooldown
        );
        assert_eq!(
            ThermalConditioningMode::parse_mode("warmup"),
            ThermalConditioningMode::Warmup
        );
        assert_eq!(
            ThermalConditioningMode::parse_mode("smart"),
            ThermalConditioningMode::Smart
        );
        assert_eq!(
            ThermalConditioningMode::parse_mode("unknown"),
            ThermalConditioningMode::Off
        );
    }
}
