// src/modules/system_info.rs
use std::fs;
use std::path::{Path, PathBuf};

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
        let processor_model = Self::read_file_single("/proc/cpuinfo")
            .and_then(|s| s.lines().find(|l| l.contains("model name")).map(|l| l.split(':').last().unwrap_or("").trim().to_string()))
            .unwrap_or_default();
        let total_cores = Some(num_cpus::get());
        let cpu_driver = Self::read_file_single("/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver");
        let kernel_version = Self::uname_release().unwrap_or_default();

        Self {
            distro_name,
            distro_version,
            architecture,
            processor_model,
            total_cores,
            cpu_driver,
            kernel_version,
        }
    }

    fn read_file_single<P: AsRef<Path>>(path: P) -> Option<String> {
        fs::read_to_string(path).ok().map(|s| s.trim().to_string())
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

    pub fn cpu_min_freq() -> Option<f32> {
        Self::read_file_single("/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq")
            .and_then(|s| s.parse::<f32>().ok())
            .map(|khl| khl / 1000.0)
    }

    pub fn cpu_max_freq() -> Option<f32> {
        Self::read_file_single("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
            .and_then(|s| s.parse::<f32>().ok())
            .map(|khl| khl / 1000.0)
    }

    fn read_cpu_temperature(core_id: usize) -> f32 {
        let sensor_priority = ["coretemp", "k10temp", "zenpower", "acpitz"];
        let hwmon_path = "/sys/class/hwmon";
        
        if let Ok(entries) = fs::read_dir(hwmon_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_file = path.join("name");
                
                if let Ok(sensor_name) = fs::read_to_string(&name_file) {
                    let sensor_name = sensor_name.trim();
                    
                    if sensor_priority.contains(&sensor_name) {
                        let preferred_temp_id = core_id + 2;
                        let max_temp_id = std::cmp::min(core_id + 10, 20);
                        
                        for temp_id in preferred_temp_id..max_temp_id {
                            let temp_file = path.join(format!("temp{}_input", temp_id));
                            
                            if temp_file.exists() {
                                if let Ok(temp_str) = fs::read_to_string(&temp_file) {
                                    if let Ok(temp_millidegrees) = temp_str.trim().parse::<f32>() {
                                        return temp_millidegrees / 1000.0;
                                    }
                                }
                            }
                        }
                        
                        // Fallback: use package temp for all cores
                        let temp_input = path.join("temp1_input");
                        if let Ok(temp_str) = fs::read_to_string(&temp_input) {
                            if let Ok(temp) = temp_str.trim().parse::<f32>() {
                                return temp / 1000.0;
                            }
                        }
                    }
                }
            }
        }
        
        0.0
    }
    // System nesnesini parametre olarak alıyoruz
   pub fn get_cpu_info(sys: &mut System) -> Vec<CoreInfo> {
        // CRITICAL: Refresh CPU before reading
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();

        let cpus = sys.cpus();
        let mut cores = Vec::new();

        for (i, cpu) in cpus.iter().enumerate() {
            let usage = cpu.cpu_usage();
            let frequency = cpu.frequency() as f32;
            let temperature = Self::read_cpu_temperature(i);

            cores.push(CoreInfo {
                id: i,
                usage,
                temperature,
                frequency,
            });
        }

        cores
    }
    pub fn cpu_fan_speed() -> Option<i32> {
        // Try to read fan speed from hwmon
        let hwmon_path = "/sys/class/hwmon";
        
        if let Ok(entries) = fs::read_dir(hwmon_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                
                // Look for fan1_input (most common)
                let fan_input = path.join("fan1_input");
                if fan_input.exists() {
                    if let Ok(fan_str) = fs::read_to_string(&fan_input) {
                        if let Ok(rpm) = fan_str.trim().parse::<i32>() {
                            if rpm > 0 {
                                return Some(rpm);
                            }
                        }
                    }
                }
            }
        }
        
        None
    }

    pub fn current_gov() -> Option<String> {
        Self::read_file_single("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
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

    // System nesnesini parametre olarak alıyoruz
  pub fn cpu_usage(sys: &mut System) -> f32 {
        // CRITICAL: Refresh CPU before reading
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        
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

    // avg_temp artık System parametresi alıyor
    pub fn avg_temp(sys: &mut System) -> i32 {
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
            if let Some(v) = Self::read_file_single(intel_pstate) {
                if let Ok(n) = v.parse::<i32>() {
                    return (Some((n != 0) == false), Some(false));
                }
            }
            return (None, None);
        }

        if cpu_freq.exists() {
            if let Some(v) = Self::read_file_single(cpu_freq) {
                if let Ok(n) = v.parse::<i32>() {
                    return (Some(n != 0), Some(false));
                }
            }
            return (None, None);
        }

        if amd_pstate.exists() {
            if let Some(s) = Self::read_file_single(amd_pstate) {
                if s.trim() == "active" { return (None, Some(true)); }
                return (None, Some(false));
            }
            return (None, None);
        }

        (None, None)
    }

    fn read_file_opt<P: AsRef<Path>>(path: P) -> Option<String> {
        fs::read_to_string(path).ok().map(|s| s.trim().to_string())
    }

    pub fn get_battery_path() -> Option<PathBuf> {
        if CONFIG.has_option("battery", "battery_device") {
            let battery_device = CONFIG.get("battery", "battery_device", "");
            if !battery_device.is_empty() {
                let custom = Path::new(POWER_SUPPLY_DIR).join(&battery_device);
                let type_path = custom.join("type");
                if type_path.is_file() {
                    if let Some(content) = Self::read_file_opt(type_path) {
                        if content.to_lowercase() == "battery" {
                            return Some(custom);
                        }
                    }
                }
            }
        }

        if let Ok(entries) = fs::read_dir(POWER_SUPPLY_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                let type_path = path.join("type");
                if type_path.is_file() {
                    if let Some(content) = Self::read_file_opt(type_path) {
                        if content.to_lowercase() == "battery" {
                            return Some(path);
                        }
                    }
                }
            }
        }

        None
    }

    pub fn battery_info() -> BatteryInfo {
        let battery_path = Self::get_battery_path();

        let mut is_ac_plugged = Some(true);

        if battery_path.is_none() {
            return BatteryInfo {
                is_charging: None,
                is_ac_plugged: Some(true),
                charging_start_threshold: None,
                charging_stop_threshold: None,
                battery_level: None,
                power_consumption: None,
            };
        }

        if let Ok(entries) = fs::read_dir(POWER_SUPPLY_DIR) {
            for entry in entries.flatten() {
                let p = entry.path();
                let t = Self::read_file_opt(p.join("type")).unwrap_or_default();
                if t == "Mains" {
                    let online = Self::read_file_opt(p.join("online")).unwrap_or_default();
                    is_ac_plugged = Some(online == "1");
                }
            }
        }

        let bp = battery_path.unwrap();
        let battery_status = Self::read_file_opt(bp.join("status"));
        let battery_capacity = Self::read_file_opt(bp.join("capacity"));
        let energy_rate = Self::read_file_opt(bp.join("power_now")).or_else(|| Self::read_file_opt(bp.join("current_now")));
        let charge_start = Self::read_file_opt(bp.join("charge_start_threshold")).or_else(|| Self::read_file_opt(bp.join("charge_control_start_threshold")));
        let charge_stop = Self::read_file_opt(bp.join("charge_stop_threshold")).or_else(|| Self::read_file_opt(bp.join("charge_control_end_threshold")));

        let is_charging = battery_status.as_ref().map(|s| s.to_lowercase() == "charging");
        let battery_level = battery_capacity.and_then(|c| c.parse::<u8>().ok());
        let power_consumption = energy_rate.and_then(|e| e.parse::<f32>().ok()).map(|v| v / 1_000_000.0);
        let charging_start_threshold = charge_start.and_then(|s| s.parse::<i32>().ok());
        let charging_stop_threshold = charge_stop.and_then(|s| s.parse::<i32>().ok());

        BatteryInfo {
            is_charging,
            is_ac_plugged,
            charging_start_threshold,
            charging_stop_threshold,
            battery_level,
            power_consumption,
        }
    }

    // turbo_on_suggestion artık System parametresi alıyor
    pub fn turbo_on_suggestion(sys: &mut System) -> bool {
        let usage = Self::cpu_usage(sys);
        if usage >= 20.0 { return true; }
        if usage <= 25.0 && Self::avg_temp(sys) as f32 >= 70.0 { return false; }
        false
    }

    pub fn governor_suggestion() -> Option<String> {
        let batt = Self::battery_info();
        if batt.is_ac_plugged.unwrap_or(true) {
            AVAILABLE_GOVERNORS_SORTED.get(0).cloned()
        } else {
            AVAILABLE_GOVERNORS_SORTED.last().cloned()
        }
    }

    // System nesnesini parametre olarak alıyoruz
  pub fn generate_system_report(&self, sys: &mut System) -> SystemReport {
        // CRITICAL: Ensure CPU is properly refreshed before generating report
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        
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
            current_epp: battery.is_ac_plugged.and_then(|ac| Self::current_epp(ac)),
            current_epb: battery.is_ac_plugged.and_then(|ac| Self::current_epb(ac)),
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
        let _ = s.generate_system_report(&mut sys);
    }
}
