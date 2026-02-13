pub mod globals;
pub mod tlp_stat_parser;
pub mod power_helper;
pub mod config;
pub mod core;
pub mod battery;
pub mod modules;

// Re-exports
pub use globals::*;
pub use config::{CONFIG, find_config_file}; // CONFIG re-export now works

#[cfg(feature = "gui")]
pub mod gui;
