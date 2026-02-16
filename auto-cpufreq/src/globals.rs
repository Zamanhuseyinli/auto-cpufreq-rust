// src/globals.rs

use std::path::Path;
use std::process::Command;

pub const ALL_GOVERNORS: &[&str] = &[
    "performance",
    "ondemand",
    "conservative",
    "schedutil",
    "userspace",
    "powersave",
];

pub const CONSERVATION_MODE_FILE: &str = 
    "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode";

pub const GITHUB: &str = "https://github.com/AdnanHodzic/auto-cpufreq";

pub const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply/";

pub const CPU_TEMP_SENSOR_PRIORITY: &[&str] = &[
    "coretemp",
    "acpitz",
    "k10temp",
    "zenpower",
];

lazy_static::lazy_static! {
    pub static ref IS_INSTALLED_WITH_AUR: bool = check_aur_install();
    pub static ref AVAILABLE_GOVERNORS: Vec<String> = get_available_governors();
    pub static ref AVAILABLE_GOVERNORS_SORTED: Vec<String> = sort_governors(&AVAILABLE_GOVERNORS);
}

fn check_aur_install() -> bool {
    Path::new("/etc/arch-release").exists()
        && Command::new("pacman")
            .args(&["-Qs", "auto-cpufreq"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
}

fn get_available_governors() -> Vec<String> {
    Command::new("cat")
        .arg("/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| {
            s.trim()
                .split_whitespace()
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn sort_governors(available: &[String]) -> Vec<String> {
    ALL_GOVERNORS
        .iter()
        .filter_map(|&gov| {
            if available.contains(&gov.to_string()) {
                Some(gov.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_governors_order() {
        assert_eq!(ALL_GOVERNORS[0], "performance");
        assert_eq!(ALL_GOVERNORS[ALL_GOVERNORS.len() - 1], "powersave");
    }
}
