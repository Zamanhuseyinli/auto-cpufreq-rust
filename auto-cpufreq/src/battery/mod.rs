// src/battery/mod.rs
use std::fs;
use std::path::Path;
use std::process::Command;
use anyhow::Result;

pub mod asus;
pub mod ideapad_acpi;
pub mod ideapad_laptop;
pub mod thinkpad;

use crate::config::Config;

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply/";

/// Detect which laptop module is loaded
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaptopModule {
    IdeapadAcpi,
    IdeapadLaptop,
    ThinkpadAcpi,
    AsusWmi,
    None,
}

impl LaptopModule {
    pub fn detect() -> Self {
        if is_module_loaded("ideapad_acpi") {
            Self::IdeapadAcpi
        } else if is_module_loaded("ideapad_laptop") {
            Self::IdeapadLaptop
        } else if is_module_loaded("thinkpad_acpi") {
            Self::ThinkpadAcpi
        } else if is_module_loaded("asus_wmi") {
            Self::AsusWmi
        } else {
            Self::None
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::IdeapadAcpi => "ideapad_acpi",
            Self::IdeapadLaptop => "ideapad_laptop",
            Self::ThinkpadAcpi => "thinkpad_acpi",
            Self::AsusWmi => "asus_wmi",
            Self::None => "none",
        }
    }
}

fn is_module_loaded(module: &str) -> bool {
    Command::new("lsmod")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|stdout| stdout.contains(module))
        .unwrap_or(false)
}

/// Get list of batteries in the system
pub fn get_batteries() -> Result<Vec<String>> {
    let power_dir = Path::new(POWER_SUPPLY_DIR);
    
    if !power_dir.exists() {
        return Ok(Vec::new());
    }

    let mut batteries = Vec::new();
    
    for entry in fs::read_dir(power_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        if name_str.starts_with("BAT") {
            batteries.push(name_str.to_string());
        }
    }
    
    batteries.sort();
    Ok(batteries)
}

/// Common trait for battery threshold management
pub trait BatteryManager {
    fn setup(&self, config: &Config) -> Result<()>;
    fn print_thresholds(&self) -> Result<()>;
}

/// Main battery setup function - delegates to appropriate module
pub fn battery_setup(config: &Config) -> Result<()> {
    let module = LaptopModule::detect();
    
    match module {
        LaptopModule::IdeapadAcpi => {
            ideapad_acpi::IdeapadAcpiManager.setup(config)
        }
        LaptopModule::IdeapadLaptop => {
            ideapad_laptop::IdeapadLaptopManager.setup(config)
        }
        LaptopModule::ThinkpadAcpi => {
            thinkpad::ThinkpadManager.setup(config)
        }
        LaptopModule::AsusWmi => {
            asus::AsusManager.setup(config)
        }
        LaptopModule::None => {
            Ok(()) // No battery management needed
        }
    }
}

/// Print battery thresholds
pub fn battery_get_thresholds() -> Result<()> {
    let module = LaptopModule::detect();
    
    match module {
        LaptopModule::IdeapadAcpi => {
            ideapad_acpi::IdeapadAcpiManager.print_thresholds()
        }
        LaptopModule::IdeapadLaptop => {
            ideapad_laptop::IdeapadLaptopManager.print_thresholds()
        }
        LaptopModule::ThinkpadAcpi => {
            thinkpad::ThinkpadManager.print_thresholds()
        }
        LaptopModule::AsusWmi => {
            asus::AsusManager.print_thresholds()
        }
        LaptopModule::None => {
            Ok(()) // Nothing to print
        }
    }
}
