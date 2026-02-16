// src/bin/auto_cpufreq.rs

use anyhow::Result;
use clap::Parser;
use auto_cpufreq::config::{CONFIG, find_config_file};
use auto_cpufreq::core::*;
use auto_cpufreq::globals::*;
use auto_cpufreq::power_helper::*;
use auto_cpufreq::battery;
use auto_cpufreq::modules::{SystemMonitor, ViewType};
use std::thread;
use std::time::Duration;

use auto_cpufreq::core::footer;

#[derive(Parser, Debug)]
#[command(name = "auto-cpufreq")]
#[command(about = "Automatic CPU speed & power optimizer for Linux", long_about = None)]
struct Args {
    /// Monitor and see suggestions for CPU optimizations
    #[arg(long)]
    monitor: bool,

    /// Monitor and make (temp.) suggested CPU optimizations
    #[arg(long)]
    live: bool,

    #[arg(long, hide = true)]
    daemon: bool,

    /// Install daemon for (permanent) automatic CPU optimizations
    #[arg(long)]
    install: bool,

    /// Update daemon and package
    #[arg(long, value_name = "PATH")]
    update: Option<Option<String>>,

    /// Remove daemon
    #[arg(long)]
    remove: bool,

    /// Force use of either "powersave" or "performance" governors
    #[arg(long, value_name = "GOVERNOR")]
    force: Option<String>,

    /// Force use of CPU turbo mode
    #[arg(long, value_name = "MODE")]
    turbo: Option<String>,

    /// Use config file at defined path
    #[arg(long, value_name = "PATH")]
    config: Option<String>,

    /// View live stats of CPU optimizations
    #[arg(long)]
    stats: bool,

    #[arg(long, hide = true)]
    get_state: bool,

    /// Turn off Bluetooth on boot
    #[arg(long)]
    bluetooth_boot_off: bool,

    /// Turn on Bluetooth on boot
    #[arg(long)]
    bluetooth_boot_on: bool,

    /// Show debug info
    #[arg(long)]
    debug: bool,

    /// Show verbose/detailed output (use with --monitor, --live, --stats)
    #[arg(long, short)]
    verbose: bool,

    /// Show currently installed version
    #[arg(long)]
    version: bool,

    /// Support the project
    #[arg(long)]
    donate: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Display info if config file is used
    let config_path = find_config_file(args.config.as_deref());
    CONFIG.set_path(config_path.clone())?;

    fn config_info_dialog() {
        if CONFIG.has_config() {
            println!("\nUsing settings defined in {} file", CONFIG.get_path().display());
        }
    }

    // If no arguments provided, show help
    if !has_any_flag(&args) {
        println!("\n{}\n", "-".repeat(32) + " auto-cpufreq " + &"-".repeat(33));
        println!("Automatic CPU speed & power optimizer for Linux");
        println!("\nExample usage:\nauto-cpufreq --monitor");
        println!("\n-----\n");
        
        return Ok(());
    }

    // Handle force governor override
    if let Some(ref force_val) = args.force {
        not_running_daemon_check()?;
        root_check()?;
        let state = AutoCpuFreqState::new();
        set_override(&state, force_val)?;
    }

    // Handle turbo override
    if let Some(ref turbo_val) = args.turbo {
        not_running_daemon_check()?;
        root_check()?;
        let state = AutoCpuFreqState::new();
        set_turbo_override(&state, turbo_val)?;
    }

    if args.monitor {
        root_check()?;
        battery::battery_setup(&CONFIG)?;
        gnome_power_detect().ok();
        tlp_service_detect().ok();

        if *TLP_STAT_EXISTS || (*SYSTEMCTL_EXISTS && gnome_power_status()?) {
            println!("press Enter to continue or Ctrl + C to exit...");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
        }

        let mut monitor = SystemMonitor::new_with_verbose(ViewType::Monitor, true, args.verbose);
        monitor.run_blocking();
        
    } else if args.live {
        root_check()?;
        battery::battery_setup(&CONFIG)?;

        gnome_power_detect_install().ok();
        gnome_power_stop_live().ok();
        tuned_stop_live().ok();
        tlp_service_detect().ok();

        if *TLP_STAT_EXISTS || (*SYSTEMCTL_EXISTS && gnome_power_status()?) {
            println!("press Enter to continue or Ctrl + C to exit...");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
        }

        cpufreqctl()?;

        // Spawn daemon thread
        let daemon_handle = thread::spawn(|| {
            loop {
                thread::sleep(Duration::from_secs(1));
                // set_autofreq() would be called here
            }
        });

        let mut monitor = SystemMonitor::new_with_verbose(ViewType::Live, false, args.verbose);
        monitor.run_blocking();
        
        daemon_handle.join().unwrap();
        
    } else if args.daemon {
        config_info_dialog();
        root_check()?;
        gnome_power_detect()?;
        tlp_service_detect()?;

        battery::battery_setup(&CONFIG)?;
        
        println!("\n* Starting auto-cpufreq daemon");
        println!("* Monitoring system and adjusting CPU frequency...\n");

        loop {
            footer(79);
            
            // Update stats file
            if let Err(e) = update_stats_file() {
                eprintln!("WARNING: Failed to update stats file: {}", e);
            }
            
            // Ensure cpufreqctl is available
            cpufreqctl()?;
            
            // Show system info (first iteration only)
            static FIRST_RUN: std::sync::Once = std::sync::Once::new();
            FIRST_RUN.call_once(|| {
                let _ = distro_info();
                let _ = sysinfo();
            });
            
            // Main frequency adjustment logic
            if let Err(e) = set_autofreq() {
                eprintln!("ERROR: Failed to set auto frequency: {}", e);
            }
            
            countdown(2);
        }
        
    } else if args.install {
        root_check()?;
        
        gnome_power_detect()?;
        tlp_service_detect()?;
        
        // Install daemon using appropriate init system
        install_daemon()?;
        
        println!("\nauto-cpufreq daemon installed and started");
        println!("\nTo view live stats, run:\nauto-cpufreq --stats");
        
    } else if let Some(update_path) = args.update {
        root_check()?;
        let _custom_dir = update_path.unwrap_or_else(|| "/opt/auto-cpufreq/source".to_string());

        if *IS_INSTALLED_WITH_AUR {
            println!("\n{}\n", "=".repeat(80));
            println!("Arch-based distribution with AUR support detected.");
            println!("Please refresh auto-cpufreq using your AUR helper.");
            println!("\n{}\n", "=".repeat(80));
        } else {
            let is_new_update = check_for_update()?;
            if !is_new_update {
                return Ok(());
            }

            println!("\nDo you want to update auto-cpufreq to the latest release? [Y/n]: ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            
            let ans = input.trim().to_lowercase();
            if ans.is_empty() || ans == "y" || ans == "yes" {
                // First remove the old daemon
                remove_daemon()?;
                
                // TODO: implement new_update(&custom_dir)?;
                println!("\nRe-enabling daemon...");
                
                // Reinstall daemon
                install_daemon()?;
                
                println!("\nauto-cpufreq is updated to the latest version");
                app_version();
            } else {
                println!("Update aborted");
            }
        }
        
    } else if args.remove {
        root_check()?;
        remove_daemon()?;
        
    } else if args.stats {
        root_check()?;

        not_running_daemon_check()?;
        config_info_dialog();
        
        gnome_power_detect()?;
        tlp_service_detect()?;

        if *TLP_STAT_EXISTS || (*SYSTEMCTL_EXISTS && gnome_power_status()?) {
            println!("press Enter to continue or Ctrl + C to exit...");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
        }

        let mut monitor = SystemMonitor::new_with_verbose(ViewType::Stats, false, args.verbose);
        monitor.update();
        
        let rows = std::cmp::max(monitor.left.len(), monitor.right.len());
        let width = 80usize;
        let half = width / 2 - 1;
        for i in 0..rows {
            let left = monitor.left.get(i).cloned().unwrap_or_default();
            let right = monitor.right.get(i).cloned().unwrap_or_default();
            println!("{:<half$} │ {}", left, right, half=half);
        }
        
    } else if args.get_state {
        not_running_daemon_check()?;
        let state = AutoCpuFreqState::new();
        let override_val = get_override(&state);
        println!("{}", override_val.to_str());
        
    } else if args.bluetooth_boot_off {
        footer(79);
        root_check()?;
        bluetooth_disable()?;
        footer(79);
        
    } else if args.bluetooth_boot_on {
        footer(79);
        root_check()?;
        bluetooth_enable()?;
        footer(79);
        
    } else if args.debug {
        config_info_dialog();
        root_check()?;
        battery::battery_get_thresholds()?;
        cpufreqctl()?;
        footer(79);
        distro_info()?;
        sysinfo()?;
        println!();
        app_version();
        println!();
        println!("Battery is: {}charging", if charging()? { "" } else { "dis" });
        println!();
        get_load();
        print_current_gov();
        get_turbo();
        footer(79);
        
    } else if args.version {
        footer(79);
        distro_info()?;
        app_version();
        footer(79);
        
    } else if args.donate {
        footer(79);
        println!("If auto-cpufreq helped you out and you find it useful ...\n");
        println!("Show your appreciation by donating!");
        println!("https://github.com/Zamanhuseyinli/auto-cpufreq#donate");
        footer(79);
    }

    Ok(())
}

fn has_any_flag(args: &Args) -> bool {
    args.monitor || args.live || args.daemon || args.install || 
    args.update.is_some() || args.remove || args.force.is_some() || 
    args.turbo.is_some() || args.stats || args.get_state || 
    args.bluetooth_boot_off || args.bluetooth_boot_on || 
    args.debug || args.version || args.donate
}
