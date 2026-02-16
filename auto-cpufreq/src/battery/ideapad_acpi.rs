use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

use super::{BatteryManager, get_batteries};
use crate::config::Config;

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply/";

pub struct IdeapadAcpiManager;

impl BatteryManager for IdeapadAcpiManager {
    fn setup(&self, config: &Config) -> Result<()> {
        if !config.get_bool("battery", "enable_thresholds").unwrap_or(false) {
            return Ok(());
        }

        if !Path::new(POWER_SUPPLY_DIR).exists() {
            println!("WARNING: could NOT access {}", POWER_SUPPLY_DIR);
            return Ok(());
        }

        let batteries = get_batteries()?;
        
        for bat in batteries {
            let start_threshold = get_threshold_value(config, "start");
            let stop_threshold = get_threshold_value(config, "stop");
            
            set_battery(start_threshold, "start", &bat)?;
            set_battery(stop_threshold, "stop", &bat)?;
        }
        
        Ok(())
    }

    fn print_thresholds(&self) -> Result<()> {
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

fn read_threshold(battery: &str, mode: &str) -> Result<String> {
    let file_path = PathBuf::from(format!(
        "{}{}/charge_{}_threshold",
        POWER_SUPPLY_DIR, battery, mode
    ));
    
    std::process::Command::new("cat")
        .arg(&file_path)
        .output()
        .with_context(|| format!("Failed to read threshold from {:?}", file_path))
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
