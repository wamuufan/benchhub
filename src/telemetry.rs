use crate::models::TelemetryData;
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
                if get_mem(self.device, &mut mem_info) == 0 && mem_info.used > 0 {
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

pub struct SensorCache {
    pub cpu_power_paths: Vec<PathBuf>,
    pub gpu_power_paths: Vec<PathBuf>,
    pub apu_power_paths: Vec<PathBuf>,
    pub battery_power_paths: Vec<PathBuf>,
    pub ac_online_path: Option<PathBuf>,
    pub cpu_temp_paths: Vec<PathBuf>,
    pub gpu_temp_paths: Vec<PathBuf>,
    pub gpu_usage_paths: Vec<PathBuf>,
    pub gpu_freq_paths: Vec<PathBuf>,
    pub vram_freq_paths: Vec<PathBuf>,
    pub vram_used_paths: Vec<PathBuf>,
    pub processor_cooling_cur_states: Vec<PathBuf>,
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
            ac_online_path: None,
            cpu_temp_paths: Vec::new(),
            gpu_temp_paths: Vec::new(),
            gpu_usage_paths: Vec::new(),
            gpu_freq_paths: Vec::new(),
            vram_freq_paths: Vec::new(),
            vram_used_paths: Vec::new(),
            processor_cooling_cur_states: Vec::new(),
        };
        cache.discover();
        cache
    }

    pub fn discover(&mut self) {
        let hwmon_dir = Path::new("/sys/class/hwmon");
        if hwmon_dir.exists() {
            if let Ok(entries) = fs::read_dir(hwmon_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = fs::read_to_string(p.join("name"))
                        .unwrap_or_default()
                        .trim()
                        .to_lowercase();
                    let is_cpu = name.contains("k10temp")
                        || name.contains("coretemp")
                        || name.contains("zenpower")
                        || name.contains("cpu")
                        || name.contains("soc_thermal");
                    let is_apu = name.contains("amdgpu");
                    let is_discrete_gpu = name.contains("nouveau")
                        || name.contains("nvidia")
                        || name.contains("i915")
                        || name.contains("xe");

                    // Read power, temp, and freq files in this hwmon directory
                    let mut found_avg = None;
                    let mut found_input = None;

                    if let Ok(files) = fs::read_dir(&p) {
                        for file in files.flatten() {
                            let fname = file.file_name().to_string_lossy().to_string();
                            if fname.starts_with("power") {
                                if fname.ends_with("_average") {
                                    found_avg = Some(file.path());
                                } else if fname.ends_with("_input") && found_avg.is_none() {
                                    found_input = Some(file.path());
                                }
                            }
                            if fname.starts_with("temp") && fname.ends_with("_input") {
                                if is_cpu {
                                    self.cpu_temp_paths.push(file.path());
                                } else if is_apu || is_discrete_gpu {
                                    self.gpu_temp_paths.push(file.path());
                                }
                            }
                            if is_apu || is_discrete_gpu {
                                if fname == "freq1_input" {
                                    self.gpu_freq_paths.push(file.path());
                                } else if fname == "freq2_input" {
                                    self.vram_freq_paths.push(file.path());
                                }
                            }
                        }
                    }

                    // Prevent double-counting: pick only ONE power file per hwmon device
                    let chosen_power = found_avg.or(found_input);
                    if let Some(path) = chosen_power {
                        if is_cpu {
                            self.cpu_power_paths.push(path);
                        } else if is_apu {
                            self.apu_power_paths.push(path);
                        } else if is_discrete_gpu {
                            self.gpu_power_paths.push(path);
                        }
                    }
                }
            }
        }

        // On AMD platforms (Ryzen APUs like 7840HS), amdgpu power is the APU Package Power (CPU+SoC)
        // If no dedicated CPU power path was found (e.g. k10temp has no power file), assign APU power to CPU
        if self.cpu_power_paths.is_empty() && !self.apu_power_paths.is_empty() {
            self.cpu_power_paths.extend(self.apu_power_paths.clone());
        }

        let psy_dir = Path::new("/sys/class/power_supply");
        if psy_dir.exists() {
            if let Ok(entries) = fs::read_dir(psy_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if name.starts_with("BAT") || name.contains("battery") {
                        let power_path = p.join("power_now");
                        if power_path.exists() {
                            self.battery_power_paths.push(power_path);
                        }
                    } else if name.starts_with("AC") || name.contains("mains") {
                        let online_path = p.join("online");
                        if online_path.exists() {
                            self.ac_online_path = Some(online_path);
                        }
                    }
                }
            }
        }

        let tz_dir = Path::new("/sys/class/thermal");
        if tz_dir.exists() {
            if let Ok(entries) = fs::read_dir(tz_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if name.starts_with("thermal_zone") && self.cpu_temp_paths.is_empty() {
                        let tz_type = fs::read_to_string(p.join("type"))
                            .unwrap_or_default()
                            .to_lowercase();
                        if tz_type.contains("cpu")
                            || tz_type.contains("acpi")
                            || tz_type.contains("pkg")
                        {
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
            }
        }

        let drm_dir = Path::new("/sys/class/drm");
        if drm_dir.exists() {
            if let Ok(entries) = fs::read_dir(drm_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("card") && !name.contains('-') {
                        let busy_p = p.join("device/gpu_busy_percent");
                        if busy_p.exists() {
                            self.gpu_usage_paths.push(busy_p);
                        }

                        // AMD / Intel GPU core clock paths
                        let sclk_p = p.join("device/pp_dpm_sclk");
                        if sclk_p.exists() {
                            self.gpu_freq_paths.push(sclk_p);
                        }
                        let gfxclk_p = p.join("device/current_gfxclk");
                        if gfxclk_p.exists() {
                            self.gpu_freq_paths.push(gfxclk_p);
                        }
                        let gt_act_p = p.join("gt_act_freq_mhz");
                        if gt_act_p.exists() {
                            self.gpu_freq_paths.push(gt_act_p);
                        }
                        let gt_cur_p = p.join("gt_cur_freq_mhz");
                        if gt_cur_p.exists() {
                            self.gpu_freq_paths.push(gt_cur_p);
                        }

                        // AMD / Intel VRAM clock paths
                        let mclk_p = p.join("device/pp_dpm_mclk");
                        if mclk_p.exists() {
                            self.vram_freq_paths.push(mclk_p);
                        }

                        // AMD / Intel VRAM memory used paths
                        let vram_used_p = p.join("device/mem_info_vram_used");
                        if vram_used_p.exists() {
                            self.vram_used_paths.push(vram_used_p);
                        }
                    }
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
        let nvml = NvmlContext::init();
        Self {
            sys,
            last_energy_uj: None,
            last_energy_time: None,
            rapl_path,
            sensor_cache: SensorCache::new(),
            nvml,
        }
    }

    pub fn sample_gpu_usage(&self) -> f32 {
        if let Some(nvml) = &self.nvml {
            if let Some(util) = nvml.sample_utilization() {
                return util.clamp(0.0, 100.0);
            }
        }
        for p in &self.sensor_cache.gpu_usage_paths {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(val) = s.trim().parse::<f32>() {
                    return val.clamp(0.0, 100.0);
                }
            }
        }
        0.0
    }

    pub fn sample_gpu_temp(&self) -> f32 {
        if let Some(nvml) = &self.nvml {
            if let Some(temp) = nvml.sample_temperature() {
                return temp;
            }
        }
        for p in &self.sensor_cache.gpu_temp_paths {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(val) = s.trim().parse::<f32>() {
                    let temp = val / 1000.0;
                    if temp > 0.0 && temp < 140.0 {
                        return temp;
                    }
                }
            }
        }
        0.0
    }

    pub fn sample_gpu_freq_mhz(&self) -> f32 {
        if let Some(nvml) = &self.nvml {
            if let Some(mhz) = nvml.sample_gpu_freq_mhz() {
                return mhz;
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
        if let Some(nvml) = &self.nvml {
            if let Some(mhz) = nvml.sample_vram_freq_mhz() {
                return mhz;
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
        if let Some(nvml) = &self.nvml {
            if let Some(mb) = nvml.sample_vram_usage_mb() {
                return mb;
            }
        }
        for p in &self.sensor_cache.vram_used_paths {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(bytes) = s.trim().parse::<f32>() {
                    let mb = bytes / 1024.0 / 1024.0;
                    if mb > 0.0 && mb < 1_000_000.0 {
                        return mb;
                    }
                }
            }
        }
        0.0
    }

    pub fn sample_gpu_throttling(&self, gpu_temp: f32) -> String {
        if let Some(nvml) = &self.nvml {
            if let Some(reason) = nvml.sample_throttle_reason() {
                return reason;
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
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(val) = s.trim().parse::<f32>() {
                    let temp = val / 1000.0;
                    if temp > 0.0 && temp < 140.0 {
                        cpu_temp = temp;
                        break;
                    }
                }
            }
        }

        let gpu_temp = self.sample_gpu_temp();
        let cpu_usage = self.sys.global_cpu_info().cpu_usage();
        let gpu_usage = self.sample_gpu_usage();
        let gpu_freq_mhz = self.sample_gpu_freq_mhz();
        let vram_freq_mhz = self.sample_vram_freq_mhz();
        let vram_usage_mb = self.sample_vram_usage_mb();
        let power_w = self.sample_power_w().unwrap_or(0.0);
        let gpu_power_w = self.sample_gpu_power_w();
        let ac_power_w = self.sample_ac_power_w(power_w, gpu_power_w);
        let ram_usage_mb = (self.sys.used_memory() as f32) / 1024.0 / 1024.0;
        let cpu_throttle = self.sample_cpu_throttling(cpu_temp);
        let gpu_throttle = self.sample_gpu_throttling(gpu_temp);

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
                        if elapsed > 0.001 && current_uj >= last_uj {
                            calculate_power_watts(current_uj - last_uj, elapsed)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    self.last_energy_uj = Some(current_uj);
                    self.last_energy_time = Some(now);
                    if power > 0.0 && power < 500.0 {
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
        // 1. Try NVML for NVIDIA discrete GPU (mW -> W)
        if let Some(nvml) = &self.nvml {
            if let Some(w) = nvml.sample_power_w() {
                if w > 0.0 && w < 500.0 {
                    return w;
                }
            }
        }

        // 2. Try Hwmon for AMD / Intel discrete GPU (µW -> W)
        let mut total_w = 0.0;
        for p in &self.sensor_cache.gpu_power_paths {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(uw) = s.trim().parse::<f32>() {
                    if uw > 0.0 {
                        let w = uw / 1_000_000.0;
                        if w < 500.0 {
                            total_w += w;
                        }
                    }
                }
            }
        }

        total_w
    }

    pub fn sample_ac_power_w(&self, cpu_power: f32, gpu_power: f32) -> f32 {
        // 1. Check battery discharge power (µW -> W) if running on battery
        for p in &self.sensor_cache.battery_power_paths {
            if let Ok(s) = fs::read_to_string(p) {
                if let Ok(uw) = s.trim().parse::<f32>() {
                    let w = uw / 1_000_000.0;
                    if w > 0.0 && w < 500.0 {
                        return w;
                    }
                }
            }
        }

        // 2. If plugged in to AC Mains (or online), calculate total wall/AC draw
        let is_ac_online = self
            .sensor_cache
            .ac_online_path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|s| s.trim() == "1")
            .unwrap_or(true);

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
    let base_powercap = Path::new("/sys/class/powercap");
    if let Ok(entries) = fs::read_dir(base_powercap) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("intel-rapl") {
                let energy_file = p.join("energy_uj");
                if energy_file.exists() && fs::read_to_string(&energy_file).is_ok() {
                    return Some(energy_file);
                }
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
    let is_en = crate::i18n::get_language() == "en";
    let mut parts = Vec::new();
    if !engine.sensor_cache.cpu_temp_paths.is_empty() {
        parts.push(if is_en {
            format!(
                "{} CPU Temperature Sensor(s)",
                engine.sensor_cache.cpu_temp_paths.len()
            )
        } else {
            format!(
                "{} CPU Sıcaklık Sensörü",
                engine.sensor_cache.cpu_temp_paths.len()
            )
        });
    } else {
        parts.push(
            if is_en {
                "No CPU Temp Sensor"
            } else {
                "CPU Sıcaklık Sensörü Yok"
            }
            .to_string(),
        );
    }
    if engine.nvml.is_some() {
        parts.push(
            if is_en {
                "NVIDIA NVML (dGPU) Active"
            } else {
                "NVIDIA NVML (Harici GPU) Sensörü Aktif"
            }
            .to_string(),
        );
    } else if !engine.sensor_cache.gpu_temp_paths.is_empty() {
        parts.push(if is_en {
            format!("{} GPU Sensor(s)", engine.sensor_cache.gpu_temp_paths.len())
        } else {
            format!("{} GPU Sensörü", engine.sensor_cache.gpu_temp_paths.len())
        });
    }
    if engine.nvml.is_some() || !engine.sensor_cache.gpu_power_paths.is_empty() {
        parts.push(
            if is_en {
                "GPU Power Sensor Active (W)"
            } else {
                "GPU Güç Sensörü Aktif (W)"
            }
            .to_string(),
        );
    }
    if engine.rapl_path.is_some() {
        parts.push(
            if is_en {
                "Intel RAPL Energy/Power Active"
            } else {
                "Intel RAPL Enerji/Güç Ölçer Aktif"
            }
            .to_string(),
        );
    } else if !engine.sensor_cache.cpu_power_paths.is_empty() {
        parts.push(
            if is_en {
                "CPU Power Sensor Active"
            } else {
                "CPU Güç Sensörü Aktif"
            }
            .to_string(),
        );
    } else if !engine.sensor_cache.battery_power_paths.is_empty() {
        parts.push(
            if is_en {
                "Battery / AC Power Sensor Active"
            } else {
                "Pil / AC Güç Sensörü Aktif"
            }
            .to_string(),
        );
    } else {
        parts.push(
            if is_en {
                "No Power Sensor Found"
            } else {
                "Güç Sensörü Bulunamadı"
            }
            .to_string(),
        );
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
        "Standart GPU".to_string()
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
    mut interval_rx: tokio::sync::watch::Receiver<u64>,
) {
    let mut engine = TelemetryEngine::new();
    let mut current_interval = *interval_rx.borrow_and_update();
    loop {
        if interval_rx.has_changed().unwrap_or(false) {
            current_interval = *interval_rx.borrow_and_update();
        }
        let data = engine.read_current();
        if tx.send(data).await.is_err() {
            break;
        }
        let dur = Duration::from_millis(current_interval.max(100));
        tokio::select! {
            _ = sleep(dur) => {},
            _ = interval_rx.changed() => {
                current_interval = *interval_rx.borrow();
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
        .filter(|s| s.cpu_throttle.contains("Termal") || s.cpu_throttle.contains("thermal"))
        .count();
    let cpu_throttling = if cpu_thermal_count > 0 {
        let pct = ((cpu_thermal_count as f32 / count) * 100.0).round() as u32;
        format!("thermal_throttle (%{})", pct)
    } else if samples.iter().any(|s| {
        !s.cpu_throttle.is_empty()
            && s.cpu_throttle != "Yok"
            && s.cpu_throttle != "none"
            && s.cpu_throttle != "-"
    }) {
        "detected".to_string()
    } else {
        "none".to_string()
    };

    let gpu_thermal_count = samples
        .iter()
        .filter(|s| s.gpu_throttle.contains("Termal") || s.gpu_throttle.contains("thermal"))
        .count();
    let gpu_power_count = samples
        .iter()
        .filter(|s| s.gpu_throttle.contains("Güç") || s.gpu_throttle.contains("power"))
        .count();
    let gpu_throttling = if gpu_thermal_count > 0 {
        let pct = ((gpu_thermal_count as f32 / count) * 100.0).round() as u32;
        format!("thermal_throttle (%{})", pct)
    } else if gpu_power_count > 0 {
        let pct = ((gpu_power_count as f32 / count) * 100.0).round() as u32;
        format!("power_limit (%{})", pct)
    } else if samples.iter().any(|s| {
        !s.gpu_throttle.is_empty()
            && s.gpu_throttle != "Yok"
            && s.gpu_throttle != "none"
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
                    "performance" => "Performans (performance)".to_string(),
                    "balanced" => "Dengeli (balanced)".to_string(),
                    "power-saver" => "Güç Tasarrufu (power-saver)".to_string(),
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
                "performance" => "Performans (performance)".to_string(),
                "balanced" | "balanced-performance" => "Dengeli (balanced)".to_string(),
                "low-power" | "power-saver" | "quiet" | "cool" => {
                    "Güç Tasarrufu (power-saver)".to_string()
                }
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
                "performance" => "Performans (performance)".to_string(),
                "schedutil" | "ondemand" => "Dengeli (schedutil)".to_string(),
                "powersave" => "Güç Tasarrufu (powersave)".to_string(),
                _ => s,
            };
        }
    }

    "Bilinmiyor".to_string()
}
