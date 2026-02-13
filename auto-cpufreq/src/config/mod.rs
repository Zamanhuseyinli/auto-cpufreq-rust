// src/config/mod.rs

pub mod config;
pub mod config_event_handler;

pub use config::{Config, find_config_file, CONFIG};
pub use config_event_handler::ConfigEventHandler;
