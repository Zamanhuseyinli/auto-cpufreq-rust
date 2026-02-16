// src/modules/system_monitor.rs
use std::thread;
use std::time::Duration;

use sysinfo::System;

use crate::modules::system_info::{SystemInfo, SystemReport};

#[derive(Debug, Clone, Copy)]
pub enum ViewType {
    Stats,
    Monitor,
    Live,
}

impl std::fmt::Display for ViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewType::Stats => write!(f, "Stats"),
            ViewType::Monitor => write!(f, "Monitor"),
            ViewType::Live => write!(f, "Live"),
        }
    }
}

pub struct SystemMonitor {
    pub view: ViewType,
    pub suggestion: bool,
    pub verbose: bool,
    pub left: Vec<String>,
    pub right: Vec<String>,
    sys: System,
}

impl SystemMonitor {
    pub fn new(view: ViewType, suggestion: bool) -> Self {
        Self::new_with_verbose(view, suggestion, false)
    }

    pub fn new_with_verbose(view: ViewType, suggestion: bool, verbose: bool) -> Self {
        let sys = System::new_all();
        
        Self { 
            view, 
            suggestion,
            verbose,
            left: Vec::new(), 
            right: Vec::new(),
            sys,
        }
    }

    pub fn update(&mut self) {
        // CRITICAL: Proper CPU refresh sequence
        self.sys.refresh_cpu();
        std::thread::sleep(Duration::from_millis(200));
        self.sys.refresh_cpu();
        
        let sys_info = SystemInfo::new();
        let report = sys_info.generate_system_report(&mut self.sys);
        self.format_system_info(&report);
    }

    fn format_option<T: std::fmt::Display + std::fmt::Debug>(opt: Option<T>, verbose: bool) -> String {
        if verbose {
            format!("{:?}", opt)
        } else {
            opt.map(|v| v.to_string()).unwrap_or_else(|| "Unknown".to_string())
        }
    }

    fn format_battery_status(is_charging: Option<bool>, is_ac_plugged: Option<bool>, verbose: bool) -> String {
        if verbose {
            format!("is_charging: {:?}, is_ac_plugged: {:?}", is_charging, is_ac_plugged)
        } else {
            match (is_charging, is_ac_plugged) {
                (Some(true), _) => "Charging".to_string(),
                (Some(false), Some(false)) => "Discharging".to_string(),
                (Some(false), Some(true)) => "Charged".to_string(),
                _ => "Unknown".to_string(),
            }
        }
    }

    pub fn format_system_info(&mut self, report: &SystemReport) {
        self.left.clear();
        self.right.clear();

        // Left column - System Information
        self.left.push("System Information".to_string());
        self.left.push(String::new());
        self.left.push(format!("Linux distro: {} {}", report.distro_name, report.distro_ver));
        self.left.push(format!("Linux kernel: {}", report.kernel_version));
        self.left.push(format!("Processor: {}", report.processor_model));
        
        if self.verbose {
            self.left.push(format!("Cores: {:?}", report.total_core));
            self.left.push(format!("Driver: {:?}", report.cpu_driver));
        } else {
            self.left.push(format!("Cores: {}", Self::format_option(report.total_core, false)));
            self.left.push(format!("Driver: {}", report.cpu_driver.as_deref().unwrap_or("Unknown")));
        }
        
        self.left.push(format!("Architecture: {}", report.arch));
        self.left.push(String::new());

        if crate::CONFIG.has_config() {
            self.left.push(format!("Using settings defined in {}", crate::CONFIG.get_path().display()));
            self.left.push(String::new());
        }

        // Current CPU Stats
        self.left.push("Current CPU Stats".to_string());
        self.left.push(String::new());
        
        if self.verbose {
            self.left.push(format!("CPU max frequency: {:?} MHz", report.cpu_max_freq));
            self.left.push(format!("CPU min frequency: {:?} MHz", report.cpu_min_freq));
        } else {
            self.left.push(format!("CPU max frequency: {} MHz", 
                report.cpu_max_freq.map(|f| format!("{:.0}", f)).unwrap_or_else(|| "Unknown".to_string())));
            self.left.push(format!("CPU min frequency: {} MHz",
                report.cpu_min_freq.map(|f| format!("{:.0}", f)).unwrap_or_else(|| "Unknown".to_string())));
        }
        
        self.left.push(String::new());
        
        // FIXED: Compact but readable columns that fit in 40 chars total
        self.left.push(format!("{:<5} {:<7} {:<11} {:<8}", "Core", "Usage", "Temp", "Freq"));

        for core in &report.cores_info {
            let temp_str = if core.temperature > 0.0 {
                format!("{:.0}°C", core.temperature)  // Compact: no space before unit
            } else {
                "--°C".to_string()
            };
            
            // FIXED: Compact format that fits in ~40 chars with full "Frequency" visible
            self.left.push(format!("{:<5} {:>6.1}% {:<11} {:>5.0} MHz", 
                format!("CPU{}", core.id),
                core.usage,
                temp_str,
                core.frequency
            ));
        }

        if let Some(fan) = report.cpu_fan_speed {
            self.left.push(String::new());
            self.left.push(format!("CPU fan speed: {} RPM", fan));
        }

        // Right column - Battery Stats
        self.right.push("Battery Stats".to_string());
        self.right.push(String::new());
        
        if self.verbose {
            self.right.push(format!("Battery info: {:?}", report.battery_info));
        } else {
            let battery_status = Self::format_battery_status(
                report.battery_info.is_charging, 
                report.battery_info.is_ac_plugged,
                false
            );
            self.right.push(format!("Battery status: {}", battery_status));
            
            let battery_level = report.battery_info.battery_level
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "Unknown".to_string());
            self.right.push(format!("Battery level: {}", battery_level));

            let ac_status = report.battery_info.is_ac_plugged
                .map(|ac| if ac { "Yes" } else { "No" })
                .unwrap_or("Unknown");
            self.right.push(format!("AC plugged: {}", ac_status));

            let start_threshold = report.battery_info.charging_start_threshold
                .map(|t| format!("{}%", t))
                .unwrap_or_else(|| "Not set".to_string());
            self.right.push(format!("Start threshold: {}", start_threshold));

            let stop_threshold = report.battery_info.charging_stop_threshold
                .map(|t| format!("{}%", t))
                .unwrap_or_else(|| "Not set".to_string());
            self.right.push(format!("Stop threshold: {}", stop_threshold));
        }
        
        self.right.push(String::new());

        // CPU Frequency Scaling
        self.right.push("CPU Frequency Scaling".to_string());
        self.right.push(String::new());
        
        if self.verbose {
            self.right.push(format!("Current governor: {:?}", report.current_gov));
            self.right.push(format!("EPP: {:?}", report.current_epp));
            self.right.push(format!("EPB: {:?}", report.current_epb));
        } else {
            let current_gov = report.current_gov.as_deref().unwrap_or("Unknown");
            self.right.push(format!("Current governor: {}", current_gov));

            if let Some(epp) = &report.current_epp {
                self.right.push(format!("EPP: {}", epp));
            } else {
                self.right.push("EPP: Not supported".to_string());
            }

            if let Some(epb) = &report.current_epb {
                self.right.push(format!("EPB: {}", epb));
            }
        }

        if self.suggestion {
            if let Some(sugg) = SystemInfo::governor_suggestion() {
                if report.current_gov.as_deref() != Some(&sugg) {
                    self.right.push(format!("Suggested governor: {}", sugg));
                }
            }
        }

        self.right.push(String::new());

        // System Statistics
        self.right.push("System Statistics".to_string());
        self.right.push(String::new());
        self.right.push(format!("CPU usage: {:.1}%", report.cpu_usage));
        self.right.push(format!("System load: {:.2}", report.load));

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter()
                .map(|c| c.temperature)
                .filter(|&t| t > 0.0)
                .sum::<f32>();
            let temp_count = report.cores_info.iter()
                .filter(|c| c.temperature > 0.0)
                .count();
            
            if temp_count > 0 {
                let avg_temp = avg_temp / temp_count as f32;
                self.right.push(format!("Average temp: {:.1} °C", avg_temp));
            }
        }

        if let Some((a, b, c)) = report.avg_load {
            let load_status = if report.load < 1.0 { "optimal" } else { "high" };
            self.right.push(format!("Load {}: {:.2}, {:.2}, {:.2}", load_status, a, b, c));
        }

        // Turbo status
        if self.verbose {
            self.right.push(format!("Turbo boost: {:?}", report.is_turbo_on));
        } else {
            let turbo_status = match (report.is_turbo_on.0, report.is_turbo_on.1) {
                (Some(on), _) => if on { "On" } else { "Off" }.to_string(),
                (None, Some(auto)) => format!("Auto ({})", if auto { "enabled" } else { "disabled" }),
                _ => "Unknown".to_string(),
            };
            self.right.push(format!("Turbo boost: {}", turbo_status));
        }

        if self.suggestion {
            if let Some(on) = report.is_turbo_on.0 {
                let sugg = SystemInfo::turbo_on_suggestion(&mut self.sys);
                if sugg != on {
                    self.right.push(format!("Suggested turbo: {}", if sugg { "On" } else { "Off" }));
                }
            }
        }
    }

    /// Simple blocking run that prints the formatted columns to stdout every 2s.
    pub fn run_blocking(&mut self) {
        loop {
            self.update();
            
            // Clear screen
            print!("\x1B[2J\x1B[1;1H");
            
            let width = 80usize;
            let half = width / 2 - 1;
            let rows = std::cmp::max(self.left.len(), self.right.len());
            
            for i in 0..rows {
                let left = self.left.get(i).cloned().unwrap_or_default();
                let right = self.right.get(i).cloned().unwrap_or_default();
                
                // Truncate if too long
                let left_truncated = if left.len() > half {
                    format!("{}...", &left[..half-3])
                } else {
                    left
                };
                
                println!("{:<half$} │ {}", left_truncated, right, half=half);
            }
            
            thread::sleep(Duration::from_secs(2));
        }
    }
}
