// src/core.rs - OPTIMIZED VERSION
use std::fs::{self, File};
use std::io::{Write, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use sysinfo::System;
use crate::power_helper::SYSTEMCTL_EXISTS;
use chrono::Local;
use anyhow::{Result, bail, Context};

use crate::config::CONFIG;
use crate::globals::AVAILABLE_GOVERNORS_SORTED;

// ============================================================================
// OPTIMIZATION: Cached System Wrapper
// ============================================================================
pub struct CachedSystem {
    sys: System,
    last_refresh: Instant,
    refresh_interval: Duration,
}

impl CachedSystem {
    pub fn new(refresh_interval_secs: u64) -> Self {
        Self {
            sys: System::new_all(),
            last_refresh: Instant::now() - Duration::from_secs(999), // Force initial refresh
            refresh_interval: Duration::from_secs(refresh_interval_secs),
        }
    }

    pub fn get_refreshed_system(&mut self) -> &mut System {
        if self.last_refresh.elapsed() > self.refresh_interval {
            self.sys.refresh_cpu();
            std::thread::sleep(Duration::from_millis(200));
            self.sys.refresh_cpu();
            self.last_refresh = Instant::now();
        }
        &mut self.sys
    }

    pub fn force_refresh(&mut self) {
        self.sys.refresh_cpu();
        std::thread::sleep(Duration::from_millis(200));
        self.sys.refresh_cpu();
        self.last_refresh = Instant::now();
    }
}

// ============================================================================
// OPTIMIZATION: Temperature Sensor Cache
// ============================================================================
pub struct TempSensorCache {
    sensor_paths: HashMap<usize, PathBuf>,
    package_temp_path: Option<PathBuf>,
    last_scan: Instant,
}

impl TempSensorCache {
    pub fn new() -> Self {
        let mut cache = Self {
            sensor_paths: HashMap::new(),
            package_temp_path: None,
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
                        // Cache package temp (temp1)
                        let pkg_temp = path.join("temp1_input");
                        if pkg_temp.exists() {
                            self.package_temp_path = Some(pkg_temp);
                        }
                        
                        // Cache core temps (temp2+)
                        for temp_id in 2..20 {
                            let temp_file = path.join(format!("temp{}_input", temp_id));
                            if temp_file.exists() {
                                let core_id = temp_id - 2;
                                self.sensor_paths.insert(core_id, temp_file);
                            }
                        }
                        break; // Use first matching sensor
                    }
                }
            }
        }
        
        self.last_scan = Instant::now();
    }

    pub fn read_core_temp(&self, core_id: usize) -> f32 {
        // Try specific core sensor first
        if let Some(path) = self.sensor_paths.get(&core_id) {
            if let Ok(temp_str) = fs::read_to_string(path) {
                if let Ok(temp) = temp_str.trim().parse::<f32>() {
                    return temp / 1000.0;
                }
            }
        }
        
        // Fallback to package temp
        if let Some(ref path) = self.package_temp_path {
            if let Ok(temp_str) = fs::read_to_string(path) {
                if let Ok(temp) = temp_str.trim().parse::<f32>() {
                    return temp / 1000.0;
                }
            }
        }
        
        0.0
    }

    pub fn read_package_temp(&self) -> f32 {
        if let Some(ref path) = self.package_temp_path {
            if let Ok(temp_str) = fs::read_to_string(path) {
                if let Ok(temp) = temp_str.trim().parse::<f32>() {
                    return temp / 1000.0;
                }
            }
        }
        0.0
    }

    // Rescan if sensors might have changed (rare)
    pub fn maybe_rescan(&mut self) {
        if self.last_scan.elapsed() > Duration::from_secs(300) {
            self.scan_sensors();
        }
    }
}

// Global instances with lazy initialization
lazy_static::lazy_static! {
    static ref TEMP_CACHE: Arc<Mutex<TempSensorCache>> = Arc::new(Mutex::new(TempSensorCache::new()));
    static ref CACHED_SYSTEM: Arc<Mutex<CachedSystem>> = Arc::new(Mutex::new(CachedSystem::new(2)));
}

// ============================================================================
// Constants
// ============================================================================
const SCRIPTS_DIR: &str = "/usr/local/share/auto-cpufreq/scripts/";
const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply/";
pub const GITHUB: &str = "https://github.com/Zamanhuseyinli/auto-cpufreq-rust";

pub const ALL_GOVERNORS: &[&str] = &[
    "performance", 
    "ondemand", 
    "conservative", 
    "schedutil", 
    "userspace", 
    "powersave"
];

fn read_auto_cpufreq_file(sub_path: &str) -> String {
    let path = format!("/usr/local/share/auto-cpufreq/scripts/{}", sub_path);
    fs::read_to_string(&path).unwrap_or_else(|_| {
        eprintln!("Warning: File {} not found!", path);
        String::new()
    })
}

pub fn install_script() -> String { read_auto_cpufreq_file("auto-cpufreq-install.sh") }
pub fn remove_script() -> String { read_auto_cpufreq_file("auto-cpufreq-remove.sh") }
pub fn cpufreqctl_script() -> String { read_auto_cpufreq_file("cpufreqctl.sh") }
pub fn systemd_service() -> String { read_auto_cpufreq_file("auto-cpufreq.service") }
pub fn openrc_service() -> String { read_auto_cpufreq_file("auto-cpufreq-openrc") }
pub fn dinit_service() -> String { read_auto_cpufreq_file("auto-cpufreq-dinit") }
pub fn runit_service() -> String { read_auto_cpufreq_file("auto-cpufreq-runit") }
pub fn s6_service() -> String { read_auto_cpufreq_file("auto-cpufreq-s6/run") }

// ============================================================================
// Global state structures
// ============================================================================
pub struct AutoCpuFreqState {
    pub cpu_count: usize,
    pub performance_load_threshold: f32,
    pub powersave_load_threshold: f32,
    pub stats_file_path: PathBuf,
    pub governor_override_path: PathBuf,
    pub turbo_override_path: PathBuf,
    pub is_aur: bool,
}

impl AutoCpuFreqState {
    pub fn new() -> Self {
        let cpu_count = num_cpus::get();
        
        let (stats_path, gov_path, turbo_path) = (
                PathBuf::from("/var/run/auto-cpufreq.stats"),
                PathBuf::from("/opt/auto-cpufreq/override.pickle"),
                PathBuf::from("/opt/auto-cpufreq/turbo-override.pickle"),
        );

        Self {
            cpu_count,
            performance_load_threshold: (50 * cpu_count) as f32 / 100.0,
            powersave_load_threshold: (75 * cpu_count) as f32 / 100.0,
            stats_file_path: stats_path,
            governor_override_path: gov_path,
            turbo_override_path: turbo_path,
            is_aur: Self::check_aur_install(),
        }
    }

    fn check_aur_install() -> bool {
        Path::new("/etc/arch-release").exists() && 
        Command::new("pacman")
            .args(&["-Qs", "auto-cpufreq"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }
}

// ============================================================================
// Version management
// ============================================================================
pub fn get_version() -> Result<String> {
    let state = AutoCpuFreqState::new();
    
    if state.is_aur {
        let output = Command::new("pacman")
            .args(&["-Qi", "auto-cpufreq"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        stdout.lines()
            .find(|line| line.contains("Version"))
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Version not found"))
    } else {
        get_formatted_version()
    }
}

pub fn get_formatted_version() -> Result<String> {
    let version = env!("CARGO_PKG_VERSION");
    Ok(version.to_string())
}

pub fn app_version() {
    match get_version() {
        Ok(v) => println!("auto-cpufreq version: {}", v),
        Err(e) => eprintln!("Error getting version: {}", e),
    }
}

pub fn check_for_update() -> Result<bool> {
    let latest_url = format!("{}/releases/latest", GITHUB.replace("github.com", "api.github.com/repos"));
    
    let client = reqwest::blocking::Client::new();
    let response = client.get(&latest_url)
        .header("User-Agent", "auto-cpufreq-rust")
        .send()?;

    if response.status().as_u16() == 200 {
        let json: serde_json::Value = response.json()?;
        let latest = json["tag_name"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No tag_name in response"))?;
        
        let current = format!("v{}", env!("CARGO_PKG_VERSION"));
        
        if latest != current {
            println!("Updates available:");
            println!("Current version: {}", current);
            println!("Latest version: {}", latest);
            Ok(true)
        } else {
            println!("auto-cpufreq is up to date");
            Ok(false)
        }
    } else {
        bail!("Failed to fetch release info: {}", response.status());
    }
}

// ============================================================================
// Governor management
// ============================================================================
#[derive(Debug, Clone, PartialEq)]
pub enum GovernorOverride {
    Default,
    Powersave,
    Performance,
}

impl GovernorOverride {
    pub fn from_str(s: &str) -> Self {
        match s {
            "powersave" => Self::Powersave,
            "performance" => Self::Performance,
            _ => Self::Default,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            Self::Powersave => "powersave",
            Self::Performance => "performance",
            Self::Default => "default",
        }
    }
}

pub fn get_override(state: &AutoCpuFreqState) -> GovernorOverride {
    if state.governor_override_path.exists() {
        fs::read_to_string(&state.governor_override_path)
            .ok()
            .map(|s| GovernorOverride::from_str(s.trim()))
            .unwrap_or(GovernorOverride::Default)
    } else {
        GovernorOverride::Default
    }
}

pub fn set_override(state: &AutoCpuFreqState, override_val: &str) -> Result<()> {
    match override_val {
        "powersave" | "performance" => {
            fs::write(&state.governor_override_path, override_val)?;
            println!("Set governor override to {}", override_val);
        }
        "reset" => {
            if state.governor_override_path.exists() {
                fs::remove_file(&state.governor_override_path)?;
            }
            println!("Governor override removed");
        }
        _ => {
            println!("Invalid option.");
            println!("Use force=performance, force=powersave, or force=reset");
        }
    }
    Ok(())
}

// ============================================================================
// Turbo management
// ============================================================================
#[derive(Debug, Clone, PartialEq)]
pub enum TurboOverride {
    Auto,
    Never,
    Always,
}

impl TurboOverride {
    pub fn from_str(s: &str) -> Self {
        match s {
            "never" => Self::Never,
            "always" => Self::Always,
            _ => Self::Auto,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            Self::Never => "never",
            Self::Always => "always",
            Self::Auto => "auto",
        }
    }
}

pub fn get_turbo_override(state: &AutoCpuFreqState) -> TurboOverride {
    if state.turbo_override_path.exists() {
        fs::read_to_string(&state.turbo_override_path)
            .ok()
            .map(|s| TurboOverride::from_str(s.trim()))
            .unwrap_or(TurboOverride::Auto)
    } else {
        TurboOverride::Auto
    }
}

pub fn set_turbo_override(state: &AutoCpuFreqState, override_val: &str) -> Result<()> {
    match override_val {
        "never" | "always" => {
            fs::write(&state.turbo_override_path, override_val)?;
            println!("Set turbo boost override to {}", override_val);
        }
        "auto" => {
            if state.turbo_override_path.exists() {
                fs::remove_file(&state.turbo_override_path)?;
            }
            println!("Turbo override removed");
        }
        _ => {
            println!("Invalid option.");
            println!("Use turbo=always, turbo=never, or turbo=auto");
        }
    }
    Ok(())
}

pub fn turbo(value: Option<bool>) -> Result<bool> {
    let p_state = Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo");
    let cpufreq = Path::new("/sys/devices/system/cpu/cpufreq/boost");
    let amd_pstate = Path::new("/sys/devices/system/cpu/amd_pstate/status");
    
    let (control_file, inverse) = if p_state.exists() {
        (p_state, true)
    } else if cpufreq.exists() {
        (cpufreq, false)
    } else if amd_pstate.exists() {
        let status = fs::read_to_string(amd_pstate)?.trim().to_string();
        if status == "active" {
            println!("CPU turbo is controlled by amd-pstate-epp driver");
        }
        return Ok(false);
    } else {
        println!("Warning: CPU turbo is not available");
        return Ok(false);
    };
    
    if let Some(val) = value {
        let write_val = if inverse { !val } else { val };
        match fs::write(control_file, format!("{}\n", write_val as u8)) {
            Ok(_) => {}
            Err(_) => {
                println!("Warning: Changing CPU turbo is not supported. Skipping.");
                return Ok(false);
            }
        }
    }
    
    let current = fs::read_to_string(control_file)?
        .trim()
        .parse::<u8>()?;
    
    Ok((current != 0) ^ inverse)
}

pub fn get_turbo() {
    match turbo(None) {
        Ok(state) => println!("Currently turbo boost is: {}", if state { "on" } else { "off" }),
        Err(e) => eprintln!("Error getting turbo state: {}", e),
    }
}

pub fn set_turbo(value: bool) {
    println!("Setting turbo boost: {}", if value { "on" } else { "off" });
    let _ = turbo(Some(value));
}

// ============================================================================
// Distribution info
// ============================================================================
pub fn distro_info() -> Result<()> {
    let mut dist_name = "UNKNOWN distro".to_string();
    let mut version = "UNKNOWN version".to_string();

    if Path::new("/etc/os-release").exists() {
        let file = File::open("/etc/os-release")?;
        let reader = BufReader::new(file);
        
        for line in reader.lines() {
            let line = line?;
            if line.starts_with("NAME=") {
                dist_name = line.trim_start_matches("NAME=")
                    .trim_matches('"')
                    .to_string();
            } else if line.starts_with("VERSION=") {
                version = line.trim_start_matches("VERSION=")
                    .trim_matches('"')
                    .to_string();
            }
        }
    }

    println!("Linux distro: {} {}", dist_name, version);
    println!("Linux kernel: {}", System::kernel_version().unwrap_or_default());
    
    Ok(())
}

// ============================================================================
// OPTIMIZED: Temperature reading functions
// ============================================================================
pub fn read_cpu_temperature(core_id: usize) -> f32 {
    TEMP_CACHE.lock().unwrap().read_core_temp(core_id)
}

pub fn read_package_temperature() -> f32 {
    TEMP_CACHE.lock().unwrap().read_package_temp()
}

// ============================================================================
// System info
// ============================================================================
pub fn sysinfo() -> Result<()> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo")?;
    let model_name = cpuinfo
        .lines()
        .find(|line| line.contains("model name"))
        .and_then(|line| line.split(':').nth(1))
        .map(|s| s.trim())
        .unwrap_or("Unknown");
    
    println!("Processor: {}", model_name);
    
    let cpu_count = num_cpus::get();
    println!("Cores: {}", cpu_count);
    
    let arch = std::env::consts::ARCH;
    println!("Architecture: {}", arch);
    
    let driver = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();
    println!("Driver: {}", driver);
    
    // OPTIMIZED: Use cached system
    let mut cached_sys = CACHED_SYSTEM.lock().unwrap();
    let sys = cached_sys.get_refreshed_system();
    
    if let Some(cpu) = sys.cpus().first() {
        println!("\n{}", "-".repeat(30) + " Current CPU stats " + &"-".repeat(30));
        println!("\nCPU max frequency: {:.0} MHz", cpu.frequency());
    }
    
    println!("\n{:<6} {:<8} {:<16} {:<10}", "Core", "Usage", "Temperature", "Frequency");
    
    for (i, cpu) in sys.cpus().iter().enumerate() {
        let temp = read_cpu_temperature(i);
        let temp_str = if temp > 0.0 {
            format!("{:.0} °C", temp)
        } else {
            "-- °C".to_string()
        };
        
        println!("{:<6} {:<8.1}% {:<16} {:.0} MHz", 
            format!("CPU{}", i),
            cpu.cpu_usage(),
            temp_str,
            cpu.frequency()
        );
    }
    
    let pkg_temp = read_package_temperature();
    if pkg_temp > 0.0 {
        println!("\nPackage temperature: {:.1} °C", pkg_temp);
    }
    
    Ok(())
}

// ============================================================================
// Power supply / charging detection
// ============================================================================
pub fn get_power_supply_ignore_list() -> Vec<String> {
    vec!["hidpp_battery".to_string()]
}

pub fn charging() -> Result<bool> {
    let power_dir = Path::new(POWER_SUPPLY_DIR);
    
    if !power_dir.exists() {
        return Ok(true);
    }
    
    let mut entries: Vec<_> = fs::read_dir(power_dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    
    let ignore_list = get_power_supply_ignore_list();
    
    if entries.is_empty() {
        return Ok(true);
    }
    
    for entry in entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        if ignore_list.iter().any(|ignored| name_str.contains(ignored)) {
            continue;
        }
        
        let supply_path = entry.path();
        let type_path = supply_path.join("type");
        
        if !type_path.exists() {
            continue;
        }
        
        let supply_type = fs::read_to_string(&type_path)?.trim().to_string();
        
        if supply_type == "Mains" {
            let online_path = supply_path.join("online");
            if online_path.exists() {
                let online = fs::read_to_string(&online_path)?.trim().to_string();
                if online == "1" {
                    return Ok(true);
                }
            }
        } else if supply_type == "Battery" {
            let status_path = supply_path.join("status");
            if status_path.exists() {
                let status = fs::read_to_string(&status_path)?.trim().to_string();
                if status == "Discharging" {
                    return Ok(false);
                }
            }
        }
    }
    
    Ok(true)
}

// ============================================================================
// Governor functions
// ============================================================================
pub fn get_current_gov() -> Result<String> {
    let output = Command::new("cpufreqctl.auto-cpufreq")
        .arg("--governor")
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let gov = stdout.split_whitespace().next().unwrap_or("unknown");
    
    Ok(gov.to_string())
}

pub fn print_current_gov() {
    match get_current_gov() {
        Ok(gov) => println!("Currently using: {} governor", gov),
        Err(e) => eprintln!("Error getting governor: {}", e),
    }
}

// ============================================================================
// cpufreqctl deployment
// ============================================================================
pub fn cpufreqctl() -> Result<()> {
    let target = "/usr/local/bin/cpufreqctl.auto-cpufreq";
    
    if !Path::new(target).exists() {
        let source = PathBuf::from(SCRIPTS_DIR).join("cpufreqctl.sh");
        fs::copy(source, target)?;
        
        Command::new("chmod")
            .args(&["a+x", target])
            .status()?;
    }
    
    Ok(())
}

pub fn cpufreqctl_restore() -> Result<()> {
    let target = "/usr/local/bin/cpufreqctl.auto-cpufreq";
    
    if Path::new(target).exists() {
        fs::remove_file(target)?;
    }
    
    Ok(())
}

fn deploy_cpufreqctl() -> Result<()> {
    let target = "/usr/local/bin/cpufreqctl.auto-cpufreq";
    
    if !Path::new(target).exists() {
        println!("\n* Deploying cpufreqctl helper script");
        fs::write(target, cpufreqctl_script())?;

        Command::new("chmod")
            .args(&["+x", target])
            .status()?;
    }
    
    Ok(())
}

fn remove_cpufreqctl() -> Result<()> {
    let target = "/usr/local/bin/cpufreqctl.auto-cpufreq";
    
    if Path::new(target).exists() {
        println!("\n* Removing cpufreqctl helper script");
        fs::remove_file(target)?;
    }
    
    Ok(())
}

// ============================================================================
// Stats file update function
// ============================================================================
pub fn update_stats_file() -> Result<()> {
    let state = AutoCpuFreqState::new();
    
    if let Some(parent) = state.stats_file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // OPTIMIZED: Use String buffer instead of multiple allocations
    let mut stats = String::with_capacity(2048);
    
    use std::fmt::Write as FmtWrite;
    
    let _ = writeln!(&mut stats, "\n{}", "=".repeat(80));
    let _ = writeln!(&mut stats, "auto-cpufreq daemon - {}", 
        Local::now().format("%Y-%m-%d %H:%M:%S"));
    let _ = writeln!(&mut stats, "{}\n", "=".repeat(80));
    
    // OPTIMIZED: Use cached system
    let mut cached_sys = CACHED_SYSTEM.lock().unwrap();
    let sys = cached_sys.get_refreshed_system();
    
    let cpu_usage: f32 = sys.cpus().iter()
        .map(|c| c.cpu_usage())
        .sum::<f32>() / sys.cpus().len() as f32;
    
    let loadavg = System::load_average();
    
    let _ = writeln!(&mut stats, "CPU usage: {:.1}%", cpu_usage);
    let _ = writeln!(&mut stats, "Load: {:.2}, {:.2}, {:.2}", 
        loadavg.one, loadavg.five, loadavg.fifteen);
    
    if let Ok(gov) = get_current_gov() {
        let _ = writeln!(&mut stats, "Governor: {}", gov);
    }
    
    if let Ok(turbo_state) = turbo(None) {
        let _ = writeln!(&mut stats, "Turbo: {}", if turbo_state { "On" } else { "Off" });
    }
    
    if let Ok(is_charging) = charging() {
        let _ = writeln!(&mut stats, "Battery: {}", 
            if is_charging { "Charging" } else { "Discharging" });
    }
    
    let _ = writeln!(&mut stats, "\n{}", "-".repeat(80));
    
    fs::write(&state.stats_file_path, stats)?;
    
    Ok(())
}

// ============================================================================
// Load information
// ============================================================================
pub fn get_load() -> (f64, f64) {
    // OPTIMIZED: Use cached system
    let mut cached_sys = CACHED_SYSTEM.lock().unwrap();
    let sys = cached_sys.get_refreshed_system();
    
    let cpu_usage: f64 = sys.cpus().iter()
        .map(|cpu| cpu.cpu_usage() as f64)
        .sum::<f64>() / sys.cpus().len() as f64;
    
    let loadavg = System::load_average();
    let load1m = loadavg.one;
    
    println!("\nTotal CPU usage: {:.1}%", cpu_usage);
    println!("Total system load: {:.2}", load1m);
    
    // OPTIMIZED: Calculate average temperature using cached sensors
    let temp_cache = TEMP_CACHE.lock().unwrap();
    let temps: Vec<f32> = (0..sys.cpus().len())
        .map(|i| temp_cache.read_core_temp(i))
        .filter(|&t| t > 0.0)
        .collect();
    
    if !temps.is_empty() {
        let avg_temp: f32 = temps.iter().sum::<f32>() / temps.len() as f32;
        println!("Average temp. of all cores: {:.1} °C\n", avg_temp);
    } else {
        println!("Average temp. of all cores: -- °C\n");
    }
    
    (cpu_usage, load1m)
}

pub fn display_system_load_avg() {
    let loadavg = System::load_average();
    println!(" (load average: {:.2}, {:.2}, {:.2})", 
        loadavg.one, loadavg.five, loadavg.fifteen);
}

// ============================================================================
// Utility functions
// ============================================================================
pub fn footer(length: usize) {
    println!("\n{}\n", "-".repeat(length));
}

pub fn root_check() -> Result<()> {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("\n{}\n", "-".repeat(33) + " Root check " + &"-".repeat(34));
        eprintln!("ERROR:\n");
        eprintln!("Must be run as root for this functionality to work");
        bail!("Not running as root");
    }
    Ok(())
}

pub fn countdown(seconds: u64) {
    use std::io::stdout;
    
    std::env::set_var("TERM", "xterm");
    
    print!("\t\t\"auto-cpufreq\" is about to refresh ");
    stdout().flush().unwrap();
    
    for remaining in (0..=seconds).rev() {
        if remaining <= 3 {
            print!(".");
            stdout().flush().unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1000 * seconds / 3));
    }
    
    println!("\n\t\tExecuted on: {}", Local::now().format("%c"));
}

// ============================================================================
// OPTIMIZED: Improved daemon detection
// ============================================================================
pub fn is_running(program: &str, argument: &str) -> bool {
    // OPTIMIZATION: Try fast pidof first
    if let Ok(output) = Command::new("pidof")
        .arg("-x")
        .arg(program)
        .output()
    {
        if !output.stdout.is_empty() {
            // Found PID, now verify it has the right argument
            return check_proc_daemon_status(program, argument);
        }
    }
    
    // Fallback to full check
    check_proc_daemon_status(program, argument)
}

fn check_proc_daemon_status(program: &str, argument: &str) -> bool {
    let proc_path = Path::new("/proc");
    
    if !proc_path.exists() {
        return is_running_sysinfo(program, argument);
    }
    
    let entries = match fs::read_dir(proc_path) {
        Ok(e) => e,
        Err(_) => return is_running_sysinfo(program, argument),
    };
    
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        
        if !file_name_str.chars().all(|c| c.is_numeric()) {
            continue;
        }
        
        let cmdline_path = path.join("cmdline");
        if let Ok(cmdline_bytes) = fs::read(&cmdline_path) {
            let cmdline_str = String::from_utf8_lossy(&cmdline_bytes);
            let args: Vec<&str> = cmdline_str.split('\0').filter(|s| !s.is_empty()).collect();
            
            if args.is_empty() {
                continue;
            }
            
            let has_program = args.iter().any(|arg| arg.contains(program));
            
            if has_program {
                let has_argument = args.iter().any(|arg| arg.contains(argument));
                
                if has_argument {
                    return true;
                }
            }
        }
    }
    
    false
}

fn is_running_sysinfo(program: &str, argument: &str) -> bool {
    let mut sys = System::new();
    sys.refresh_processes();
    
    for (_, process) in sys.processes() {
        let exe_path = process.exe()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        let cmd = process.cmd();
        let name = process.name();
        
        let has_program = 
            name.contains(program) ||
            exe_path.contains(program) ||
            cmd.iter().any(|s| s.contains(program));
        
        let has_argument = cmd.iter().any(|s| s.contains(argument));
        
        if has_program && has_argument {
            return true;
        }
    }
    
    false
}

pub fn daemon_running_check() -> Result<()> {
    if is_running("auto-cpufreq", "--daemon") {
        println!("\n{}\n", "-".repeat(24) + " auto-cpufreq running " + &"-".repeat(30));
        println!("ERROR: auto-cpufreq is running in daemon mode.");
        println!("\nMake sure to stop the daemon before running with --live or --monitor mode");
        footer(79);
        bail!("Daemon already running");
    }

    Ok(())
}

pub fn not_running_daemon_check() -> Result<()> {
    if !is_running("auto-cpufreq", "--daemon") {
        if *SYSTEMCTL_EXISTS {
            let status = Command::new("systemctl")
                .args(&["is-active", "auto-cpufreq"])
                .output();

            if let Ok(out) = status {
                let active = String::from_utf8_lossy(&out.stdout);
                if active.trim() == "active" {
                    return Ok(());
                }
            }
        }

        println!("\n{}\n", "-".repeat(24) + " auto-cpufreq not running " + &"-".repeat(30));
        println!("ERROR: auto-cpufreq is not running in daemon mode.");
        println!("\nMake sure to run \"sudo auto-cpufreq --install\" first");
        footer(79);
        bail!("Daemon not running");
    }

    Ok(())
}

// ============================================================================
// Install/Remove script runners
// ============================================================================
pub fn run_install_script() -> Result<()> {
    println!("\n* Running pre-installation script");
    
    let temp_script = "/tmp/auto-cpufreq-install.sh";
    fs::write(temp_script, install_script())?;
    
    Command::new("chmod")
        .args(&["+x", temp_script])
        .status()?;
    
    let status = Command::new("sh")
        .arg(temp_script)
        .status()?;
    
    let _ = fs::remove_file(temp_script);
    
    if status.success() {
        println!("* Pre-installation script completed successfully");
        Ok(())
    } else {
        println!("* Warning: Pre-installation script completed with errors (continuing anyway)");
        Ok(())
    }
}

pub fn run_remove_script() -> Result<()> {
    println!("\n* Running post-removal script");
    
    let temp_script = "/tmp/auto-cpufreq-remove.sh";
    fs::write(temp_script, remove_script())?;
    
    Command::new("chmod")
        .args(&["+x", temp_script])
        .status()?;
    
    let status = Command::new("sh")
        .arg(temp_script)
        .status()?;
    
    let _ = fs::remove_file(temp_script);
    
    if status.success() {
        println!("* Post-removal script completed successfully");
        Ok(())
    } else {
        println!("* Warning: Post-removal script completed with errors (continuing anyway)");
        Ok(())
    }
}

pub fn get_install_script() -> String { 
    install_script()
}

pub fn get_remove_script() -> String { 
    remove_script()
}

// ============================================================================
// Init system detection and daemon installation/removal
// ============================================================================
pub fn detect_init_system() -> &'static str {
    let output = Command::new("ps")
        .args(&["-p", "1", "-o", "comm="])
        .output();
    
    if let Ok(out) = output {
        let init = String::from_utf8_lossy(&out.stdout).trim().to_string();
        match init.as_str() {
            "systemd" => "systemd",
            "init" => "openrc",
            "dinit" => "dinit",
            "runit" => "runit",
            "s6-svscan" => "s6",
            _ => "unknown"
        }
    } else {
        "unknown"
    }
}

pub fn install_daemon() -> Result<()> {
    let init = detect_init_system();
    
    println!("\n{}", "=".repeat(80));
    println!("Installing auto-cpufreq daemon ({} detected)", init);
    println!("{}", "=".repeat(80));
    
    run_install_script()?;
    
    deploy_cpufreqctl()?;
    
    match init {
        "systemd" => install_systemd(),
        "openrc" => install_openrc(),
        "dinit" => install_dinit(),
        "runit" => install_runit(),
        "s6" => install_s6(),
        _ => {
            println!("\n* Unsupported init system detected, could not install the daemon\n");
            println!("* Please open an issue on https://github.com/Zamanhuseyinli/auto-cpufreq-rust\n");
            bail!("Unsupported init system: {}", init)
        }
    }
}

pub fn remove_daemon() -> Result<()> {
    let init = detect_init_system();
    
    println!("\n{}", "=".repeat(80));
    println!("Removing auto-cpufreq daemon ({} detected)", init);
    println!("{}", "=".repeat(80));
    
    let result = match init {
        "systemd" => remove_systemd(),
        "openrc" => remove_openrc(),
        "dinit" => remove_dinit(),
        "runit" => remove_runit(),
        "s6" => remove_s6(),
        _ => {
            println!("\n* Unsupported init system detected, could not remove the daemon");
            println!("* Please open an issue on https://github.com/Zamanhuseyinli/auto-cpufreq-rust\n");
            bail!("Unsupported init system: {}", init)
        }
    };
    
    remove_cpufreqctl()?;
    
    run_remove_script()?;
    
    result
}

// ============================================================================
// systemd
// ============================================================================
fn install_systemd() -> Result<()> {
    println!("\n* Deploying auto-cpufreq systemd unit file");
    
    fs::write("/etc/systemd/system/auto-cpufreq.service", systemd_service())?;
    
    println!("\n* Reloading systemd manager configuration");
    Command::new("systemctl")
        .arg("daemon-reload")
        .status()?;
    
    println!("\n* Starting auto-cpufreq daemon (systemd) service");
    Command::new("systemctl")
        .args(&["start", "auto-cpufreq"])
        .status()?;
    
    println!("\n* Enabling auto-cpufreq daemon (systemd) at boot");
    Command::new("systemctl")
        .args(&["enable", "auto-cpufreq"])
        .status()?;
    
    Ok(())
}

fn remove_systemd() -> Result<()> {
    println!("\n* Stopping auto-cpufreq daemon (systemd) service");
    let _ = Command::new("systemctl")
        .args(&["stop", "auto-cpufreq"])
        .status();
    
    println!("\n* Disabling auto-cpufreq daemon (systemd) at boot");
    let _ = Command::new("systemctl")
        .args(&["disable", "auto-cpufreq"])
        .status();
    
    println!("\n* Removing auto-cpufreq daemon (systemd) unit file");
    let _ = fs::remove_file("/etc/systemd/system/auto-cpufreq.service");
    
    println!("\n* Reloading systemd manager configuration");
    Command::new("systemctl")
        .arg("daemon-reload")
        .status()?;
    
    println!("\nReset failed");
    Command::new("systemctl")
        .arg("reset-failed")
        .status()?;
    
    Ok(())
}

// ============================================================================
// OpenRC
// ============================================================================
fn install_openrc() -> Result<()> {
    println!("\n* Deploying auto-cpufreq openrc unit file");
    
    fs::write("/etc/init.d/auto-cpufreq", openrc_service())?;
    
    Command::new("chmod")
        .args(&["+x", "/etc/init.d/auto-cpufreq"])
        .status()?;
    
    println!("\n* Starting auto-cpufreq daemon (openrc) service");
    Command::new("rc-service")
        .args(&["auto-cpufreq", "start"])
        .status()?;
    
    println!("\n* Enabling auto-cpufreq daemon (openrc) at boot");
    Command::new("rc-update")
        .args(&["add", "auto-cpufreq"])
        .status()?;
    
    Ok(())
}

fn remove_openrc() -> Result<()> {
    println!("\n* Stopping auto-cpufreq daemon (openrc) service");
    let _ = Command::new("rc-service")
        .args(&["auto-cpufreq", "stop"])
        .status();
    
    println!("\n* Disabling auto-cpufreq daemon (openrc) at boot");
    let _ = Command::new("rc-update")
        .args(&["del", "auto-cpufreq"])
        .status();
    
    println!("\n* Removing auto-cpufreq daemon (openrc) unit file");
    let _ = fs::remove_file("/etc/init.d/auto-cpufreq");
    
    Ok(())
}

// ============================================================================
// dinit
// ============================================================================
fn install_dinit() -> Result<()> {
    println!("\n* Deploying auto-cpufreq (dinit) unit file");
    
    fs::write("/etc/dinit.d/auto-cpufreq", dinit_service())?;
    
    println!("\n* Starting auto-cpufreq daemon (dinit) service");
    Command::new("dinitctl")
        .args(&["start", "auto-cpufreq"])
        .status()?;
    
    println!("\n* Enabling auto-cpufreq daemon (dinit) at boot");
    Command::new("dinitctl")
        .args(&["enable", "auto-cpufreq"])
        .status()?;
    
    Ok(())
}

fn remove_dinit() -> Result<()> {
    println!("\n* Stopping auto-cpufreq daemon (dinit) service");
    let _ = Command::new("dinitctl")
        .args(&["stop", "auto-cpufreq"])
        .status();
    
    println!("\n* Disabling auto-cpufreq daemon (dinit) at boot");
    let _ = Command::new("dinitctl")
        .args(&["disable", "auto-cpufreq"])
        .status();
    
    println!("\n* Removing auto-cpufreq daemon (dinit) unit file");
    let _ = fs::remove_file("/etc/dinit.d/auto-cpufreq");
    
    Ok(())
}

// ============================================================================
// runit
// ============================================================================
fn install_runit() -> Result<()> {
    let (sv_path, service_path) = if Path::new("/etc/os-release").exists() {
        let os_release = fs::read_to_string("/etc/os-release")?;
        let mut distro_id = String::new();
        
        for line in os_release.lines() {
            if line.starts_with("ID=") {
                distro_id = line.trim_start_matches("ID=").trim_matches('"').to_string();
                break;
            }
        }
        
        match distro_id.as_str() {
            "void" => ("/etc", "/var"),
            "artix" => ("/etc/runit", "/run/runit"),
            _ => {
                println!("\n* Runit init detected but your distro is not supported\n");
                println!("* Please open an issue on https://github.com/Zamanhuseyinli/auto-cpufreq-rust\n");
                bail!("Unsupported runit distro: {}", distro_id);
            }
        }
    } else {
        bail!("Could not detect distro for runit");
    };
    
    println!("\n* Deploying auto-cpufreq (runit) unit file");
    
    let sv_dir = format!("{}/sv/auto-cpufreq", sv_path);
    fs::create_dir_all(&sv_dir)?;
    
    let run_script = format!("{}/run", sv_dir);
    fs::write(&run_script, runit_service())?;
    
    Command::new("chmod")
        .args(&["+x", &run_script])
        .status()?;
    
    println!("\n* Creating symbolic link ({}/service/auto-cpufreq -> {}/sv/auto-cpufreq)", service_path, sv_path);
    
    let service_link = format!("{}/service/auto-cpufreq", service_path);
    let _ = fs::remove_file(&service_link);
    
    std::os::unix::fs::symlink(&sv_dir, &service_link)?;
    
    println!("\n* Starting auto-cpufreq daemon (runit)");
    Command::new("sv")
        .args(&["start", "auto-cpufreq"])
        .status()?;
    
    Command::new("sv")
        .args(&["up", "auto-cpufreq"])
        .status()?;
    
    Ok(())
}

fn remove_runit() -> Result<()> {
    let (sv_path, service_path) = if Path::new("/etc/os-release").exists() {
        let os_release = fs::read_to_string("/etc/os-release")?;
        let mut distro_id = String::new();
        
        for line in os_release.lines() {
            if line.starts_with("ID=") {
                distro_id = line.trim_start_matches("ID=").trim_matches('"').to_string();
                break;
            }
        }
        
        match distro_id.as_str() {
            "void" => ("/etc", "/var"),
            "artix" => ("/etc/runit", "/run/runit"),
            _ => bail!("Unsupported runit distro"),
        }
    } else {
        bail!("Could not detect distro");
    };
    
    println!("\n* Stopping auto-cpufreq daemon (runit) service");
    let _ = Command::new("sv")
        .args(&["stop", "auto-cpufreq"])
        .status();
    
    println!("\n* Removing auto-cpufreq daemon (runit) unit file");
    let _ = fs::remove_dir_all(format!("{}/sv/auto-cpufreq", sv_path));
    let _ = fs::remove_file(format!("{}/service/auto-cpufreq", service_path));
    
    Ok(())
}

// ============================================================================
// s6
// ============================================================================
fn install_s6() -> Result<()> {
    println!("\n* Deploying auto-cpufreq (s6) unit file");
    
    let s6_dir = "/etc/s6/sv/auto-cpufreq";
    fs::create_dir_all(s6_dir)?;
    
    let run_script = format!("{}/run", s6_dir);
    fs::write(&run_script, s6_service())?;
    
    Command::new("chmod")
        .args(&["+x", &run_script])
        .status()?;
    
    println!("\n* Add auto-cpufreq service (s6) to default bundle");
    Command::new("s6-service")
        .args(&["add", "default", "auto-cpufreq"])
        .status()?;
    
    println!("\n* Starting auto-cpufreq daemon (s6)");
    Command::new("s6-rc")
        .args(&["-u", "change", "auto-cpufreq", "default"])
        .status()?;
    
    println!("\n* Update daemon service bundle (s6)");
    Command::new("s6-db-reload")
        .status()?;
    
    Ok(())
}

fn remove_s6() -> Result<()> {
    println!("\n* Disabling auto-cpufreq daemon (s6) at boot");
    let _ = Command::new("s6-service")
        .args(&["delete", "default", "auto-cpufreq"])
        .status();
    
    println!("\n* Removing auto-cpufreq daemon (s6) unit file");
    let _ = fs::remove_dir_all("/etc/s6/sv/auto-cpufreq");
    
    println!("\n* Update daemon service bundle (s6)");
    Command::new("s6-db-reload")
        .status()?;
    
    Ok(())
}

// ============================================================================
// Automatic frequency adjustment - Main daemon logic
// ============================================================================
fn get_appropriate_governor(is_charging: bool, cpu_usage: f32, load: f32) -> &'static str {
    let state = AutoCpuFreqState::new();
    let override_val = get_override(&state);
    
    match override_val {
        GovernorOverride::Performance => return "performance",
        GovernorOverride::Powersave => return "powersave",
        GovernorOverride::Default => {},
    }
    
    if CONFIG.has_option("charger", "governor") && is_charging {
        let gov = CONFIG.get("charger", "governor", "");
        if !gov.is_empty() && AVAILABLE_GOVERNORS_SORTED.iter().any(|g| g == &gov) {
            if let Some(g) = AVAILABLE_GOVERNORS_SORTED.iter().find(|&x| x == &gov) {
                return g.as_str();
            }
        }
    }
    
    if CONFIG.has_option("battery", "governor") && !is_charging {
        let gov = CONFIG.get("battery", "governor", "");
        if !gov.is_empty() && AVAILABLE_GOVERNORS_SORTED.iter().any(|g| g == &gov) {
            if let Some(g) = AVAILABLE_GOVERNORS_SORTED.iter().find(|&x| x == &gov) {
                return g.as_str();
            }
        }
    }
    
    if is_charging {
        if cpu_usage > 50.0 || load > state.performance_load_threshold {
            if AVAILABLE_GOVERNORS_SORTED.contains(&"performance".to_string()) {
                return "performance";
            }
        }
        if AVAILABLE_GOVERNORS_SORTED.contains(&"schedutil".to_string()) {
            return "schedutil";
        } else if AVAILABLE_GOVERNORS_SORTED.contains(&"ondemand".to_string()) {
            return "ondemand";
        }
    } else {
        if cpu_usage < 25.0 && load < state.powersave_load_threshold {
            if AVAILABLE_GOVERNORS_SORTED.contains(&"powersave".to_string()) {
                return "powersave";
            }
        }
        if AVAILABLE_GOVERNORS_SORTED.contains(&"schedutil".to_string()) {
            return "schedutil";
        }
    }
    
    AVAILABLE_GOVERNORS_SORTED.first()
        .map(|s| s.as_str())
        .unwrap_or("schedutil")
}

fn set_governor(governor: &str) -> Result<()> {
    println!("Setting governor: {}", governor);
    
    let status = Command::new("cpufreqctl.auto-cpufreq")
        .arg("--governor")
        .arg("--set")
        .arg(governor)
        .status()
        .context("Failed to set governor")?;
    
    if !status.success() {
        bail!("Governor change failed");
    }
    
    Ok(())
}

fn set_turbo_based_on_usage(cpu_usage: f32, is_charging: bool) -> Result<()> {
    let state = AutoCpuFreqState::new();
    let turbo_override = get_turbo_override(&state);
    
    match turbo_override {
        TurboOverride::Always => {
            set_turbo(true);
            return Ok(());
        }
        TurboOverride::Never => {
            set_turbo(false);
            return Ok(());
        }
        TurboOverride::Auto => {},
    }
    
    if CONFIG.has_option("charger", "turbo") && is_charging {
        let turbo_conf = CONFIG.get("charger", "turbo", "auto");
        match turbo_conf.as_str() {
            "always" => { set_turbo(true); return Ok(()); }
            "never" => { set_turbo(false); return Ok(()); }
            _ => {}
        }
    }
    
    if CONFIG.has_option("battery", "turbo") && !is_charging {
        let turbo_conf = CONFIG.get("battery", "turbo", "auto");
        match turbo_conf.as_str() {
            "always" => { set_turbo(true); return Ok(()); }
            "never" => { set_turbo(false); return Ok(()); }
            _ => {}
        }
    }
    
    // OPTIMIZED: Use cached system and temps
    let mut cached_sys = CACHED_SYSTEM.lock().unwrap();
    let sys = cached_sys.get_refreshed_system();
    
    let temp_cache = TEMP_CACHE.lock().unwrap();
    let cores = (0..sys.cpus().len())
        .map(|i| temp_cache.read_core_temp(i))
        .filter(|&t| t > 0.0)
        .collect::<Vec<_>>();
    
    let avg_temp = if !cores.is_empty() {
        cores.iter().sum::<f32>() / cores.len() as f32
    } else {
        0.0
    };
    
    if is_charging {
        if cpu_usage > 25.0 && avg_temp < 75.0 {
            set_turbo(true);
        } else if avg_temp >= 75.0 {
            set_turbo(false);
        }
    } else {
        if cpu_usage > 75.0 {
            set_turbo(true);
        } else {
            set_turbo(false);
        }
    }
    
    Ok(())
}

pub fn set_autofreq() -> Result<()> {
    let is_charging = charging()?;
    
    // OPTIMIZED: Use cached system
    let mut cached_sys = CACHED_SYSTEM.lock().unwrap();
    let sys = cached_sys.get_refreshed_system();
    
    let cpu_usage: f32 = sys.cpus().iter()
        .map(|c| c.cpu_usage())
        .sum::<f32>() / sys.cpus().len() as f32;
    
    let load = System::load_average().one as f32;
    
    let target_governor = get_appropriate_governor(is_charging, cpu_usage, load);
    let current_governor = get_current_gov().unwrap_or_else(|_| "unknown".to_string());
    
    if target_governor != current_governor {
        set_governor(target_governor)?;
    }
    
    set_turbo_based_on_usage(cpu_usage, is_charging)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_governor_override() {
        assert_eq!(GovernorOverride::from_str("powersave"), GovernorOverride::Powersave);
        assert_eq!(GovernorOverride::from_str("performance"), GovernorOverride::Performance);
        assert_eq!(GovernorOverride::from_str("invalid"), GovernorOverride::Default);
    }
    
    #[test]
    fn test_turbo_override() {
        assert_eq!(TurboOverride::from_str("never"), TurboOverride::Never);
        assert_eq!(TurboOverride::from_str("always"), TurboOverride::Always);
        assert_eq!(TurboOverride::from_str("auto"), TurboOverride::Auto);
    }

    #[test]
    fn test_temp_cache() {
        let cache = TempSensorCache::new();
        let temp = cache.read_core_temp(0);
        assert!(temp >= 0.0);
    }
}
