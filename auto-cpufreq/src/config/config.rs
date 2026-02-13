// src/config/config.rs

use anyhow::{Result, bail};
use notify::{Watcher, RecursiveMode};
use notify::event::{EventKind, ModifyKind, CreateKind, RemoveKind};

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

// `ini` crate no longer exposes `Ini` at root; it's re-exported via configparser
use configparser::ini::Ini;

pub struct Config {
    path: Arc<Mutex<PathBuf>>,
    config: Arc<Mutex<Ini>>,
    watcher: Arc<Mutex<Option<notify::RecommendedWatcher>>>,
}

impl Config {
    pub fn new() -> Self {
        Config {
            path: Arc::new(Mutex::new(PathBuf::new())),
            config: Arc::new(Mutex::new(Ini::new())),
            watcher: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_path(&self, path: PathBuf) -> Result<()> {
        *self.path.lock().unwrap() = path.clone();
        
        if path.exists() {
            self.update_config()?;
        }

        // Setup file watcher
        self.setup_watcher(&path)?;

        Ok(())
    }

    fn setup_watcher(&self, path: &Path) -> Result<()> {
        // annotate clone types to satisfy inference
        let config_clone: Arc<Mutex<Ini>> = Arc::clone(&self.config);
        let path_clone = Arc::clone(&self.path);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            match res {
                Ok(event) => {
                    let should_update = matches!(
                        event.kind,
                        EventKind::Create(CreateKind::File) |
                        EventKind::Modify(ModifyKind::Data(_)) |
                        EventKind::Remove(RemoveKind::File)
                    );

                    if should_update {
                        let current_path = path_clone.lock().unwrap().clone();
                        
                        // Check if the event is for our config file
                        for path in &event.paths {
                            if path == &current_path || 
                               path.with_extension("").with_extension("") == current_path.with_extension("").with_extension("") {
                                // load a fresh Ini instance and replace if successful
                                let mut new_config = Ini::new();
                                if new_config.load(current_path.to_str().unwrap_or("")).is_ok() {
                                    *config_clone.lock().unwrap() = new_config;
                                }
                                break;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Watch error: {:?}", e),
            }
        })?;

        if let Some(parent) = path.parent() {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }

        *self.watcher.lock().unwrap() = Some(watcher);

        Ok(())
    }

    pub fn has_config(&self) -> bool {
        self.path.lock().unwrap().exists()
    }

    pub fn get_path(&self) -> PathBuf {
        self.path.lock().unwrap().clone()
    }

    pub fn update_config(&self) -> Result<()> {
        let path = self.path.lock().unwrap().clone();
        
        let mut new_config = Ini::new();
        match new_config.load(path.to_str().unwrap_or("")) {
            Ok(_) => {
                *self.config.lock().unwrap() = new_config;
                Ok(())
            }
            Err(e) => {
                eprintln!("The following error occurred while parsing the config file:\n{}", e);
                Ok(()) // Don't propagate the error, just log it
            }
        }
    }

    pub fn get_string(&self, section: &str, key: &str) -> Result<Option<String>> {
        let config = self.config.lock().unwrap();
        Ok(config
            .get(section, key))
    }

    pub fn get_bool(&self, section: &str, key: &str) -> Result<bool> {
        let value = self.get_string(section, key)?;
        
        match value.as_deref() {
            Some("true") | Some("True") | Some("1") | Some("yes") | Some("Yes") => Ok(true),
            Some("false") | Some("False") | Some("0") | Some("no") | Some("No") => Ok(false),
            Some(v) => bail!("Invalid boolean value: {}", v),
            None => Ok(false),
        }
    }

    pub fn get_int(&self, section: &str, key: &str) -> Result<Option<i32>> {
        let value = self.get_string(section, key)?;
        
        match value {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    pub fn get_threshold(&self, mode: &str) -> Result<u8> {
        let key = match mode {
            "start" => "charging_start_threshold",
            "stop" => "charging_stop_threshold",
            _ => bail!("Invalid threshold mode: {}", mode),
        };

        let value = self.get_int("battery", key)?;
        
        match value {
            Some(v) if v >= 0 && v <= 100 => Ok(v as u8),
            Some(v) => bail!("Threshold value out of range (0-100): {}", v),
            None => Ok(if mode == "start" { 0 } else { 100 }),
        }
    }

    pub fn has_option(&self, section: &str, key: &str) -> bool {
        self.config
            .lock()
            .unwrap()
            .get(section, key)
            .is_some()
    }

    pub fn get(&self, section: &str, key: &str, fallback: &str) -> String {
        self.get_string(section, key)
            .ok()
            .flatten()
            .unwrap_or_else(|| fallback.to_string())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

// Global config instance
lazy_static::lazy_static! {
    pub static ref CONFIG: Config = Config::new();
}

/// Find the config file to use
/// 
/// Look for a config file in the following prioritization order:
/// 1. Command line argument
/// 2. User config file
/// 3. System config file
pub fn find_config_file(args_config_file: Option<&str>) -> PathBuf {
    // Get home directory
    let home = get_home_dir();
    
    // Prepare paths
    let user_config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"));
    
    let user_config_file = user_config_dir.join("auto-cpufreq/auto-cpufreq.conf");
    let system_config_file = PathBuf::from("/etc/auto-cpufreq.conf");

    // (1) Command line argument was specified
    if let Some(config_path) = args_config_file {
        let path = PathBuf::from(config_path);
        if path.is_file() {
            return path;
        } else {
            eprintln!("Config file specified with '--config {}' not found.", config_path);
            std::process::exit(1);
        }
    }
    
    // (2) User config file
    if user_config_file.is_file() {
        return user_config_file;
    }
    
    // (3) System config file (default if nothing else is found)
    system_config_file
}

fn get_home_dir() -> PathBuf {
    // Try to get home directory from $SUDO_USER or $USER
    let output = Command::new("sh")
        .arg("-c")
        .arg("getent passwd ${SUDO_USER:-$USER} | cut -d: -f6")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let home = String::from_utf8_lossy(&output.stdout);
            PathBuf::from(home.trim())
        }
        _ => {
            // Fallback to HOME environment variable
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/root"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = Config::new();
        assert!(!config.has_config());
    }

    #[test]
    fn test_get_threshold_defaults() {
        let config = Config::new();
        assert_eq!(config.get_threshold("start").unwrap(), 0);
        assert_eq!(config.get_threshold("stop").unwrap(), 100);
    }

    #[test]
    fn test_get_bool() {
        let config = Config::new();
        
        // Test with no config file (should return false)
        assert!(!config.get_bool("battery", "enable_thresholds").unwrap());
    }
}
