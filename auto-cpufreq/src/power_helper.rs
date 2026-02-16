// src/power_helper.rs

use anyhow::{Result, Context};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use crate::core::GITHUB;
use crate::tlp_stat_parser::TLPStatusParser;

// Check if a command exists
pub fn does_command_exist(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

lazy_static::lazy_static! {
    pub static ref BLUETOOTHCTL_EXISTS: bool = does_command_exist("bluetoothctl");
    pub static ref POWERPROFILESCTL_EXISTS: bool = does_command_exist("powerprofilesctl");
    pub static ref SYSTEMCTL_EXISTS: bool = does_command_exist("systemctl");
    pub static ref TLP_STAT_EXISTS: bool = does_command_exist("tlp-stat");
    pub static ref TUNED_STAT_EXISTS: bool = does_command_exist("tuned");
}

pub fn header() {
    println!("\n------------------------- auto-cpufreq: Power helper -------------------------\n");
}

pub fn warning() {
    println!("\n----------------------------------- Warning -----------------------------------\n");
}

pub fn footer() {
    println!("\n{}\n", "-".repeat(79));
}

// Detect if GNOME Power Profile service is running
pub fn gnome_power_status() -> Result<bool> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(false);
    }

    let status = Command::new("systemctl")
        .args(&["is-active", "--quiet", "power-profiles-daemon"]) 
        .status()
        .context("Failed to check GNOME power profiles daemon status")?;

    Ok(status.success())
}

// Alert in case TLP service is running
pub fn tlp_service_detect() -> Result<()> {
    if !*TLP_STAT_EXISTS {
        return Ok(());
    }

    let output = Command::new("tlp-stat")
        .arg("-s")
        .output()
        .context("Failed to run tlp-stat")?;

    let status_output = String::from_utf8_lossy(&output.stdout);
    let tlp_status = TLPStatusParser::new(&status_output);

    if tlp_status.is_enabled() {
        warning();
        println!("Detected you are running a TLP service!");
        println!("This daemon might interfere with auto-cpufreq which can lead to unexpected results.");
        println!("We strongly encourage you to remove TLP unless you really know what you are doing.");
    }

    Ok(())
}

// Alert in case GNOME power profile service is running
pub fn gnome_power_detect() -> Result<()> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(());
    }

    if gnome_power_status()? {
        warning();
        println!("Detected running GNOME Power Profiles daemon service!");
        println!("\nThis daemon might interfere with auto-cpufreq and will be automatically");
        println!("disabled when auto-cpufreq daemon is installed and");
        println!("it will be re-enabled after auto-cpufreq is removed.");
        println!("Steps to perform this action using auto-cpufreq: power_helper script:");
        println!("git clone {}.git", GITHUB);
        println!("python3 -m auto_cpufreq.power_helper --gnome_power_disable");
        println!("\nReference: {}#configuring-auto-cpufreq", GITHUB);
    }

    Ok(())
}

// Automatically disable GNOME power profile service during install
pub fn gnome_power_detect_install() -> Result<()> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(());
    }

    if gnome_power_status()? {
        warning();
        println!("Detected running GNOME Power Profiles daemon service!");
        println!("\nThis daemon might interfere with auto-cpufreq and has been disabled.\n");
        println!("This daemon is not automatically disabled in \"monitor\" mode and");
        println!("will be enabled after auto-cpufreq daemon is removed.");
    }

    Ok(())
}


// Stop GNOME >= 40 power profiles (live)
pub fn gnome_power_stop_live() -> Result<()> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(());
    }

    if gnome_power_status()? && *POWERPROFILESCTL_EXISTS {
        Command::new("powerprofilesctl")
            .args(&["set", "balanced"]) 
            .status()?;
        
        Command::new("systemctl")
            .args(&["stop", "power-profiles-daemon"]) 
            .status()?;
    }

    Ok(())
}

// Stop tuned (live)
pub fn tuned_stop_live() -> Result<()> {
    if *SYSTEMCTL_EXISTS && *TUNED_STAT_EXISTS {
        Command::new("systemctl")
            .args(&["stop", "tuned"]) 
            .status()?;
    }

    Ok(())
}

// Start GNOME >= 40 power profiles (live)
pub fn gnome_power_start_live() -> Result<()> {
    if *SYSTEMCTL_EXISTS {
        Command::new("systemctl")
            .args(&["start", "power-profiles-daemon"]) 
            .status()?;
    }

    Ok(())
}

// Start tuned (live)
pub fn tuned_start_live() -> Result<()> {
    if *SYSTEMCTL_EXISTS && *TUNED_STAT_EXISTS {
        Command::new("systemctl")
            .args(&["start", "tuned"]) 
            .status()?;
    }

    Ok(())
}

// Enable GNOME >= 40 power profiles (uninstall)
pub fn gnome_power_svc_enable() -> Result<()> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(());
    }

    println!("* Enabling GNOME power profiles\n");
    
    Command::new("systemctl")
        .args(&["unmask", "power-profiles-daemon"]) 
        .status()
        .context("Failed to unmask power-profiles-daemon")?;
    
    Command::new("systemctl")
        .args(&["enable", "--now", "power-profiles-daemon"]) 
        .status()
        .context("Failed to enable power-profiles-daemon")?;

    Ok(())
}

// Enable TuneD
pub fn tuned_svc_enable() -> Result<()> {
    if !*SYSTEMCTL_EXISTS || !*TUNED_STAT_EXISTS {
        return Ok(());
    }

    println!("* Enabling TuneD\n");
    
    Command::new("systemctl")
        .args(&["unmask", "tuned"]) 
        .status()
        .context("Failed to unmask tuned")?;
    
    Command::new("systemctl")
        .args(&["enable", "--now", "tuned"]) 
        .status()
        .context("Failed to enable tuned")?;

    Ok(())
}

// GNOME power profiles current status
pub fn gnome_power_svc_status() -> Result<()> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(());
    }

    println!("* GNOME power profiles status");
    Command::new("systemctl")
        .args(&["status", "power-profiles-daemon"]) 
        .status()
        .context("Failed to get GNOME power profiles status")?;

    Ok(())
}

// Set AutoEnable in /etc/bluetooth/main.conf
pub fn set_bluetooth_auto_enable(value: bool) -> Result<bool> {
    let btconf = Path::new("/etc/bluetooth/main.conf");
    let setting = format!("AutoEnable={}", if value { "true" } else { "false" });

    let content = fs::read_to_string(btconf)
        .context("Failed to read bluetooth config")?;

    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut in_policy_section = false;
    let mut found_and_set = false;

    for line in lines {
        let stripped = line.trim();

        if stripped.starts_with('[') {
            if in_policy_section && !found_and_set {
                new_lines.push(setting.clone());
                found_and_set = true;
            }
            in_policy_section = stripped.to_lowercase() == "[policy]";
            new_lines.push(line.to_string());
            continue;
        }

        if in_policy_section {
            if !stripped.starts_with('#') && stripped.starts_with("AutoEnable=") {
                new_lines.push(setting.clone());
                found_and_set = true;
                continue;
            }
            if stripped.starts_with('#') {
                let uncommented = stripped.trim_start_matches('#').trim();
                if uncommented.starts_with("AutoEnable=") {
                    new_lines.push(setting.clone());
                    found_and_set = true;
                    continue;
                }
            }
        }

        new_lines.push(line.to_string());
    }

    if in_policy_section && !found_and_set {
        new_lines.push(setting.clone());
        found_and_set = true;
    }

    if !found_and_set {
        new_lines.push(String::new());
        new_lines.push("[Policy]".to_string());
        new_lines.push(setting);
    }

    fs::write(btconf, new_lines.join("\n"))
        .context("Failed to write bluetooth config")?;

    Ok(true)
}

// Disable bluetooth on boot
pub fn bluetooth_disable() -> Result<()> {
    if !*BLUETOOTHCTL_EXISTS {
        println!("* Turn off bluetooth on boot [skipping] (package providing bluetooth access is not present)");
        return Ok(());
    }

    println!("* Turn off Bluetooth on boot (only)!");
    println!("  If you want bluetooth enabled on boot run: auto-cpufreq --bluetooth_boot_on");
    
    if !set_bluetooth_auto_enable(false)? {
        println!("\nERROR:\nWas unable to turn off bluetooth on boot");
    }

    Ok(())
}

// Enable bluetooth on boot
pub fn bluetooth_enable() -> Result<()> {
    if !*BLUETOOTHCTL_EXISTS {
        println!("* Turn on bluetooth on boot [skipping] (package providing bluetooth access is not present)");
        return Ok(());
    }

    println!("* Turn on bluetooth on boot");
    
    if !set_bluetooth_auto_enable(true)? {
        println!("\nERROR:\nWas unable to turn on bluetooth on boot");
    }

    Ok(())
}


// GNOME power removal reminder
pub fn gnome_power_rm_reminder() -> Result<()> {
    if !*SYSTEMCTL_EXISTS {
        return Ok(());
    }

    if !gnome_power_status()? {
        warning();
        println!("Detected GNOME Power Profiles daemon service is stopped!");
        println!("This service will now be enabled and started again.\n");
    }

    Ok(())
}

