// src/modules/system_info.rs - OPTIMIZED VERSION
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;

use sysinfo::System;

use crate::CONFIG;
use crate::POWER_SUPPLY_DIR;
use crate::AVAILABLE_GOVERNORS_SORTED;

#[derive(Debug, Clone)]
pub struct CoreInfo {
    pub id: usize,
    pub usage: f32,
    pub temperature: f32,
    pub frequency: f32,
}

#[derive(Debug, Clone)]
pub struct BatteryInfo {
    pub is_charging: Option<bool>,
    pub is_ac_plugged: Option<bool>,
    pub charging_start_threshold: Option<i32>,
    pub charging_stop_threshold: Option<i32>,
    pub battery_level: Option<u8>,
    pub power_consumption: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct SystemReport {
    pub distro_name: String,
    pub distro_ver: String,
    pub arch: String,
    pub processor_model: String,
    pub total_core: Option<usize>,
    pub kernel_version: String,
    pub current_gov: Option<String>,
    pub current_epp: Option<String>,
    pub current_epb: Option<String>,
    pub cpu_driver: Option<String>,
    pub cpu_fan_speed: Option<i32>,
    pub cpu_usage: f32,
    pub cpu_max_freq: Option<f32>,
    pub cpu_min_freq: Option<f32>,
    pub load: f32,
    pub avg_load: Option<(f32,f32,f32)>,
    pub cores_info: Vec<CoreInfo>,
    pub battery_info: BatteryInfo,
    pub is_turbo_on: (Option<bool>, Option<bool>),
}

// ============================================================================
// OPTIMIZATION: Temperature Sensor Cache
// ============================================================================
struct TempSensorCache {
    sensor_paths: HashMap<usize, PathBuf>,
    package_temp_path: Option<PathBuf>,
    fan_speed_path: Option<PathBuf>,
    last_scan: Instant,
}

impl TempSensorCache {
    fn new() -> Self {
        let mut cache = Self {
            sensor_paths: HashMap::new(),
            package_temp_path: None,
            fan_speed_path: None,
            last_scan: Instant::now(),
        };
        cache.scan_sensors();
        cache
    }

    fn scan_sensors(&mut self) {
        let sensor_priority = ["coretemp", "k10temp", "zenpower", "acpitz"];
        let hwmon_path = "/sys/class/hwmon";
        
        if let Ok(entries) = fs::read_dir(hwmon_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_file = path.join("name");
                
                if let Ok(sensor_name) = fs::read_to_string(&name_file) {
                    let sensor_name = sensor_name.trim();
                    
                    if sensor_priority.contains(&sensor_name) {
                        // Cache package temp
                        let pkg_temp = path.join("temp1_input");
                        if pkg_temp.exists() {
                            self.package_temp_path = Some(pkg_temp);
                        }
                        
                        // Cache core temps
                        for temp_id in 2..20 {
                            let temp_file = path.join(format!("temp{}_input", temp_id));
                            if temp_file.exists() {
                                let core_id = temp_id - 2;
                                self.sensor_paths.insert(core_id, temp_file);
                            }
                        }
                    }
                    
                    // Cache fan speed
                    if self.fan_speed_path.is_none() {
                        let fan_input = path.join("fan1_input");
                        if fan_input.exists() {
                            self.fan_speed_path = Some(fan_input);
                        }
                    }
                }
            }
        }
        
        self.last_scan = Instant::now();
    }

    fn read_core_temp(&self, core_id: usize) -> f32 {
        if let Some(path) = self.sensor_paths.get(&core_id) {
            if let Ok(temp_str) = fs::read_to_string(path) {
                if let Ok(temp) = temp_str.trim().parse::<f32>() {
                    return temp / 1000.0;
                }
            }
        }
        
        if let Some(ref path) = self.package_temp_path {
            if let Ok(temp_str) = fs::read_to_string(path) {
                if let Ok(temp) = temp_str.trim().parse::<f32>() {
                    return temp / 1000.0;
                }
            }
        }
        
        0.0
    }

    fn read_fan_speed(&self) -> Option<i32> {
        if let Some(ref path) = self.fan_speed_path {
            if let Ok(fan_str) = fs::read_to_string(path) {
                if let Ok(rpm) = fan_str.trim().parse::<i32>() {
                    if rpm > 0 {
                        return Some(rpm);
                    }
                }
            }
        }
        None
    }
}
lazy_static::lazy_static! {
    static ref TEMP_CACHE: Arc<Mutex<TempSensorCache>> = Arc::new(Mutex::new(TempSensorCache::new()));
}

// ============================================================================
// OPTIMIZATION: Static Info Cache
// ============================================================================
struct StaticInfoCache {
    processor_model: String,
    cpu_driver: Option<String>,
    cpu_min_freq: Option<f32>,
    cpu_max_freq: Option<f32>,
}

impl StaticInfoCache {
    fn new() -> Self {
        Self {
            processor_model: Self::read_processor_model(),
            cpu_driver: Self::read_cpu_driver(),
            cpu_min_freq: Self::read_cpu_min_freq(),
            cpu_max_freq: Self::read_cpu_max_freq(),
        }
    }

    fn read_processor_model() -> String {
        fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.contains("model name"))
                    .and_then(|l| l.split(':').nth(1))
                    .map(|s| s.trim().to_string())
            })
            .unwrap_or_default()
    }

    fn read_cpu_driver() -> Option<String> {
        fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver")
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn read_cpu_min_freq() -> Option<f32> {
        fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|khz| khz / 1000.0)
    }

    fn read_cpu_max_freq() -> Option<f32> {
        fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|khz| khz / 1000.0)
    }
}

lazy_static::lazy_static! {
    static ref STATIC_INFO: StaticInfoCache = StaticInfoCache::new();
}

// ============================================================================
// OPTIMIZATION: Battery Path Cache
// ============================================================================
struct BatteryPathCache {
    battery_path: Option<PathBuf>,
    mains_path: Option<PathBuf>,
    cached_at: Instant,
}

impl BatteryPathCache {
    fn new() -> Self {
        let (battery_path, mains_path) = Self::scan_power_supply();
        Self {
            battery_path,
            mains_path,
            cached_at: Instant::now(),
        }
    }

    fn scan_power_supply() -> (Option<PathBuf>, Option<PathBuf>) {
        let mut battery = None;
        let mut mains = None;

        // Check custom config first
        if CONFIG.has_option("battery", "battery_device") {
            let battery_device = CONFIG.get("battery", "battery_device", "");
            if !battery_device.is_empty() {
                let custom = Path::new(POWER_SUPPLY_DIR).join(&battery_device);
                let type_path = custom.join("type");
                if type_path.is_file() {
                    if let Ok(content) = fs::read_to_string(type_path) {
                        if content.trim().to_lowercase() == "battery" {
                            battery = Some(custom);
                        }
                    }
                }
            }
        }

        // Scan all power supplies
        if let Ok(entries) = fs::read_dir(POWER_SUPPLY_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                let type_path = path.join("type");
                
                if let Ok(content) = fs::read_to_string(&type_path) {
                    match content.trim() {
                        "Battery" if battery.is_none() => battery = Some(path),
                        "Mains" if mains.is_none() => mains = Some(path),
                        _ => {}
                    }
                }
            }
        }

        (battery, mains)
    }

    fn maybe_rescan(&mut self) {
        if self.cached_at.elapsed() > Duration::from_secs(60) {
            let (battery, mains) = Self::scan_power_supply();
            self.battery_path = battery;
            self.mains_path = mains;
            self.cached_at = Instant::now();
        }
    }
}

lazy_static::lazy_static! {
    static ref BATTERY_PATH_CACHE: Arc<Mutex<BatteryPathCache>> = 
        Arc::new(Mutex::new(BatteryPathCache::new()));
}

// ============================================================================
// SystemInfo
// ============================================================================
pub struct SystemInfo {
    pub distro_name: String,
    pub distro_version: String,
    pub architecture: String,
    pub processor_model: String,
    pub total_cores: Option<usize>,
    pub cpu_driver: Option<String>,
    pub kernel_version: String,
}

impl SystemInfo {
    pub fn new() -> Self {
        let distro_name = Self::read_os_release_name().unwrap_or_else(|| "UNKNOWN".into());
        let distro_version = Self::read_os_release_version().unwrap_or_else(|| "UNKNOWN".into());
        let architecture = std::env::consts::ARCH.to_string();
        let total_cores = Some(num_cpus::get());
        let kernel_version = Self::uname_release().unwrap_or_default();

        Self {
            distro_name,
            distro_version,
            architecture,
            processor_model: STATIC_INFO.processor_model.clone(),
            total_cores,
            cpu_driver: STATIC_INFO.cpu_driver.clone(),
            kernel_version,
        }
    }

    fn read_os_release_name() -> Option<String> {
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return Some(line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"').to_string());
                }
            }
        }
        None
    }

    fn read_os_release_version() -> Option<String> {
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("VERSION=") {
                    return Some(line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"').to_string());
                }
            }
        }
        None
    }

    fn uname_release() -> Option<String> {
        std::process::Command::new("uname").arg("-r").output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    // OPTIMIZED: Use cached values
    pub fn cpu_min_freq() -> Option<f32> {
        STATIC_INFO.cpu_min_freq
    }

    pub fn cpu_max_freq() -> Option<f32> {
        STATIC_INFO.cpu_max_freq
    }

    // OPTIMIZED: Batch read all CPU info at once
    pub fn get_cpu_info(sys: &System) -> Vec<CoreInfo> {
        let cpus = sys.cpus();
        let mut cores = Vec::with_capacity(cpus.len());

        // OPTIMIZED: Lock cache once and read all temps
        let temp_cache = TEMP_CACHE.lock().unwrap();

        for (i, cpu) in cpus.iter().enumerate() {
            cores.push(CoreInfo {
                id: i,
                usage: cpu.cpu_usage(),
                frequency: cpu.frequency() as f32,
                temperature: temp_cache.read_core_temp(i),
            });
        }

        cores
    }

    // OPTIMIZED: Use cached fan speed
    pub fn cpu_fan_speed() -> Option<i32> {
        TEMP_CACHE.lock().unwrap().read_fan_speed()
    }

    pub fn current_gov() -> Option<String> {
        fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .ok()
            .map(|s| s.trim().to_string())
    }

    pub fn current_epp(is_ac_plugged: bool) -> Option<String> {
        let epp_path = Path::new("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference");
        if !epp_path.exists() {
            return None;
        }

        let section = if is_ac_plugged { "charger" } else { "battery" };
        Some(CONFIG.get(section, "energy_performance_preference", "balance_power"))
    }

    pub fn current_epb(is_ac_plugged: bool) -> Option<String> {
        let epb_path = Path::new("/sys/devices/system/cpu/intel_pstate");
        if !epb_path.exists() {
            return None;
        }
        let section = if is_ac_plugged { "charger" } else { "battery" };
        Some(CONFIG.get(section, "energy_perf_bias", "balance_power"))
    }

    pub fn cpu_usage(sys: &System) -> f32 {
        let cpus = sys.cpus();
        if cpus.is_empty() {
            return 0.0;
        }
        let sum: f32 = cpus.iter().map(|c| c.cpu_usage()).sum();
        sum / (cpus.len() as f32)
    }

    pub fn system_load() -> f32 {
        if let Ok(s) = fs::read_to_string("/proc/loadavg") {
            if let Some(first) = s.split_whitespace().next() {
                return first.parse::<f32>().unwrap_or(0.0);
            }
        }
        0.0
    }

    pub fn avg_load() -> Option<(f32,f32,f32)> {
        if let Ok(s) = fs::read_to_string("/proc/loadavg") {
            let mut parts = s.split_whitespace();
            let a = parts.next().and_then(|p| p.parse::<f32>().ok());
            let b = parts.next().and_then(|p| p.parse::<f32>().ok());
            let c = parts.next().and_then(|p| p.parse::<f32>().ok());
            if let (Some(a), Some(b), Some(c)) = (a,b,c) {
                return Some((a,b,c));
            }
        }
        None
    }

    pub fn avg_temp(sys: &System) -> i32 {
        let temps: Vec<f32> = Self::get_cpu_info(sys)
            .iter()
            .map(|c| c.temperature)
            .filter(|&t| t > 0.0)
            .collect();
        
        if temps.is_empty() { 
            0 
        } else { 
            (temps.iter().sum::<f32>() / temps.len() as f32) as i32 
        }
    }

    pub fn turbo_on() -> (Option<bool>, Option<bool>) {
        let intel_pstate = Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo");
        let cpu_freq = Path::new("/sys/devices/system/cpu/cpufreq/boost");
        let amd_pstate = Path::new("/sys/devices/system/cpu/amd_pstate/status");

        if intel_pstate.exists() {
            if let Ok(v) = fs::read_to_string(intel_pstate) {
                if let Ok(n) = v.trim().parse::<i32>() {
                    return (Some(n == 0), Some(false));
                }
            }
            return (None, None);
        }

        if cpu_freq.exists() {
            if let Ok(v) = fs::read_to_string(cpu_freq) {
                if let Ok(n) = v.trim().parse::<i32>() {
                    return (Some(n != 0), Some(false));
                }
            }
            return (None, None);
        }

        if amd_pstate.exists() {
            if let Ok(s) = fs::read_to_string(amd_pstate) {
                if s.trim() == "active" { return (None, Some(true)); }
                return (None, Some(false));
            }
            return (None, None);
        }

        (None, None)
    }

    // OPTIMIZED: Battery path cache
    pub fn get_battery_path() -> Option<PathBuf> {
        let mut cache = BATTERY_PATH_CACHE.lock().unwrap();
        cache.maybe_rescan();
        cache.battery_path.clone()
    }

    // OPTIMIZED: Batch read all battery info
    pub fn battery_info() -> BatteryInfo {
        let mut cache = BATTERY_PATH_CACHE.lock().unwrap();
        cache.maybe_rescan();

        let mut is_ac_plugged = Some(true);

        // Check mains status
        if let Some(ref mains_path) = cache.mains_path {
            if let Ok(online) = fs::read_to_string(mains_path.join("online")) {
                is_ac_plugged = Some(online.trim() == "1");
            }
        }

        let battery_path = match &cache.battery_path {
            Some(p) => p,
            None => {
                return BatteryInfo {
                    is_charging: None,
                    is_ac_plugged: Some(true),
                    charging_start_threshold: None,
                    charging_stop_threshold: None,
                    battery_level: None,
                    power_consumption: None,
                };
            }
        };

        // OPTIMIZED: Batch read all battery files
        let status = fs::read_to_string(battery_path.join("status")).ok();
        let capacity = fs::read_to_string(battery_path.join("capacity")).ok();
        let energy_rate = fs::read_to_string(battery_path.join("power_now"))
            .or_else(|_| fs::read_to_string(battery_path.join("current_now")))
            .ok();
        let charge_start = fs::read_to_string(battery_path.join("charge_start_threshold"))
            .or_else(|_| fs::read_to_string(battery_path.join("charge_control_start_threshold")))
            .ok();
        let charge_stop = fs::read_to_string(battery_path.join("charge_stop_threshold"))
            .or_else(|_| fs::read_to_string(battery_path.join("charge_control_end_threshold")))
            .ok();

        let is_charging = status.as_ref().map(|s| s.trim().to_lowercase() == "charging");
        let battery_level = capacity.and_then(|c| c.trim().parse::<u8>().ok());
        let power_consumption = energy_rate.and_then(|e| e.trim().parse::<f32>().ok()).map(|v| v / 1_000_000.0);
        let charging_start_threshold = charge_start.and_then(|s| s.trim().parse::<i32>().ok());
        let charging_stop_threshold = charge_stop.and_then(|s| s.trim().parse::<i32>().ok());

        BatteryInfo {
            is_charging,
            is_ac_plugged,
            charging_start_threshold,
            charging_stop_threshold,
            battery_level,
            power_consumption,
        }
    }

    pub fn turbo_on_suggestion(sys: &System) -> bool {
        let usage = Self::cpu_usage(sys);
        if usage >= 20.0 { return true; }
        if usage <= 25.0 && Self::avg_temp(sys) as f32 >= 70.0 { return false; }
        false
    }

    pub fn governor_suggestion() -> Option<String> {
        let batt = Self::battery_info();
        if batt.is_ac_plugged.unwrap_or(true) {
            AVAILABLE_GOVERNORS_SORTED.first().cloned()
        } else {
            AVAILABLE_GOVERNORS_SORTED.last().cloned()
        }
    }

    // OPTIMIZED: Generate report without redundant refreshes
    pub fn generate_system_report(&self, sys: &System) -> SystemReport {
        let battery = Self::battery_info();
        let cores = Self::get_cpu_info(sys);

        SystemReport {
            distro_name: self.distro_name.clone(),
            distro_ver: self.distro_version.clone(),
            arch: self.architecture.clone(),
            processor_model: self.processor_model.clone(),
            total_core: self.total_cores,
            kernel_version: self.kernel_version.clone(),
            current_gov: Self::current_gov(),
            current_epp: battery.is_ac_plugged.and_then(Self::current_epp),
            current_epb: battery.is_ac_plugged.and_then(Self::current_epb),
            cpu_driver: self.cpu_driver.clone(),
            cpu_fan_speed: Self::cpu_fan_speed(),
            cpu_usage: Self::cpu_usage(sys),
            cpu_max_freq: Self::cpu_max_freq(),
            cpu_min_freq: Self::cpu_min_freq(),
            load: Self::system_load(),
            avg_load: Self::avg_load(),
            cores_info: cores,
            battery_info: battery,
            is_turbo_on: Self::turbo_on(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let s = SystemInfo::new();
        let mut sys = System::new_all();
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        let _ = s.generate_system_report(&sys);
    }

    #[test]
    fn test_temp_cache() {
        let cache = TEMP_CACHE.lock().unwrap();
        let temp = cache.read_core_temp(0);
        assert!(temp >= 0.0);
    }

    #[test]
    fn test_battery_cache() {
        let cache = BATTERY_PATH_CACHE.lock().unwrap();
        // Just ensure it doesn't panic
        let _ = cache.battery_path.is_some();
    }
}
