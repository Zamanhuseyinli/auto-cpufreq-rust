// src/battery/ideapad_laptop.rs

use std::path::PathBuf;
use anyhow::{Result, Context};

use super::{BatteryManager, get_batteries};
use crate::config::Config;

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply/";
const CONSERVATION_MODE_FILE: &str = 
    "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode";

pub struct IdeapadLaptopManager;

impl BatteryManager for IdeapadLaptopManager {
    fn setup(&self, config: &Config) -> Result<()> {
        if !config.get_bool("battery", "enable_thresholds").unwrap_or(false) {
            return Ok(());
        }

        let batteries = get_batteries()?;

        // Check conservation mode setting
        if let Ok(Some(mode)) = config.get_string("battery", "ideapad_laptop_conservation_mode") {
            match mode.as_str() {
                "true" => {
                    conservation_mode(1)?;
                    return Ok(());
                }
                "false" => {
                    conservation_mode(0)?;
                }
                _ => {}
            }
        }

        // Only set thresholds if conservation mode is off
        if !check_conservation_mode()? {
            for bat in batteries {
                let start_threshold = get_threshold_value(config, "start");
                let stop_threshold = get_threshold_value(config, "stop");
                
                set_battery(start_threshold, "start", &bat)?;
                set_battery(stop_threshold, "stop", &bat)?;
            }
        } else {
            println!("conservation mode is enabled unable to set thresholds");
        }
        
        Ok(())
    }

    fn print_thresholds(&self) -> Result<()> {
        if check_conservation_mode()? {
            println!("conservation mode is on");
            return Ok(());
        }

        let batteries = get_batteries()?;
        
        println!("\n{}\n", "-".repeat(32) + " Battery Info " + &"-".repeat(33));
        println!("battery count = {}", batteries.len());
        
        for bat in &batteries {
            match read_threshold(bat, "start") {
                Ok(val) => println!("{} start threshold = {}", bat, val),
                Err(e) => println!("ERROR: failed to read battery {} thresholds: {}", bat, e),
            }
            
            match read_threshold(bat, "stop") {
                Ok(val) => println!("{} stop threshold = {}", bat, val),
                Err(e) => println!("ERROR: failed to read battery {} thresholds: {}", bat, e),
            }
        }
        
        Ok(())
    }
}

fn get_threshold_value(config: &Config, mode: &str) -> u8 {
    config.get_threshold(mode).unwrap_or_else(|_| {
        if mode == "start" { 0 } else { 100 }
    })
}

fn set_battery(value: u8, mode: &str, battery: &str) -> Result<()> {
    let path = PathBuf::from(format!(
        "{}{}/charge_{}_threshold",
        POWER_SUPPLY_DIR, battery, mode
    ));
    
    if !path.exists() {
        println!("WARNING: {} does NOT exist", path.display());
        return Ok(());
    }
    
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("echo {} | tee {}", value, path.display()))
        .output()
        .with_context(|| format!("Failed to set battery threshold"))?;
    
    Ok(())
}

fn read_threshold(battery: &str, mode: &str) -> Result<String> {
    let path = PathBuf::from(format!(
        "{}{}/charge_{}_threshold",
        POWER_SUPPLY_DIR, battery, mode
    ));
    
    std::process::Command::new("cat")
        .arg(&path)
        .output()
        .with_context(|| format!("Failed to read threshold"))
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn conservation_mode(value: u8) -> Result<()> {
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("echo {} | tee {}", value, CONSERVATION_MODE_FILE))
        .output()
    {
        Ok(_) => {
            println!("conservation_mode is {}", value);
            Ok(())
        }
        Err(e) => {
            println!("unable to set conservation mode");
            Err(e.into())
        }
    }
}

fn check_conservation_mode() -> Result<bool> {
    match std::process::Command::new("cat")
        .arg(CONSERVATION_MODE_FILE)
        .output()
    {
        Ok(output) => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            match value.as_str() {
                "1" => Ok(true),
                "0" => Ok(false),
                _ => {
                    println!("could not get value from conservation mode");
                    Ok(false)
                }
            }
        }
        Err(_) => {
            println!("could not get the value from conservation mode");
            Ok(false)
        }
    }
}
