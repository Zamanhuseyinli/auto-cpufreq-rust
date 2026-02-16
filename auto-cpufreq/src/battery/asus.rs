use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;

use super::{BatteryManager, get_batteries};
use crate::config::Config;

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply/";

pub struct AsusManager;

impl BatteryManager for AsusManager {
    fn setup(&self, config: &Config) -> Result<()> {
        // Check if thresholds are enabled
        if !config.get_bool("battery", "enable_thresholds").unwrap_or(false) {
            return Ok(());
        }

        if !Path::new(POWER_SUPPLY_DIR).exists() {
            println!("WARNING {} does NOT exist", POWER_SUPPLY_DIR);
            return Ok(());
        }

        let batteries = get_batteries()?;
        
        for bat in batteries {
            let start_threshold = get_threshold_value(config, "start");
            let stop_threshold = get_threshold_value(config, "stop");
            
            set_battery(start_threshold, ThresholdMode::Start.as_str(), &bat)?;
            set_battery(stop_threshold, ThresholdMode::Stop.as_str(), &bat)?;
        }
        
        Ok(())
    }

    fn print_thresholds(&self) -> Result<()> {
        let batteries = get_batteries()?;
        
        println!("\n{}\n", "-".repeat(32) + " Battery Info " + &"-".repeat(33));
        println!("battery count = {}", batteries.len());
        
        for bat in &batteries {
            print_battery_threshold(bat, ThresholdMode::Start);
            print_battery_threshold(bat, ThresholdMode::Stop);
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum ThresholdMode {
    Start,
    Stop,
}

impl ThresholdMode {
    fn as_str(&self) -> &str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
        }
    }

    fn fallback_str(&self) -> &str {
        match self {
            Self::Start => "start",
            Self::Stop => "end",
        }
    }

    fn primary_path(&self, battery: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}{}/charge_{}_threshold",
            POWER_SUPPLY_DIR, battery, self.as_str()
        ))
    }

    fn fallback_path(&self, battery: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}{}/charge_control_{}_threshold",
            POWER_SUPPLY_DIR, battery, self.fallback_str()
        ))
    }
}

fn get_threshold_value(config: &Config, mode: &str) -> u8 {
    config.get_threshold(mode).unwrap_or_else(|_| {
        if mode == "start" { 0 } else { 100 }
    })
}

fn set_battery(value: u8, mode: &str, battery: &str) -> Result<()> {
    let file_path = PathBuf::from(format!(
        "{}{}/charge_{}_threshold",
        POWER_SUPPLY_DIR, battery, mode
    ));

    if !file_path.exists() {
        println!("WARNING: {} does NOT exist", file_path.display());
        return Ok(());
    }

    match std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("echo {} | tee {}", value, file_path.display()))
        .output()
    {
        Ok(output) => {
            if !output.status.success() {
                println!("WARNING: Failed to set {} threshold for {}", mode, battery);
                println!("  stderr: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
        Err(e) => {
            println!("WARNING: Command failed for {} threshold: {}", mode, e);
        }
    }

    Ok(())
}

fn print_battery_threshold(battery: &str, mode: ThresholdMode) {
    let primary = mode.primary_path(battery);
    let fallback = mode.fallback_path(battery);
    
    if primary.exists() {
        match fs::read_to_string(&primary) {
            Ok(val) => println!("{} {} threshold = {}", battery, mode.as_str(), val.trim()),
            Err(e) => println!("ERROR: failed to read battery {} thresholds: {}", battery, e),
        }
    } else if fallback.exists() {
        match fs::read_to_string(&fallback) {
            Ok(val) => println!("{} {} threshold = {}", battery, mode.as_str(), val.trim()),
            Err(e) => println!("ERROR: failed to read battery {} thresholds: {}", battery, e),
        }
    } else {
        println!("{} {} threshold: file not found", battery, mode.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_mode_strings() {
        assert_eq!(ThresholdMode::Start.as_str(), "start");
        assert_eq!(ThresholdMode::Stop.as_str(), "stop");
        assert_eq!(ThresholdMode::Stop.fallback_str(), "end");
    }
}
