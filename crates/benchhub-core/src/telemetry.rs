use crate::models::{GpuMode, TelemetryData};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::time::sleep;

#[repr(C)]
struct NvmlUtilization {
    gpu: u32,
    memory: u32,
}

#[repr(C)]
struct NvmlMemoryInfo {
    total: u64,
    free: u64,
    used: u64,
}

pub struct NvmlContext {
    _handle: *mut libc::c_void,
    device: *mut libc::c_void,
    get_power: unsafe extern "C" fn(*mut libc::c_void, *mut u32) -> i32,
    get_temp: unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32,
    get_util: unsafe extern "C" fn(*mut libc::c_void, *mut NvmlUtilization) -> i32,
    get_clock: Option<unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32>,
    get_mem_info: Option<unsafe extern "C" fn(*mut libc::c_void, *mut NvmlMemoryInfo) -> i32>,
    get_throttle: Option<unsafe extern "C" fn(*mut libc::c_void, *mut u64) -> i32>,
    shutdown: unsafe extern "C" fn() -> i32,
}

unsafe impl Send for NvmlContext {}
unsafe impl Sync for NvmlContext {}

impl Drop for NvmlContext {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.shutdown)();
            if !self._handle.is_null() {
                libc::dlclose(self._handle);
            }
        }
    }
}

impl NvmlContext {
    pub fn init() -> Option<Self> {
        unsafe {
            let lib_name = std::ffi::CString::new("libnvidia-ml.so.1").ok()?;
            let handle = libc::dlopen(lib_name.as_ptr(), libc::RTLD_NOW);
            if handle.is_null() {
                return None;
            }

            let sym_init =
                libc::dlsym(handle, std::ffi::CString::new("nvmlInit_v2").ok()?.as_ptr());
            let sym_shutdown = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlShutdown").ok()?.as_ptr(),
            );
            let sym_get_handle = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetHandleByIndex_v2")
                    .ok()?
                    .as_ptr(),
            );
            let sym_get_power = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetPowerUsage")
                    .ok()?
                    .as_ptr(),
            );
            let sym_get_temp = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetTemperature")
                    .ok()?
                    .as_ptr(),
            );
            let sym_get_util = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetUtilizationRates")
                    .ok()?
                    .as_ptr(),
            );
            let sym_get_clock = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetClockInfo")
                    .ok()?
                    .as_ptr(),
            );
            let sym_get_mem = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetMemoryInfo")
                    .ok()?
                    .as_ptr(),
            );
            let sym_get_throttle = libc::dlsym(
                handle,
                std::ffi::CString::new("nvmlDeviceGetCurrentClocksThrottleReasons")
                    .ok()?
                    .as_ptr(),
            );

            if sym_init.is_null()
                || sym_shutdown.is_null()
                || sym_get_handle.is_null()
                || sym_get_power.is_null()
                || sym_get_temp.is_null()
                || sym_get_util.is_null()
            {
                libc::dlclose(handle);
                return None;
            }

            let init_fn: unsafe extern "C" fn() -> i32 = std::mem::transmute(sym_init);
            let shutdown_fn: unsafe extern "C" fn() -> i32 = std::mem::transmute(sym_shutdown);
            let get_handle_fn: unsafe extern "C" fn(u32, *mut *mut libc::c_void) -> i32 =
                std::mem::transmute(sym_get_handle);
            let get_power_fn: unsafe extern "C" fn(*mut libc::c_void, *mut u32) -> i32 =
                std::mem::transmute(sym_get_power);
            let get_temp_fn: unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32 =
                std::mem::transmute(sym_get_temp);
            let get_util_fn: unsafe extern "C" fn(*mut libc::c_void, *mut NvmlUtilization) -> i32 =
                std::mem::transmute(sym_get_util);
            let get_clock_fn: Option<
                unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32,
            > = if !sym_get_clock.is_null() {
                Some(std::mem::transmute::<
                    *mut libc::c_void,
                    unsafe extern "C" fn(*mut libc::c_void, u32, *mut u32) -> i32,
                >(sym_get_clock))
            } else {
                None
            };
            let get_mem_fn: Option<
                unsafe extern "C" fn(*mut libc::c_void, *mut NvmlMemoryInfo) -> i32,
            > = if !sym_get_mem.is_null() {
                Some(std::mem::transmute::<
                    *mut libc::c_void,
                    unsafe extern "C" fn(*mut libc::c_void, *mut NvmlMemoryInfo) -> i32,
                >(sym_get_mem))
            } else {
                None
            };
            let get_throttle_fn: Option<unsafe extern "C" fn(*mut libc::c_void, *mut u64) -> i32> =
                if !sym_get_throttle.is_null() {
                    Some(std::mem::transmute::<
                        *mut libc::c_void,
                        unsafe extern "C" fn(*mut libc::c_void, *mut u64) -> i32,
                    >(sym_get_throttle))
                } else {
                    None
                };

            if init_fn() != 0 {
                libc::dlclose(handle);
                return None;
            }

            let mut device: *mut libc::c_void = std::ptr::null_mut();
            if get_handle_fn(0, &mut device) != 0 || device.is_null() {
                let _ = shutdown_fn();
                libc::dlclose(handle);
                return None;
            }

            Some(Self {
                _handle: handle,
                device,
                get_power: get_power_fn,
                get_temp: get_temp_fn,
                get_util: get_util_fn,
                get_clock: get_clock_fn,
                get_mem_info: get_mem_fn,
                get_throttle: get_throttle_fn,
                shutdown: shutdown_fn,
            })
        }
    }

    pub fn sample_power_w(&self) -> Option<f32> {
        let mut mw: u32 = 0;
        unsafe {
            if (self.get_power)(self.device, &mut mw) == 0 {
                // NVML returns milliwatts (mW) -> divide by 1000.0 to get Watts (W)
                let w = (mw as f32) / 1000.0;
                // Laptop dGPU is typically 10-175W. Ignore invalid spikes / error codes > 500W
                if w > 0.0 && w < 500.0 {
                    return Some(w);
                }
            }
        }
        None
    }

    pub fn sample_temperature(&self) -> Option<f32> {
        let mut temp: u32 = 0;
        unsafe {
            if (self.get_temp)(self.device, 0, &mut temp) == 0 {
                let t = temp as f32;
                if t > 0.0 && t < 140.0 {
                    return Some(t);
                }
            }
        }
        None
    }

    pub fn sample_utilization(&self) -> Option<f32> {
        let mut util = NvmlUtilization { gpu: 0, memory: 0 };
        unsafe {
            if (self.get_util)(self.device, &mut util) == 0 {
                return Some(util.gpu as f32);
            }
        }
        None
    }

    pub fn sample_gpu_freq_mhz(&self) -> Option<f32> {
        if let Some(get_clock) = self.get_clock {
            let mut clock_mhz: u32 = 0;
            unsafe {
                // NVML_CLOCK_GRAPHICS = 0
                if get_clock(self.device, 0, &mut clock_mhz) == 0 && clock_mhz > 0 {
                    return Some(clock_mhz as f32);
                }
            }
        }
        None
    }

    pub fn sample_vram_freq_mhz(&self) -> Option<f32> {
        if let Some(get_clock) = self.get_clock {
            let mut clock_mhz: u32 = 0;
            unsafe {
                // NVML_CLOCK_MEM = 2
                if get_clock(self.device, 2, &mut clock_mhz) == 0 && clock_mhz > 0 {
                    return Some(clock_mhz as f32);
                }
            }
        }
        None
    }

    pub fn sample_vram_usage_mb(&self) -> Option<f32> {
        if let Some(get_mem) = self.get_mem_info {
            let mut mem_info = NvmlMemoryInfo {
                total: 0,
                free: 0,
                used: 0,
            };
            unsafe {
                if get_mem(self.device, &mut mem_info) == 0 {
                    return Some((mem_info.used as f32) / 1024.0 / 1024.0);
                }
            }
        }
        None
    }

    pub fn sample_throttle_reason(&self) -> Option<String> {
        if let Some(get_throttle) = self.get_throttle {
            let mut reasons: u64 = 0;
            unsafe {
                if get_throttle(self.device, &mut reasons) == 0 {
                    if (reasons & 0x0060) != 0 {
                        return Some("thermal_throttle".to_string());
                    }
                    if (reasons & 0x0084) != 0 {
                        return Some("power_limit".to_string());
                    }
                    if (reasons & 0x0008) != 0 {
                        return Some("hardware_throttle".to_string());
                    }
                    return Some("none".to_string());
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct BatterySensor {
    pub power_path: PathBuf,
    pub status_path: PathBuf,
}

fn sorted_dir_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    paths
}

fn read_sysfs_f32(path: &Path) -> Option<f32> {
    fs::read_to_string(path).ok()?.trim().parse::<f32>().ok()
}

/// Returns true if a `/sys/class/power_supply/*` entry belongs to a peripheral device
/// (e.g., mouse/keyboard `hidpp_battery_*`) rather than the laptop's main battery.
pub fn is_non_system_power_supply(name: &str, scope: Option<&str>) -> bool {
    if let Some(s) = scope {
        if s.trim().eq_ignore_ascii_case("device") {
            return true;
        }
    }
    let lower = name.to_ascii_lowercase();
    lower.starts_with("hid")
        || lower.contains("wacom")
        || lower.contains("controller")
        || lower.contains("headset")
}

/// Returns true only when the battery status indicates active discharging.
pub fn is_battery_discharging(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("discharging")
}

pub struct SensorCache {
    pub cpu_power_paths: Vec<PathBuf>,
    pub gpu_power_paths: Vec<PathBuf>,
    pub apu_power_paths: Vec<PathBuf>,
    pub battery_power_paths: Vec<PathBuf>,
    pub battery_sensors: Vec<BatterySensor>,
    pub ac_online_path: Option<PathBuf>,
    pub ac_online_paths: Vec<PathBuf>,
    pub cpu_temp_paths: Vec<PathBuf>,
    pub gpu_temp_paths: Vec<PathBuf>,
    pub gpu_usage_paths: Vec<PathBuf>,
    pub gpu_freq_paths: Vec<PathBuf>,
    pub vram_freq_paths: Vec<PathBuf>,
    pub vram_used_paths: Vec<PathBuf>,
    pub processor_cooling_cur_states: Vec<PathBuf>,
    pub nvidia_pci_runtime_status_paths: Vec<PathBuf>,
}

impl Default for SensorCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorCache {
    pub fn new() -> Self {
        let mut cache = Self {
            cpu_power_paths: Vec::new(),
            gpu_power_paths: Vec::new(),
            apu_power_paths: Vec::new(),
            battery_power_paths: Vec::new(),
            battery_sensors: Vec::new(),
            ac_online_path: None,
            ac_online_paths: Vec::new(),
            cpu_temp_paths: Vec::new(),
            gpu_temp_paths: Vec::new(),
            gpu_usage_paths: Vec::new(),
            gpu_freq_paths: Vec::new(),
            vram_freq_paths: Vec::new(),
            vram_used_paths: Vec::new(),
            processor_cooling_cur_states: Vec::new(),
            nvidia_pci_runtime_status_paths: Vec::new(),
        };
        cache.discover();
        cache
    }

    pub fn is_nvidia_pci_suspended(&self) -> bool {
        if self.nvidia_pci_runtime_status_paths.is_empty() {
            return false;
        }
        self.nvidia_pci_runtime_status_paths.iter().all(|p| {
            fs::read_to_string(p)
                .map(|s| s.trim().eq_ignore_ascii_case("suspended"))
                .unwrap_or(false)
        })
    }

    pub fn discover(&mut self) {
        let mut primary_cpu_temps = Vec::new();
        let mut fallback_cpu_temps = Vec::new();
        let has_rapl = find_rapl_energy_path().is_some();
        let mut saw_intel_cpu = false;

        for p in sorted_dir_paths(Path::new("/sys/class/hwmon")) {
            let name = fs::read_to_string(p.join("name"))
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            if name.contains("coretemp") {
                saw_intel_cpu = true;
            }
            let is_primary_cpu =
                name.contains("k10temp") || name.contains("coretemp") || name.contains("zenpower");
            let is_cpu = is_primary_cpu || name.contains("cpu") || name.contains("soc_thermal");
            let is_apu = name.contains("amdgpu");
            let is_discrete_gpu = name.contains("nouveau")
                || name.contains("nvidia")
                || name.contains("i915")
                || name.contains("xe");

            // Read power, temp, and freq files in deterministic sorted order
            let mut found_avg = None;
            let mut found_input = None;

            for file_path in sorted_dir_paths(&p) {
                let fname = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if fname.starts_with("power") {
                    if fname.ends_with("_average") && found_avg.is_none() {
                        found_avg = Some(file_path.clone());
                    } else if fname.ends_with("_input") && found_input.is_none() {
                        found_input = Some(file_path.clone());
                    }
                }
                if fname.starts_with("temp") && fname.ends_with("_input") {
                    if is_primary_cpu {
                        primary_cpu_temps.push(file_path.clone());
                    } else if is_cpu {
                        fallback_cpu_temps.push(file_path.clone());
                    } else if is_apu || is_discrete_gpu {
                        self.gpu_temp_paths.push(file_path.clone());
                    }
                }
                if is_apu || is_discrete_gpu {
                    if fname == "freq1_input" {
                        self.gpu_freq_paths.push(file_path.clone());
                    } else if fname == "freq2_input" {
                        self.vram_freq_paths.push(file_path.clone());
                    }
                }
            }

            // Prevent double-counting: pick only ONE power file per hwmon device
            if let Some(path) = found_avg.or(found_input) {
                if is_cpu {
                    self.cpu_power_paths.push(path);
                } else if is_apu {
                    let has_fan_control = p.join("fan1_input").exists() || p.join("pwm1").exists();
                    if has_fan_control || has_rapl || saw_intel_cpu {
                        self.gpu_power_paths.push(path);
                    } else {
                        self.apu_power_paths.push(path);
                    }
                } else if is_discrete_gpu {
                    self.gpu_power_paths.push(path);
                }
            }
        }

        self.cpu_temp_paths.extend(primary_cpu_temps);
        self.cpu_temp_paths.extend(fallback_cpu_temps);

        // Ensure AMD GPUs always expose gpu_power_w when no other discrete GPU hwmon power path exists
        if self.gpu_power_paths.is_empty() && !self.apu_power_paths.is_empty() {
            self.gpu_power_paths.extend(self.apu_power_paths.clone());
        }

        // On AMD APU platforms (e.g. Ryzen 7840HS) without RAPL or Intel CPU sensors,
        // amdgpu power represents APU package power (CPU+SoC) when no dedicated CPU power path exists
        if self.cpu_power_paths.is_empty()
            && !self.apu_power_paths.is_empty()
            && !has_rapl
            && !saw_intel_cpu
        {
            self.cpu_power_paths.extend(self.apu_power_paths.clone());
        }

        for p in sorted_dir_paths(Path::new("/sys/class/power_supply")) {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let scope = fs::read_to_string(p.join("scope")).ok();
            if is_non_system_power_supply(name, scope.as_deref()) {
                continue;
            }
            let ptype = fs::read_to_string(p.join("type")).unwrap_or_default();
            let is_battery = ptype.trim().eq_ignore_ascii_case("battery")
                || name.starts_with("BAT")
                || name.contains("battery");
            let is_mains = ptype.trim().eq_ignore_ascii_case("mains")
                || name.starts_with("AC")
                || name.starts_with("ADP")
                || name.contains("mains");

            if is_battery {
                let power_path = p.join("power_now");
                if power_path.exists() {
                    self.battery_power_paths.push(power_path.clone());
                    self.battery_sensors.push(BatterySensor {
                        power_path,
                        status_path: p.join("status"),
                    });
                }
            } else if is_mains {
                let online_path = p.join("online");
                if online_path.exists() {
                    if self.ac_online_path.is_none() {
                        self.ac_online_path = Some(online_path.clone());
                    }
                    self.ac_online_paths.push(online_path);
                }
            }
        }

        for p in sorted_dir_paths(Path::new("/sys/class/thermal")) {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("thermal_zone") && self.cpu_temp_paths.is_empty() {
                let tz_type = fs::read_to_string(p.join("type"))
                    .unwrap_or_default()
                    .to_lowercase();
                if tz_type.contains("cpu") || tz_type.contains("acpi") || tz_type.contains("pkg") {
                    self.cpu_temp_paths.push(p.join("temp"));
                }
            } else if name.starts_with("cooling_device") {
                let c_type = fs::read_to_string(p.join("type"))
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase();
                if c_type == "processor" {
                    let cur_state_p = p.join("cur_state");
                    if cur_state_p.exists() {
                        self.processor_cooling_cur_states.push(cur_state_p);
                    }
                }
            }
        }

        for p in sorted_dir_paths(Path::new("/sys/class/drm")) {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("card") && !name.contains('-') {
                let busy_p = p.join("device/gpu_busy_percent");
                if busy_p.exists() {
                    self.gpu_usage_paths.push(busy_p);
                }

                for rel in [
                    "device/pp_dpm_sclk",
                    "device/current_gfxclk",
                    "gt_act_freq_mhz",
                    "gt_cur_freq_mhz",
                ] {
                    let candidate = p.join(rel);
                    if candidate.exists() {
                        self.gpu_freq_paths.push(candidate);
                    }
                }

                let mclk_p = p.join("device/pp_dpm_mclk");
                if mclk_p.exists() {
                    self.vram_freq_paths.push(mclk_p);
                }

                let vram_used_p = p.join("device/mem_info_vram_used");
                if vram_used_p.exists() {
                    self.vram_used_paths.push(vram_used_p);
                }
            }
        }

        // Discover PCI runtime PM status nodes for NVIDIA display/3D adapters
        for p in sorted_dir_paths(Path::new("/sys/bus/pci/devices")) {
            let vendor = fs::read_to_string(p.join("vendor")).unwrap_or_default();
            let class = fs::read_to_string(p.join("class")).unwrap_or_default();
            if vendor.trim().eq_ignore_ascii_case("0x10de")
                && class.trim().to_ascii_lowercase().starts_with("0x03")
            {
                let status_p = p.join("power/runtime_status");
                if status_p.exists() {
                    self.nvidia_pci_runtime_status_paths.push(status_p);
                }
            }
        }
    }
}

pub fn parse_frequency_sysfs(path: &Path) -> Option<f32> {
    if let Ok(content) = fs::read_to_string(path) {
        // Case 1: pp_dpm_sclk / pp_dpm_mclk containing multiple lines, current one has '*' e.g. "1: 1500Mhz *" or "1: 1500MHz *"
        if content.contains('*') {
            for line in content.lines() {
                if line.contains('*') {
                    for token in line.split_whitespace() {
                        let clean = token
                            .trim_end_matches('*')
                            .trim_end_matches("Mhz")
                            .trim_end_matches("MHz")
                            .trim_end_matches(':');
                        if let Ok(mhz) = clean.parse::<f32>() {
                            if mhz > 0.0 && mhz < 10000.0 {
                                return Some(mhz);
                            }
                        }
                    }
                }
            }
        }

        // Case 2: Direct single value (e.g. gt_act_freq_mhz, current_gfxclk, freq1_input)
        if let Ok(val) = content.trim().parse::<f32>() {
            if val > 100_000.0 {
                // Likely in Hz (hwmon freq_input) -> convert to MHz
                let mhz = val / 1_000_000.0;
                if mhz > 0.0 && mhz < 10000.0 {
                    return Some(mhz);
                }
            } else if val > 10_000.0 {
                // Likely in kHz -> convert to MHz
                let mhz = val / 1000.0;
                if mhz > 0.0 && mhz < 10000.0 {
                    return Some(mhz);
                }
            } else if val > 0.0 && val < 10000.0 {
                // Directly in MHz
                return Some(val);
            }
        }
    }
    None
}

pub struct TelemetryEngine {
    sys: System,
    last_energy_uj: Option<u64>,
    last_energy_time: Option<Instant>,
    rapl_path: Option<PathBuf>,
    sensor_cache: SensorCache,
    nvml: Option<NvmlContext>,
}

impl Default for TelemetryEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryEngine {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_cpu_frequency();
        sys.refresh_memory();
        let rapl_path = find_rapl_energy_path();
        // Prime the initial RAPL baseline so even the very first sample computes valid power.
        let (last_energy_uj, last_energy_time) = match rapl_path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            Some(uj) => (Some(uj), Some(Instant::now())),
            None => (None, None),
        };
        let nvml = NvmlContext::init();
        Self {
            sys,
            last_energy_uj,
            last_energy_time,
            rapl_path,
            sensor_cache: SensorCache::new(),
            nvml,
        }
    }

    /// Creates a `TelemetryEngine` backed by the provided `SensorCache` (primarily for deterministic tests).
    pub fn with_sensor_cache(sensor_cache: SensorCache) -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_cpu_frequency();
        sys.refresh_memory();
        Self {
            sys,
            last_energy_uj: None,
            last_energy_time: None,
            rapl_path: None,
            sensor_cache,
            nvml: None,
        }
    }

    /// Returns mutable access to discovered sysfs sensor paths.
    pub fn sensor_cache_mut(&mut self) -> &mut SensorCache {
        &mut self.sensor_cache
    }

    fn has_igpu_sysfs_sensors(&self) -> bool {
        !self.sensor_cache.gpu_usage_paths.is_empty()
            || !self.sensor_cache.gpu_temp_paths.is_empty()
    }

    fn resolve_use_nvml(&self, gpu_mode: GpuMode) -> bool {
        let Some(nvml) = &self.nvml else {
            return false;
        };
        match gpu_mode {
            GpuMode::NvidiaDgpu => true,
            GpuMode::Integrated => false,
            GpuMode::Auto => {
                if self.has_igpu_sysfs_sensors() {
                    if self.sensor_cache.is_nvidia_pci_suspended() {
                        return false;
                    }
                    let dgpu_util = nvml.sample_utilization().unwrap_or(0.0);
                    let igpu_util = self.sample_sysfs_gpu_usage();
                    if dgpu_util <= 1.0 && igpu_util > 5.0 {
                        return false;
                    }
                }
                true
            }
        }
    }

    fn sample_sysfs_gpu_usage(&self) -> f32 {
        for p in &self.sensor_cache.gpu_usage_paths {
            if let Some(val) = read_sysfs_f32(p) {
                return val.clamp(0.0, 100.0);
            }
        }
        0.0
    }

    pub fn sample_gpu_usage(&self) -> f32 {
        self.sample_gpu_usage_for(self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_gpu_usage_for(&self, use_nvml: bool) -> f32 {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(util) = nvml.sample_utilization() {
                    return util.clamp(0.0, 100.0);
                }
            }
        }
        if use_nvml && self.nvml.is_some() {
            return 0.0;
        }
        self.sample_sysfs_gpu_usage()
    }

    pub fn sample_gpu_temp(&self) -> f32 {
        self.sample_gpu_temp_for(self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_gpu_temp_for(&self, use_nvml: bool) -> f32 {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(temp) = nvml.sample_temperature() {
                    return temp;
                }
                return 0.0;
            }
        }
        for p in &self.sensor_cache.gpu_temp_paths {
            if let Some(val) = read_sysfs_f32(p) {
                let temp = val / 1000.0;
                if temp > 0.0 && temp < 140.0 {
                    return temp;
                }
            }
        }
        0.0
    }

    pub fn sample_gpu_freq_mhz(&self) -> f32 {
        self.sample_gpu_freq_mhz_for(self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_gpu_freq_mhz_for(&self, use_nvml: bool) -> f32 {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(mhz) = nvml.sample_gpu_freq_mhz() {
                    return mhz;
                }
                return 0.0;
            }
        }
        for p in &self.sensor_cache.gpu_freq_paths {
            if let Some(mhz) = parse_frequency_sysfs(p) {
                return mhz;
            }
        }
        0.0
    }

    pub fn sample_vram_freq_mhz(&self) -> f32 {
        self.sample_vram_freq_mhz_for(self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_vram_freq_mhz_for(&self, use_nvml: bool) -> f32 {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(mhz) = nvml.sample_vram_freq_mhz() {
                    return mhz;
                }
                return 0.0;
            }
        }
        for p in &self.sensor_cache.vram_freq_paths {
            if let Some(mhz) = parse_frequency_sysfs(p) {
                return mhz;
            }
        }
        0.0
    }

    pub fn sample_vram_usage_mb(&self) -> f32 {
        self.sample_vram_usage_mb_for(self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_vram_usage_mb_for(&self, use_nvml: bool) -> f32 {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(mb) = nvml.sample_vram_usage_mb() {
                    return mb;
                }
                return 0.0;
            }
        }
        for p in &self.sensor_cache.vram_used_paths {
            if let Some(bytes) = read_sysfs_f32(p) {
                let mb = bytes / 1024.0 / 1024.0;
                if (0.0..1_000_000.0).contains(&mb) {
                    return mb;
                }
            }
        }
        0.0
    }

    pub fn sample_gpu_throttling(&self, gpu_temp: f32) -> String {
        self.sample_gpu_throttling_for(gpu_temp, self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_gpu_throttling_for(&self, gpu_temp: f32, use_nvml: bool) -> String {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(reason) = nvml.sample_throttle_reason() {
                    return reason;
                }
            }
        }
        if gpu_temp >= 87.0 {
            "thermal_throttle".to_string()
        } else {
            "none".to_string()
        }
    }

    pub fn sample_cpu_throttling(&self, cpu_temp: f32) -> String {
        for p in &self.sensor_cache.processor_cooling_cur_states {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(state) = s.trim().parse::<u32>() {
                    if state > 0 {
                        return "thermal_throttle".to_string();
                    }
                }
            }
        }
        if cpu_temp >= 93.0 {
            "thermal_throttle".to_string()
        } else {
            "none".to_string()
        }
    }

    pub fn read_current(&mut self) -> TelemetryData {
        self.read_current_for_mode(GpuMode::Auto)
    }

    pub fn read_current_for_mode(&mut self, gpu_mode: GpuMode) -> TelemetryData {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_cpu_frequency();
        self.sys.refresh_memory();

        let mut cpu_freq = if !self.sys.cpus().is_empty() {
            let sum_freq: u64 = self.sys.cpus().iter().map(|c| c.frequency()).sum();
            (sum_freq as f32) / (self.sys.cpus().len() as f32)
        } else {
            0.0
        };

        if cpu_freq <= 0.0 {
            if let Some(fallback_freq) = read_cpu_freq_fallback() {
                cpu_freq = fallback_freq;
            }
        }

        let mut cpu_temp = 0.0;
        for p in &self.sensor_cache.cpu_temp_paths {
            if let Some(val) = read_sysfs_f32(p) {
                let temp = val / 1000.0;
                if temp > 0.0 && temp < 140.0 {
                    cpu_temp = temp;
                    break;
                }
            }
        }

        let use_nvml = self.resolve_use_nvml(gpu_mode);
        let gpu_temp = self.sample_gpu_temp_for(use_nvml);
        let cpu_usage = self.sys.global_cpu_info().cpu_usage();
        let gpu_usage = self.sample_gpu_usage_for(use_nvml);
        let gpu_freq_mhz = self.sample_gpu_freq_mhz_for(use_nvml);
        let vram_freq_mhz = self.sample_vram_freq_mhz_for(use_nvml);
        let vram_usage_mb = self.sample_vram_usage_mb_for(use_nvml);
        let power_w = self.sample_power_w().unwrap_or(0.0);
        let gpu_power_w = self.sample_gpu_power_w_for(use_nvml);
        let ac_power_w = self.sample_ac_power_w(power_w, gpu_power_w);
        let ram_usage_mb = (self.sys.used_memory() as f32) / 1024.0 / 1024.0;
        let cpu_throttle = self.sample_cpu_throttling(cpu_temp);
        let gpu_throttle = self.sample_gpu_throttling_for(gpu_temp, use_nvml);

        TelemetryData {
            timestamp: Utc::now().timestamp_millis(),
            cpu_temp,
            gpu_temp,
            cpu_usage,
            gpu_usage,
            cpu_freq,
            gpu_freq_mhz,
            vram_freq_mhz,
            power_w,
            gpu_power_w,
            ac_power_w,
            ram_usage_mb,
            vram_usage_mb,
            cpu_throttle,
            gpu_throttle,
        }
    }

    pub fn sample_power_w(&mut self) -> Option<f32> {
        if let Some(path) = &self.rapl_path {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(current_uj) = content.trim().parse::<u64>() {
                    let now = Instant::now();
                    let power = if let (Some(last_uj), Some(last_time)) =
                        (self.last_energy_uj, self.last_energy_time)
                    {
                        let elapsed = now.duration_since(last_time).as_secs_f32();
                        if elapsed > 0.001 {
                            if current_uj >= last_uj {
                                calculate_power_watts(current_uj - last_uj, elapsed)
                            } else {
                                let max_range = path
                                    .parent()
                                    .map(|p| p.join("max_energy_range_uj"))
                                    .and_then(|p| fs::read_to_string(p).ok())
                                    .and_then(|s| s.trim().parse::<u64>().ok());
                                if let Some(max_range) = max_range {
                                    if max_range > last_uj {
                                        let delta_uj =
                                            (max_range - last_uj).saturating_add(current_uj);
                                        calculate_power_watts(delta_uj, elapsed)
                                    } else {
                                        0.0
                                    }
                                } else {
                                    0.0
                                }
                            }
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    self.last_energy_uj = Some(current_uj);
                    self.last_energy_time = Some(now);
                    if power.is_finite() && (0.0..5000.0).contains(&power) {
                        return Some(power);
                    }
                }
            }
        }

        let mut total_power_w = 0.0;
        let mut found = false;
        for p in &self.sensor_cache.cpu_power_paths {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(uw) = s.trim().parse::<f32>() {
                    if uw > 0.0 {
                        // Hwmon returns microwatts (µW) -> divide by 1,000,000.0 to get Watts (W)
                        let w = uw / 1_000_000.0;
                        if w < 500.0 {
                            total_power_w += w;
                            found = true;
                        }
                    }
                }
            }
        }
        if found {
            return Some(total_power_w);
        }

        None
    }

    pub fn sample_gpu_power_w(&self) -> f32 {
        self.sample_gpu_power_w_for(self.resolve_use_nvml(GpuMode::Auto))
    }

    fn sample_gpu_power_w_for(&self, use_nvml: bool) -> f32 {
        if use_nvml {
            if let Some(nvml) = &self.nvml {
                if let Some(w) = nvml.sample_power_w() {
                    if w > 0.0 && w < 500.0 {
                        return w;
                    }
                }
                return 0.0;
            }
        }

        // Try Hwmon for AMD / Intel discrete GPU (µW -> W)
        let mut total_w = 0.0;
        for p in &self.sensor_cache.gpu_power_paths {
            if let Some(uw) = read_sysfs_f32(p) {
                if uw > 0.0 {
                    let w = uw / 1_000_000.0;
                    if w < 500.0 {
                        total_w += w;
                    }
                }
            }
        }

        total_w
    }

    pub fn sample_ac_power_w(&self, cpu_power: f32, gpu_power: f32) -> f32 {
        // 1. Only trust battery power_now when that battery is actively Discharging;
        // while Charging on AC mains, power_now reports battery charge rate rather than system draw.
        for sensor in &self.sensor_cache.battery_sensors {
            let Ok(status) = fs::read_to_string(&sensor.status_path) else {
                continue;
            };
            if !is_battery_discharging(&status) {
                continue;
            }
            if let Some(uw) = read_sysfs_f32(&sensor.power_path) {
                let w = uw / 1_000_000.0;
                if w > 0.0 && w < 500.0 {
                    return w;
                }
            }
        }

        // 2. If plugged in to AC Mains (or online), estimate total wall/AC draw
        let is_ac_online = if !self.sensor_cache.ac_online_paths.is_empty() {
            self.sensor_cache
                .ac_online_paths
                .iter()
                .filter_map(|p| fs::read_to_string(p).ok())
                .any(|s| s.trim() == "1")
        } else {
            self.sensor_cache
                .ac_online_path
                .as_ref()
                .and_then(|p| fs::read_to_string(p).ok())
                .map(|s| s.trim() == "1")
                .unwrap_or(true)
        };

        if is_ac_online && (cpu_power > 0.0 || gpu_power > 0.0) {
            // CPU + GPU power + ~12W base system load (display, RAM, motherboard, fans)
            return cpu_power + gpu_power + 12.0;
        }

        0.0
    }
}

pub fn read_cpu_freq_fallback() -> Option<f32> {
    let cpu_dir = Path::new("/sys/devices/system/cpu");
    if !cpu_dir.exists() {
        return None;
    }
    let mut freqs = Vec::new();
    if let Ok(entries) = fs::read_dir(cpu_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) {
                let freq_path = entry.path().join("cpufreq/scaling_cur_freq");
                if let Ok(s) = fs::read_to_string(freq_path) {
                    if let Ok(khz) = s.trim().parse::<f32>() {
                        freqs.push(khz / 1000.0);
                    }
                }
            }
        }
    }
    if !freqs.is_empty() {
        Some(freqs.iter().sum::<f32>() / (freqs.len() as f32))
    } else {
        None
    }
}

pub fn find_rapl_energy_path() -> Option<PathBuf> {
    for p in sorted_dir_paths(Path::new("/sys/class/powercap")) {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.starts_with("intel-rapl") {
            let energy_file = p.join("energy_uj");
            if energy_file.exists() && fs::read_to_string(&energy_file).is_ok() {
                return Some(energy_file);
            }
        }
    }
    None
}

pub fn calculate_power_watts(delta_uj: u64, elapsed_seconds: f32) -> f32 {
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }
    (delta_uj as f32 / 1_000_000.0) / elapsed_seconds
}

pub fn get_sensor_diagnosis() -> String {
    let engine = TelemetryEngine::new();
    let mut parts = Vec::new();
    if !engine.sensor_cache.cpu_temp_paths.is_empty() {
        parts.push(format!(
            "{} CPU Temperature Sensor(s)",
            engine.sensor_cache.cpu_temp_paths.len()
        ));
    } else {
        parts.push("No CPU Temp Sensor".to_string());
    }
    if engine.nvml.is_some() {
        parts.push("NVIDIA NVML (dGPU) Active".to_string());
    } else if !engine.sensor_cache.gpu_temp_paths.is_empty() {
        parts.push(format!(
            "{} GPU Sensor(s)",
            engine.sensor_cache.gpu_temp_paths.len()
        ));
    }
    if engine.nvml.is_some() || !engine.sensor_cache.gpu_power_paths.is_empty() {
        parts.push("GPU Power Sensor Active (W)".to_string());
    }
    if engine.rapl_path.is_some() {
        parts.push("Intel RAPL Energy/Power Active".to_string());
    } else if !engine.sensor_cache.cpu_power_paths.is_empty() {
        parts.push("CPU Power Sensor Active".to_string());
    } else if !engine.sensor_cache.battery_power_paths.is_empty() {
        parts.push("Battery / AC Power Sensor Active".to_string());
    } else {
        parts.push("No Power Sensor Found".to_string());
    }
    parts.join("  •  ")
}

pub fn get_system_info_summary() -> String {
    let mut sys = System::new();
    sys.refresh_cpu_usage();

    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if let Ok(info) = fs::read_to_string("/proc/cpuinfo") {
                for line in info.lines() {
                    if line.starts_with("model name") {
                        if let Some((_, val)) = line.split_once(':') {
                            return val.trim().to_string();
                        }
                    }
                }
            }
            "CPU".to_string()
        });

    let mut gpu_names = Vec::new();
    let drm_dir = Path::new("/sys/class/drm");
    if drm_dir.exists() {
        if let Ok(entries) = fs::read_dir(drm_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("card") && !name.contains('-') {
                    let uevent_path = entry.path().join("device/uevent");
                    if let Ok(uevent) = fs::read_to_string(uevent_path) {
                        for line in uevent.lines() {
                            if line.starts_with("DRIVER=") {
                                let driver = line.trim_start_matches("DRIVER=").trim();
                                let clean_gpu = match driver {
                                    "nvidia" => "NVIDIA GPU",
                                    "amdgpu" => "AMD Radeon GPU",
                                    "i915" => "Intel Graphics",
                                    "xe" => "Intel Arc GPU",
                                    "nouveau" => "NVIDIA (Nouveau)",
                                    _ => driver,
                                };
                                if !gpu_names.contains(&clean_gpu.to_string()) {
                                    gpu_names.push(clean_gpu.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let gpu_summary = if !gpu_names.is_empty() {
        gpu_names.join(" + ")
    } else {
        "Standard GPU".to_string()
    };

    let kernel = System::kernel_version().unwrap_or_default();
    let os = System::name().unwrap_or_else(|| "Linux".to_string());

    if kernel.is_empty() {
        format!("{} | {} | {}", cpu_name, gpu_summary, os)
    } else {
        format!("{} | {} | {} {}", cpu_name, gpu_summary, os, kernel)
    }
}

pub fn get_detected_gpu_names() -> (Option<String>, Option<String>) {
    let mut dgpu = None;
    let mut igpu = None;

    if let Ok(entries) = fs::read_dir("/proc/driver/nvidia/gpus") {
        for entry in entries.flatten() {
            let info_path = entry.path().join("information");
            if let Ok(content) = fs::read_to_string(info_path) {
                for line in content.lines() {
                    if line.starts_with("Model:") {
                        let model = line.trim_start_matches("Model:").trim();
                        if !model.is_empty() {
                            dgpu = Some(model.to_string());
                            break;
                        }
                    }
                }
            }
            if dgpu.is_some() {
                break;
            }
        }
    }

    if let Ok(output) = std::process::Command::new("lspci").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let line_lower = line.to_lowercase();
            if line_lower.contains("vga compatible controller")
                || line_lower.contains("3d controller")
                || line_lower.contains("display controller")
            {
                if line.contains("NVIDIA") {
                    if dgpu.is_none() {
                        let name = if let Some(start) = line.find('[') {
                            if let Some(end) = line.find(']') {
                                &line[start + 1..end]
                            } else {
                                line
                            }
                        } else {
                            line
                        };
                        dgpu = Some(format!("NVIDIA {}", name.trim()));
                    }
                } else if line.contains("AMD") || line.contains("Advanced Micro Devices") {
                    if igpu.is_none() {
                        let name = if let Some(start) = line.find('[') {
                            if let Some(end) = line.find(']') {
                                &line[start + 1..end]
                            } else {
                                line
                            }
                        } else {
                            line
                        };
                        let clean = name
                            .replace("AMD/ATI", "")
                            .replace("Advanced Micro Devices", "");
                        igpu = Some(format!("AMD {}", clean.trim()));
                    }
                } else if line.contains("Intel") && igpu.is_none() {
                    let name = if let Some(start) = line.find('[') {
                        if let Some(end) = line.find(']') {
                            &line[start + 1..end]
                        } else {
                            line
                        }
                    } else {
                        line
                    };
                    igpu = Some(format!("Intel {}", name.trim()));
                }
            }
        }
    }

    (dgpu, igpu)
}

pub async fn start_telemetry_loop(
    tx: tokio::sync::mpsc::Sender<TelemetryData>,
    interval_rx: tokio::sync::watch::Receiver<u64>,
) {
    let (_mode_tx, mode_rx) = tokio::sync::watch::channel(GpuMode::Auto);
    start_telemetry_loop_with_mode(tx, interval_rx, mode_rx).await;
}

/// Streams periodic [`TelemetryData`] samples for the active [`GpuMode`],
/// waking immediately when the interval or selected GPU mode changes.
/// Continuously samples live hardware telemetry at `interval_rx` milliseconds
/// for `gpu_mode_rx`, pausing sensor polling whenever the effective interval is `0`.
pub async fn start_telemetry_loop_with_mode(
    tx: tokio::sync::mpsc::Sender<TelemetryData>,
    mut interval_rx: tokio::sync::watch::Receiver<u64>,
    mut gpu_mode_rx: tokio::sync::watch::Receiver<GpuMode>,
) {
    let mut engine = TelemetryEngine::new();
    let mut current_interval = *interval_rx.borrow_and_update();
    let mut current_gpu_mode = *gpu_mode_rx.borrow_and_update();
    loop {
        if interval_rx.has_changed().unwrap_or(false) {
            current_interval = *interval_rx.borrow_and_update();
        }
        if gpu_mode_rx.has_changed().unwrap_or(false) {
            current_gpu_mode = *gpu_mode_rx.borrow_and_update();
        }
        if current_interval == 0 {
            tokio::select! {
                _ = tx.closed() => break,
                res = interval_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                    current_interval = *interval_rx.borrow_and_update();
                }
                res = gpu_mode_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                    current_gpu_mode = *gpu_mode_rx.borrow_and_update();
                }
            }
            continue;
        }
        let data = engine.read_current_for_mode(current_gpu_mode);
        if tx.send(data).await.is_err() {
            break;
        }
        let dur = Duration::from_millis(current_interval.max(100));
        tokio::select! {
            _ = sleep(dur) => {},
            res = interval_rx.changed() => {
                if res.is_err() {
                    break;
                }
                current_interval = *interval_rx.borrow_and_update();
            }
            res = gpu_mode_rx.changed() => {
                if res.is_err() {
                    break;
                }
                current_gpu_mode = *gpu_mode_rx.borrow_and_update();
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct TelemetrySummary {
    pub avg_cpu_usage: f32,
    pub peak_cpu_usage: f32,
    pub avg_cpu_temp: f32,
    pub peak_cpu_temp: f32,
    pub avg_cpu_freq_mhz: u32,
    pub peak_cpu_freq_mhz: u32,
    pub avg_gpu_usage: f32,
    pub peak_gpu_usage: f32,
    pub avg_gpu_temp: f32,
    pub peak_gpu_temp: f32,
    pub avg_gpu_freq_mhz: u32,
    pub peak_gpu_freq_mhz: u32,
    pub avg_vram_freq_mhz: u32,
    pub peak_vram_freq_mhz: u32,
    pub avg_gpu_power_w: f32,
    pub peak_gpu_power_w: f32,
    pub avg_power_w: f32,
    pub peak_power_w: f32,
    pub avg_ac_power_w: f32,
    pub peak_ac_power_w: f32,
    pub avg_ram_gb: f32,
    pub peak_ram_gb: f32,
    pub avg_vram_gb: f32,
    pub peak_vram_gb: f32,
    pub cpu_throttling: String,
    pub gpu_throttling: String,
}

pub fn calculate_telemetry_summary(samples: &[TelemetryData]) -> TelemetrySummary {
    if samples.is_empty() {
        return TelemetrySummary::default();
    }
    let count = samples.len() as f32;

    let avg_cpu_usage = samples.iter().map(|s| s.cpu_usage).sum::<f32>() / count;
    let peak_cpu_usage = samples.iter().map(|s| s.cpu_usage).fold(0.0f32, f32::max);

    let avg_cpu_temp = samples.iter().map(|s| s.cpu_temp).sum::<f32>() / count;
    let peak_cpu_temp = samples.iter().map(|s| s.cpu_temp).fold(0.0f32, f32::max);

    let avg_cpu_freq_mhz = (samples.iter().map(|s| s.cpu_freq).sum::<f32>() / count).round() as u32;
    let peak_cpu_freq_mhz = samples
        .iter()
        .map(|s| s.cpu_freq)
        .fold(0.0f32, f32::max)
        .round() as u32;

    let avg_gpu_usage = samples.iter().map(|s| s.gpu_usage).sum::<f32>() / count;
    let peak_gpu_usage = samples.iter().map(|s| s.gpu_usage).fold(0.0f32, f32::max);

    let avg_gpu_temp = samples.iter().map(|s| s.gpu_temp).sum::<f32>() / count;
    let peak_gpu_temp = samples.iter().map(|s| s.gpu_temp).fold(0.0f32, f32::max);

    let avg_gpu_freq_mhz =
        (samples.iter().map(|s| s.gpu_freq_mhz).sum::<f32>() / count).round() as u32;
    let peak_gpu_freq_mhz = samples
        .iter()
        .map(|s| s.gpu_freq_mhz)
        .fold(0.0f32, f32::max)
        .round() as u32;

    let avg_vram_freq_mhz =
        (samples.iter().map(|s| s.vram_freq_mhz).sum::<f32>() / count).round() as u32;
    let peak_vram_freq_mhz = samples
        .iter()
        .map(|s| s.vram_freq_mhz)
        .fold(0.0f32, f32::max)
        .round() as u32;

    let avg_gpu_power_w = samples.iter().map(|s| s.gpu_power_w).sum::<f32>() / count;
    let peak_gpu_power_w = samples.iter().map(|s| s.gpu_power_w).fold(0.0f32, f32::max);

    let avg_power_w = samples.iter().map(|s| s.power_w).sum::<f32>() / count;
    let peak_power_w = samples.iter().map(|s| s.power_w).fold(0.0f32, f32::max);

    let avg_ac_power_w = samples.iter().map(|s| s.ac_power_w).sum::<f32>() / count;
    let peak_ac_power_w = samples.iter().map(|s| s.ac_power_w).fold(0.0f32, f32::max);

    let avg_ram_gb = (samples.iter().map(|s| s.ram_usage_mb).sum::<f32>() / count) / 1024.0;
    let peak_ram_gb = (samples
        .iter()
        .map(|s| s.ram_usage_mb)
        .fold(0.0f32, f32::max))
        / 1024.0;

    let avg_vram_gb = (samples.iter().map(|s| s.vram_usage_mb).sum::<f32>() / count) / 1024.0;
    let peak_vram_gb = (samples
        .iter()
        .map(|s| s.vram_usage_mb)
        .fold(0.0f32, f32::max))
        / 1024.0;

    let cpu_thermal_count = samples
        .iter()
        .filter(|s| s.cpu_throttle.to_lowercase().contains("thermal"))
        .count();
    let cpu_throttling = if cpu_thermal_count > 0 {
        let pct = ((cpu_thermal_count as f32 / count) * 100.0).round() as u32;
        format!("thermal_throttle ({}%)", pct)
    } else if samples.iter().any(|s| {
        !s.cpu_throttle.is_empty()
            && !s.cpu_throttle.eq_ignore_ascii_case("none")
            && s.cpu_throttle != "-"
    }) {
        "detected".to_string()
    } else {
        "none".to_string()
    };

    let gpu_thermal_count = samples
        .iter()
        .filter(|s| s.gpu_throttle.to_lowercase().contains("thermal"))
        .count();
    let gpu_power_count = samples
        .iter()
        .filter(|s| s.gpu_throttle.to_lowercase().contains("power"))
        .count();
    let gpu_throttling = if gpu_thermal_count > 0 {
        let pct = ((gpu_thermal_count as f32 / count) * 100.0).round() as u32;
        format!("thermal_throttle ({}%)", pct)
    } else if gpu_power_count > 0 {
        let pct = ((gpu_power_count as f32 / count) * 100.0).round() as u32;
        format!("power_limit ({}%)", pct)
    } else if samples.iter().any(|s| {
        !s.gpu_throttle.is_empty()
            && !s.gpu_throttle.eq_ignore_ascii_case("none")
            && s.gpu_throttle != "-"
    }) {
        "detected".to_string()
    } else {
        "none".to_string()
    };

    TelemetrySummary {
        avg_cpu_usage,
        peak_cpu_usage,
        avg_cpu_temp,
        peak_cpu_temp,
        avg_cpu_freq_mhz,
        peak_cpu_freq_mhz,
        avg_gpu_usage,
        peak_gpu_usage,
        avg_gpu_temp,
        peak_gpu_temp,
        avg_gpu_freq_mhz,
        peak_gpu_freq_mhz,
        avg_vram_freq_mhz,
        peak_vram_freq_mhz,
        avg_gpu_power_w,
        peak_gpu_power_w,
        avg_power_w,
        peak_power_w,
        avg_ac_power_w,
        peak_ac_power_w,
        avg_ram_gb,
        peak_ram_gb,
        avg_vram_gb,
        peak_vram_gb,
        cpu_throttling,
        gpu_throttling,
    }
}

/// Detects the current Linux power profile (e.g. from powerprofilesctl, ACPI platform_profile, or CPU governor)
pub fn get_power_profile() -> String {
    // 1. Try powerprofilesctl get
    if let Ok(output) = std::process::Command::new("powerprofilesctl")
        .arg("get")
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return match s.as_str() {
                    "performance" => "Performance".to_string(),
                    "balanced" => "Balanced".to_string(),
                    "power-saver" => "Power Saver".to_string(),
                    _ => s,
                };
            }
        }
    }

    // 2. Try /sys/firmware/acpi/platform_profile
    if let Ok(content) = fs::read_to_string("/sys/firmware/acpi/platform_profile") {
        let s = content.trim().to_string();
        if !s.is_empty() {
            return match s.as_str() {
                "performance" => "Performance".to_string(),
                "balanced" | "balanced-performance" => "Balanced".to_string(),
                "low-power" | "power-saver" | "quiet" | "cool" => "Power Saver".to_string(),
                _ => s,
            };
        }
    }

    // 3. Try /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
    if let Ok(content) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
    {
        let s = content.trim().to_string();
        if !s.is_empty() {
            return match s.as_str() {
                "performance" => "Performance".to_string(),
                "schedutil" | "ondemand" => "Balanced (schedutil)".to_string(),
                "powersave" => "Power Saver (powersave)".to_string(),
                _ => s,
            };
        }
    }

    "Unknown".to_string()
}

/// Quickly reads the current CPU temperature in degrees Celsius from sysfs without a full system refresh.
pub fn get_instant_cpu_temp() -> f32 {
    // 1. Try hwmon paths
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(files) = fs::read_dir(&path) {
                for file in files.flatten() {
                    let name = file.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("temp") && name_str.ends_with("_input") {
                        if let Ok(content) = fs::read_to_string(file.path()) {
                            if let Ok(val) = content.trim().parse::<f32>() {
                                let temp = val / 1000.0;
                                if temp > 15.0 && temp < 130.0 {
                                    return temp;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Try thermal_zone paths
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("thermal_zone") {
                let temp_path = entry.path().join("temp");
                if let Ok(content) = fs::read_to_string(&temp_path) {
                    if let Ok(val) = content.trim().parse::<f32>() {
                        let temp = val / 1000.0;
                        if temp > 15.0 && temp < 130.0 {
                            return temp;
                        }
                    }
                }
            }
        }
    }

    0.0
}
