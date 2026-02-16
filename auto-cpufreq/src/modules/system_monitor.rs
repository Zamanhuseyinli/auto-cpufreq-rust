// src/modules/system_monitor.rs - OPTIMIZED VERSION
use std::thread;
use std::time::Duration;
use std::fmt::Write as FmtWrite;

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

// ============================================================================
// OPTIMIZATION: String buffer pooling
// ============================================================================
struct StringBuffer {
    buffer: String,
}

impl StringBuffer {
    fn new() -> Self {
        Self {
            buffer: String::with_capacity(4096),
        }
    }

    fn clear(&mut self) {
        self.buffer.clear();
    }

    fn write_str(&mut self, s: &str) {
        self.buffer.push_str(s);
    }

    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.buffer.write_fmt(args);
    }

    fn to_lines(&self) -> Vec<String> {
        self.buffer.lines().map(String::from).collect()
    }
}

pub struct SystemMonitor {
    pub view: ViewType,
    pub suggestion: bool,
    pub verbose: bool,
    pub left: Vec<String>,
    pub right: Vec<String>,
    sys: System,
    // OPTIMIZED: Reusable string buffers
    left_buffer: StringBuffer,
    right_buffer: StringBuffer,
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
            left_buffer: StringBuffer::new(),
            right_buffer: StringBuffer::new(),
        }
    }

    pub fn update(&mut self) {
        // OPTIMIZED: Single refresh sequence
        self.sys.refresh_cpu();
        std::thread::sleep(Duration::from_millis(200));
        self.sys.refresh_cpu();
        
        let sys_info = SystemInfo::new();
        let report = sys_info.generate_system_report(&self.sys);
        self.format_system_info(&report);
    }

    // OPTIMIZED: Helper to format options efficiently
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

    // OPTIMIZED: Format using string buffers instead of Vec allocations
    pub fn format_system_info(&mut self, report: &SystemReport) {
        // Clear buffers
        self.left_buffer.clear();
        self.right_buffer.clear();

        // ========== LEFT COLUMN ==========
        self.format_left_column(report);
        
        // ========== RIGHT COLUMN ==========
        self.format_right_column(report);

        // Convert buffers to line vectors
        self.left = self.left_buffer.to_lines();
        self.right = self.right_buffer.to_lines();
    }

    fn format_left_column(&mut self, report: &SystemReport) {
        let buf = &mut self.left_buffer;

        // System Information
        buf.write_str("System Information\n\n");
        buf.write_fmt(format_args!("Linux distro: {} {}\n", report.distro_name, report.distro_ver));
        buf.write_fmt(format_args!("Linux kernel: {}\n", report.kernel_version));
        buf.write_fmt(format_args!("Processor: {}\n", report.processor_model));
        
        if self.verbose {
            buf.write_fmt(format_args!("Cores: {:?}\n", report.total_core));
            buf.write_fmt(format_args!("Driver: {:?}\n", report.cpu_driver));
        } else {
            buf.write_fmt(format_args!("Cores: {}\n", Self::format_option(report.total_core, false)));
            buf.write_fmt(format_args!("Driver: {}\n", report.cpu_driver.as_deref().unwrap_or("Unknown")));
        }
        
        buf.write_fmt(format_args!("Architecture: {}\n\n", report.arch));

        if crate::CONFIG.has_config() {
            buf.write_fmt(format_args!("Using settings defined in {}\n\n", crate::CONFIG.get_path().display()));
        }

        // Current CPU Stats
        buf.write_str("Current CPU Stats\n\n");
        
        if self.verbose {
            buf.write_fmt(format_args!("CPU max frequency: {:?} MHz\n", report.cpu_max_freq));
            buf.write_fmt(format_args!("CPU min frequency: {:?} MHz\n\n", report.cpu_min_freq));
        } else {
            let max_freq = report.cpu_max_freq.map(|f| format!("{:.0}", f)).unwrap_or_else(|| "Unknown".to_string());
            let min_freq = report.cpu_min_freq.map(|f| format!("{:.0}", f)).unwrap_or_else(|| "Unknown".to_string());
            buf.write_fmt(format_args!("CPU max frequency: {} MHz\n", max_freq));
            buf.write_fmt(format_args!("CPU min frequency: {} MHz\n\n", min_freq));
        }
        
        // Core info header
        buf.write_fmt(format_args!("{:<5} {:<7} {:<11} {:<8}\n", "Core", "Usage", "Temp", "Freq"));

        // Core info rows
        for core in &report.cores_info {
            let temp_str = if core.temperature > 0.0 {
                format!("{:.0}°C", core.temperature)
            } else {
                "--°C".to_string()
            };
            
            buf.write_fmt(format_args!("{:<5} {:>6.1}% {:<11} {:>5.0} MHz\n", 
                format!("CPU{}", core.id),
                core.usage,
                temp_str,
                core.frequency
            ));
        }

        if let Some(fan) = report.cpu_fan_speed {
            buf.write_str("\n");
            buf.write_fmt(format_args!("CPU fan speed: {} RPM\n", fan));
        }
    }

    fn format_right_column(&mut self, report: &SystemReport) {
        let buf = &mut self.right_buffer;

        // Battery Stats
        buf.write_str("Battery Stats\n\n");
        
        if self.verbose {
            buf.write_fmt(format_args!("Battery info: {:?}\n\n", report.battery_info));
        } else {
            let battery_status = Self::format_battery_status(
                report.battery_info.is_charging, 
                report.battery_info.is_ac_plugged,
                false
            );
            buf.write_fmt(format_args!("Battery status: {}\n", battery_status));
            
            let battery_level = report.battery_info.battery_level
                .map(|b| format!("{}%", b))
                .unwrap_or_else(|| "Unknown".to_string());
            buf.write_fmt(format_args!("Battery level: {}\n", battery_level));

            let ac_status = report.battery_info.is_ac_plugged
                .map(|ac| if ac { "Yes" } else { "No" })
                .unwrap_or("Unknown");
            buf.write_fmt(format_args!("AC plugged: {}\n", ac_status));

            let start_threshold = report.battery_info.charging_start_threshold
                .map(|t| format!("{}%", t))
                .unwrap_or_else(|| "Not set".to_string());
            buf.write_fmt(format_args!("Start threshold: {}\n", start_threshold));

            let stop_threshold = report.battery_info.charging_stop_threshold
                .map(|t| format!("{}%", t))
                .unwrap_or_else(|| "Not set".to_string());
            buf.write_fmt(format_args!("Stop threshold: {}\n\n", stop_threshold));
        }

        // CPU Frequency Scaling
        buf.write_str("CPU Frequency Scaling\n\n");
        
        if self.verbose {
            buf.write_fmt(format_args!("Current governor: {:?}\n", report.current_gov));
            buf.write_fmt(format_args!("EPP: {:?}\n", report.current_epp));
            buf.write_fmt(format_args!("EPB: {:?}\n", report.current_epb));
        } else {
            let current_gov = report.current_gov.as_deref().unwrap_or("Unknown");
            buf.write_fmt(format_args!("Current governor: {}\n", current_gov));

            if let Some(epp) = &report.current_epp {
                buf.write_fmt(format_args!("EPP: {}\n", epp));
            } else {
                buf.write_str("EPP: Not supported\n");
            }

            if let Some(epb) = &report.current_epb {
                buf.write_fmt(format_args!("EPB: {}\n", epb));
            }
        }

        if self.suggestion {
            if let Some(sugg) = SystemInfo::governor_suggestion() {
                if report.current_gov.as_deref() != Some(&sugg) {
                    buf.write_fmt(format_args!("Suggested governor: {}\n", sugg));
                }
            }
        }

        buf.write_str("\n");

        // System Statistics
        buf.write_str("System Statistics\n\n");
        buf.write_fmt(format_args!("CPU usage: {:.1}%\n", report.cpu_usage));
        buf.write_fmt(format_args!("System load: {:.2}\n", report.load));

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
                buf.write_fmt(format_args!("Average temp: {:.1} °C\n", avg_temp));
            }
        }

        if let Some((a, b, c)) = report.avg_load {
            let load_status = if report.load < 1.0 { "optimal" } else { "high" };
            buf.write_fmt(format_args!("Load {}: {:.2}, {:.2}, {:.2}\n", load_status, a, b, c));
        }

        // Turbo status
        if self.verbose {
            buf.write_fmt(format_args!("Turbo boost: {:?}\n", report.is_turbo_on));
        } else {
            let turbo_status = match (report.is_turbo_on.0, report.is_turbo_on.1) {
                (Some(on), _) => if on { "On" } else { "Off" }.to_string(),
                (None, Some(auto)) => format!("Auto ({})", if auto { "enabled" } else { "disabled" }),
                _ => "Unknown".to_string(),
            };
            buf.write_fmt(format_args!("Turbo boost: {}\n", turbo_status));
        }

        if self.suggestion {
            if let Some(on) = report.is_turbo_on.0 {
                let sugg = SystemInfo::turbo_on_suggestion(&self.sys);
                if sugg != on {
                    buf.write_fmt(format_args!("Suggested turbo: {}\n", if sugg { "On" } else { "Off" }));
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

            // OPTIMIZATION: Daha geniş ekran - 100 karakter
            let width = 100usize;
            let half = width / 2 - 2;
            let rows = std::cmp::max(self.left.len(), self.right.len());

            for i in 0..rows {
                let left = self.left.get(i).map(String::as_str).unwrap_or("");
                let right = self.right.get(i).map(String::as_str).unwrap_or("");

                if left.len() > half {
                    let truncate_at = half.saturating_sub(3);
                    print!("{:<half$}... │ {}\n", &left[..truncate_at], right, half=half);
                } else {
                    println!("{:<half$} │ {}", left, right, half=half);
                }
            }

            thread::sleep(Duration::from_secs(2));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_buffer() {
        let mut buf = StringBuffer::new();
        buf.write_str("Hello\n");
        buf.write_fmt(format_args!("World {}\n", 123));
        let lines = buf.to_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Hello");
        assert_eq!(lines[1], "World 123");
    }

    #[test]
    fn test_monitor_update() {
        let mut monitor = SystemMonitor::new(ViewType::Monitor, false);
        monitor.update();
        assert!(!monitor.left.is_empty());
        assert!(!monitor.right.is_empty());
    }
}
