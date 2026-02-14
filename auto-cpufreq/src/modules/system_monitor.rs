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
    pub left: Vec<String>,
    pub right: Vec<String>,
    sys: System,  // System nesnesini tutuyoruz
}

impl SystemMonitor {
    pub fn new(view: ViewType, suggestion: bool) -> Self {
        let mut sys = System::new_all();
        sys.refresh_cpu();  // İlk refresh
        
        Self { 
            view, 
            suggestion, 
            left: Vec::new(), 
            right: Vec::new(),
            sys,
        }
    }

    pub fn update(&mut self) {
        let sys_info = SystemInfo::new();
        let report = sys_info.generate_system_report(&mut self.sys);
        self.format_system_info(&report);
    }

    pub fn format_system_info(&mut self, report: &SystemReport) {
        self.left.clear();
        self.right.clear();

        self.left.push(format!("{}", "System Information"));
        self.left.push("".into());
        self.left.push(format!("Linux distro: {} {}", report.distro_name, report.distro_ver));
        self.left.push(format!("Linux kernel: {}", report.kernel_version));
        self.left.push(format!("Processor: {}", report.processor_model));
        self.left.push(format!("Cores: {:?}", report.total_core));
        self.left.push(format!("Architecture: {}", report.arch));
        self.left.push(format!("Driver: {:?}", report.cpu_driver));
        self.left.push(String::new());

        if crate::CONFIG.has_config() {
            self.left.push(format!("Using settings defined in {} file", crate::CONFIG.get_path().display()));
            self.left.push(String::new());
        }

        self.left.push("Current CPU Stats".into());
        self.left.push(String::new());
        self.left.push(format!("CPU max frequency: {:?} MHz", report.cpu_max_freq));
        self.left.push(format!("CPU min frequency: {:?} MHz", report.cpu_min_freq));
        self.left.push(String::new());
        self.left.push(format!("Core    Usage   Temperature     Frequency"));

        for core in &report.cores_info {
            self.left.push(format!("CPU{:<2}    {:>4.1}%    {:>6.0} °C    {:>6.0} MHz", core.id, core.usage, core.temperature, core.frequency));
        }

        if let Some(fan) = report.cpu_fan_speed {
            self.left.push(String::new());
            self.left.push(format!("CPU fan speed: {} RPM", fan));
        }

        // Right column
        self.right.push("Battery Stats".into());
        self.right.push(String::new());
        self.right.push(format!("Battery status: {:?}", report.battery_info));
        self.right.push(format!("Battery percentage: {}", report.battery_info.battery_level.map(|b| format!("{}%", b)).unwrap_or_else(|| "Unknown".into())));
        self.right.push(format!("AC plugged: {}", report.battery_info.is_ac_plugged.map(|b| if b { "Yes" } else { "No" }).unwrap_or("Unknown")));
        self.right.push(format!("Charging start threshold: {}", report.battery_info.charging_start_threshold.map(|v| v.to_string()).unwrap_or_else(|| "Unknown".into())));
        self.right.push(format!("Charging stop threshold: {}", report.battery_info.charging_stop_threshold.map(|v| v.to_string()).unwrap_or_else(|| "Unknown".into())));
        self.right.push(String::new());

        self.right.push("CPU Frequency Scaling".into());
        self.right.push(String::new());
        self.right.push(format!("Setting to use: \"{}\" governor", report.current_gov.clone().unwrap_or_else(|| "Unknown".into())));

        if self.suggestion {
            if let Some(sugg) = crate::modules::system_info::SystemInfo::governor_suggestion() {
                if report.current_gov.as_deref() != Some(&sugg) {
                    self.right.push(format!("Suggesting use of: \"{}\" governor", sugg));
                }
            }
        }

        if let Some(epp) = &report.current_epp {
            self.right.push(format!("EPP setting: {}", epp));
        } else {
            self.right.push("Not setting EPP (not supported by system)".into());
        }

        if let Some(epb) = &report.current_epb {
            self.right.push(format!("Setting to use: \"{}\" EPB", epb));
        }

        self.right.push(String::new());

        self.right.push("System Statistics".into());
        self.right.push(String::new());
        self.right.push(format!("Total CPU usage: {:.1} %", report.cpu_usage));
        self.right.push(format!("Total system load: {:.2}", report.load));

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter().map(|c| c.temperature).sum::<f32>() / report.cores_info.len() as f32;
            self.right.push(format!("Average temp. of all cores: {:.2} °C", avg_temp));
        }

        if let Some((a,b,c)) = report.avg_load {
            let load_status = if report.load < 1.0 { "Load optimal" } else { "Load high" };
            self.right.push(format!("{} (load average: {:.2}, {:.2}, {:.2})", load_status, a,b,c));
        }

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter().map(|c| c.temperature).sum::<f32>() / report.cores_info.len() as f32;
            let usage_status = if report.cpu_usage < 70.0 { "Optimal" } else { "High" };
            let temp_status = if avg_temp > 75.0 { "high" } else { "normal" };
            self.right.push(format!("{} total CPU usage: {:.1}%, {} average core temp: {:.1}°C", usage_status, report.cpu_usage, temp_status, avg_temp));
        }

        let turbo_status = match (report.is_turbo_on.0, report.is_turbo_on.1) {
            (Some(on), _) => if on { "On" } else { "Off" }.to_string(),
            (None, Some(auto)) => format!("Auto mode {}", if auto { "enabled" } else { "disabled" }),
            _ => "Unknown".into(),
        };
        self.right.push(format!("Setting turbo boost: {}", turbo_status));
   if self.suggestion {
      if let Some(on) = report.is_turbo_on.0 {
        let sugg = crate::modules::system_info::SystemInfo::turbo_on_suggestion(&mut self.sys);  // ✅ self.sys ekle
        if sugg != on {
            self.right.push(format!("Suggesting to set turbo boost: {}", if sugg { "on" } else { "off" }));
        }
    }
}
    }

    /// Simple blocking run that prints the formatted columns to stdout every 2s.
    pub fn run_blocking(&mut self) {
        loop {
            self.update();
            // print a simple two-column output
            println!("\x1B[2J\x1B[1;1H"); // clear screen
            let width = 80usize;
            let half = width / 2 - 1;
            let rows = std::cmp::max(self.left.len(), self.right.len());
            for i in 0..rows {
                let left = self.left.get(i).cloned().unwrap_or_default();
                let right = self.right.get(i).cloned().unwrap_or_default();
                println!("{:<half$} │ {}", left, right, half=half);
            }
            thread::sleep(Duration::from_secs(2));
        }
    }
}
