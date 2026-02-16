// src/gui/objects.rs 

use gtk::{self, Box as GtkBox, Button, Label, Orientation, Revealer, RevealerTransitionType, ScrolledWindow};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::fs;
use std::process::Command;
use sysinfo::System; 
use crate::core::*;
use crate::globals::*;
use crate::power_helper::BLUETOOTHCTL_EXISTS;
use crate::modules::system_info::SystemInfo;


fn auto_cpufreq_stats_path() -> &'static str {
    "/var/run/auto-cpufreq.stats"
}

pub fn get_stats() -> String {
    fs::read_to_string(auto_cpufreq_stats_path())
        .ok()
        .map(|content| {
            content
                .lines()
                .rev()
                .take(50)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
pub fn get_version() -> String {
    if *IS_INSTALLED_WITH_AUR {
        std::process::Command::new("pacman")
            .args(&["-Qi", "auto-cpufreq"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|s| {
                s.lines()
                    .find(|line| line.contains("Version"))
                    .map(String::from)
            })
            .unwrap_or_else(|| "Unknown".to_string())
    } else {
        get_formatted_version().unwrap_or_else(|_| "Unknown".to_string())
    }
}
pub fn get_bluetooth_boot_status() -> Option<String> {
    if !*BLUETOOTHCTL_EXISTS {
        return None;
    }

    let btconf = "/etc/bluetooth/main.conf";
    match fs::read_to_string(btconf) {
        Ok(content) => {
            let mut in_policy_section = false;
            for line in content.lines() {
                let stripped = line.trim();
                
                if stripped.starts_with('[') {
                    in_policy_section = stripped.to_lowercase() == "[policy]";
                    continue;
                }
                
                if !in_policy_section || stripped.starts_with('#') || stripped.is_empty() {
                    continue;
                }
                
                if stripped.starts_with("AutoEnable=") {
                    let value = stripped.split('=').nth(1)?.trim().to_lowercase();
                    return Some(if value == "true" { "on" } else { "off" }.to_string());
                }
            }
            Some("on".to_string())
        }
        Err(_) => None,
    }
}

// RadioButtonView for Governor Override
pub struct RadioButtonView {
    container: GtkBox,
    default: Button,
    powersave: Button,
    performance: Button,
    set_by_app: Rc<RefCell<bool>>,
    selected: Rc<RefCell<Option<String>>>,
}

impl RadioButtonView {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Horizontal, 5);
        container.set_hexpand(true);

        let label = Label::new(Some("Governor Override"));
        label.set_widget_name("bold");

        let default = Button::with_label("Default");
        default.set_halign(gtk::Align::End);
        let powersave = Button::with_label("Powersave");
        powersave.set_halign(gtk::Align::End);
        let performance = Button::with_label("Performance");
        performance.set_halign(gtk::Align::End);

        let set_by_app = Rc::new(RefCell::new(true));
        let selected = Rc::new(RefCell::new(Some("Default".to_string())));

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let default_clone = default.clone();
        let powersave_clone = powersave.clone();
        let performance_clone = performance.clone();
        
        default.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Default".to_string());
                Self::on_button_toggled("reset");
                default_clone.set_sensitive(false);
                powersave_clone.set_sensitive(true);
                performance_clone.set_sensitive(true);
            }
        });

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let default_clone2 = default.clone();
        let powersave_clone2 = powersave.clone();
        let performance_clone2 = performance.clone();
        
        powersave.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Powersave".to_string());
                Self::on_button_toggled("powersave");
                default_clone2.set_sensitive(true);
                powersave_clone2.set_sensitive(false);
                performance_clone2.set_sensitive(true);
            }
        });

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let default_clone3 = default.clone();
        let powersave_clone3 = powersave.clone();
        let performance_clone3 = performance.clone();
        
        performance.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Performance".to_string());
                Self::on_button_toggled("performance");
                default_clone3.set_sensitive(true);
                powersave_clone3.set_sensitive(true);
                performance_clone3.set_sensitive(false);
            }
        });

        container.append(&label);
        container.append(&default);
        container.append(&powersave);
        container.append(&performance);

        let mut view = Self {
            container,
            default,
            powersave,
            performance,
            set_by_app,
            selected,
        };
        view.set_selected();
        view
    }

    fn on_button_toggled(override_val: &str) {
        let result = Command::new("pkexec")
            .arg("auto-cpufreq")
            .arg(format!("--force={}", override_val))
            .status();

        if let Ok(status) = result {
            if status.code() == Some(126) || status.code() == Some(127) {
                eprintln!("Authorization failed");
            }
        }
    }

    fn set_selected(&mut self) {
        *self.set_by_app.borrow_mut() = true;
        let state = AutoCpuFreqState::new();
        let override_val = get_override(&state);
        let (label, active_btn) = match override_val {
            GovernorOverride::Powersave => ("Powersave", 1),
            GovernorOverride::Performance => ("Performance", 2),
            GovernorOverride::Default => ("Default", 0),
        };
        *self.selected.borrow_mut() = Some(label.to_string());
        
        self.default.set_sensitive(active_btn != 0);
        self.powersave.set_sensitive(active_btn != 1);
        self.performance.set_sensitive(active_btn != 2);
        
        *self.set_by_app.borrow_mut() = false;
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// CPUTurboOverride widget
pub struct CPUTurboOverride {
    container: GtkBox,
    auto: Button,
    never: Button,
    always: Button,
    set_by_app: Rc<RefCell<bool>>,
    selected: Rc<RefCell<Option<String>>>,
}

impl CPUTurboOverride {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Horizontal, 5);
        container.set_hexpand(true);

        let label = Label::new(Some("CPU Turbo Override"));
        label.set_widget_name("bold");

        let auto = Button::with_label("Auto");
        auto.set_halign(gtk::Align::End);
        let never = Button::with_label("Never");
        never.set_halign(gtk::Align::End);
        let always = Button::with_label("Always");
        always.set_halign(gtk::Align::End);

        let set_by_app = Rc::new(RefCell::new(true));
        let selected = Rc::new(RefCell::new(Some("Auto".to_string())));

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let auto_clone = auto.clone();
        let never_clone = never.clone();
        let always_clone = always.clone();
        
        auto.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Auto".to_string());
                Self::on_button_toggled("auto");
                auto_clone.set_sensitive(false);
                never_clone.set_sensitive(true);
                always_clone.set_sensitive(true);
            }
        });

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let auto_clone2 = auto.clone();
        let never_clone2 = never.clone();
        let always_clone2 = always.clone();
        
        never.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Never".to_string());
                Self::on_button_toggled("never");
                auto_clone2.set_sensitive(true);
                never_clone2.set_sensitive(false);
                always_clone2.set_sensitive(true);
            }
        });

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let auto_clone3 = auto.clone();
        let never_clone3 = never.clone();
        let always_clone3 = always.clone();
        
        always.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Always".to_string());
                Self::on_button_toggled("always");
                auto_clone3.set_sensitive(true);
                never_clone3.set_sensitive(true);
                always_clone3.set_sensitive(false);
            }
        });

        container.append(&label);
        container.append(&auto);
        container.append(&never);
        container.append(&always);

        let mut view = Self {
            container,
            auto,
            never,
            always,
            set_by_app,
            selected,
        };
        view.set_selected();
        view
    }

    fn on_button_toggled(override_val: &str) {
        let result = Command::new("pkexec")
            .arg("auto-cpufreq")
            .arg(format!("--turbo={}", override_val))
            .status();

        if let Ok(status) = result {
            if status.code() == Some(126) || status.code() == Some(127) {
                eprintln!("Authorization failed");
            }
        }
    }

    fn set_selected(&mut self) {
        *self.set_by_app.borrow_mut() = true;
        let state = AutoCpuFreqState::new();
        let override_val = get_turbo_override(&state);
        let (label, active_btn) = match override_val {
            TurboOverride::Auto => ("Auto", 0),
            TurboOverride::Never => ("Never", 1),
            TurboOverride::Always => ("Always", 2),
        };
        *self.selected.borrow_mut() = Some(label.to_string());
        
        self.auto.set_sensitive(active_btn != 0);
        self.never.set_sensitive(active_btn != 1);
        self.always.set_sensitive(active_btn != 2);
        
        *self.set_by_app.borrow_mut() = false;
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// BluetoothBootControl widget
pub struct BluetoothBootControl {
    container: GtkBox,
    _advanced_btn: Button,
    _revealer: Revealer,
    on_btn: Button,
    off_btn: Button,
    set_by_app: Rc<RefCell<bool>>,
    selected: Rc<RefCell<Option<String>>>,
}

impl BluetoothBootControl {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 10);
        container.set_hexpand(true);

        let advanced_btn = Button::with_label("Advanced Settings");
        advanced_btn.set_halign(gtk::Align::Start);

        let revealer = Revealer::new();
        revealer.set_transition_type(RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(200);

        let inner_box = GtkBox::new(Orientation::Horizontal, 5);
        inner_box.set_hexpand(true);

        let label = Label::new(Some("Bluetooth on Boot"));
        label.set_widget_name("bold");
        let on_btn = Button::with_label("On");
        on_btn.set_halign(gtk::Align::End);
        let off_btn = Button::with_label("Off");
        off_btn.set_halign(gtk::Align::End);

        let selected = Rc::new(RefCell::new(Some("On".to_string())));
        let set_by_app = Rc::new(RefCell::new(true));

        inner_box.append(&label);
        inner_box.append(&on_btn);
        inner_box.append(&off_btn);

        revealer.set_child(Some(&inner_box));

        container.append(&advanced_btn);
        container.append(&revealer);

        let revealer_clone = revealer.clone();
        let btn_clone = advanced_btn.clone();
        advanced_btn.connect_clicked(move |_| {
            let revealed = revealer_clone.reveals_child();
            revealer_clone.set_reveal_child(!revealed);
            if revealed {
                btn_clone.set_label("Advanced Settings");
            } else {
                btn_clone.set_label("Hide Advanced Settings");
            }
        });

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let on_clone = on_btn.clone();
        let off_clone = off_btn.clone();
        
        on_btn.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("On".to_string());
                Self::on_button_toggled("on");
                on_clone.set_sensitive(false);
                off_clone.set_sensitive(true);
            }
        });

        let sel_clone = selected.clone();
        let set_by_app_clone = set_by_app.clone();
        let on_clone2 = on_btn.clone();
        let off_clone2 = off_btn.clone();
        
        off_btn.connect_clicked(move |_| {
            if !*set_by_app_clone.borrow() {
                *sel_clone.borrow_mut() = Some("Off".to_string());
                Self::on_button_toggled("off");
                on_clone2.set_sensitive(true);
                off_clone2.set_sensitive(false);
            }
        });

        let mut control = Self {
            container,
            _advanced_btn: advanced_btn,
            _revealer: revealer,
            on_btn,
            off_btn,
            set_by_app,
            selected,
        };

        control.set_selected();
        control
    }

    fn on_button_toggled(action: &str) {
        let arg = if action == "on" {
            "--bluetooth_boot_on"
        } else {
            "--bluetooth_boot_off"
        };

        let result = Command::new("pkexec")
            .arg("auto-cpufreq")
            .arg(arg)
            .status();

        if let Ok(status) = result {
            if status.code() == Some(126) || status.code() == Some(127) {
                eprintln!("Authorization failed");
            }
        }
    }

    fn set_selected(&mut self) {
        *self.set_by_app.borrow_mut() = true;
        let active_btn = match get_bluetooth_boot_status() {
            Some(status) if status == "off" => {
                *self.selected.borrow_mut() = Some("Off".to_string());
                1
            }
            _ => {
                *self.selected.borrow_mut() = Some("On".to_string());
                0
            }
        };
        
        self.on_btn.set_sensitive(active_btn != 0);
        self.off_btn.set_sensitive(active_btn != 1);
        
        *self.set_by_app.borrow_mut() = false;
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// CurrentGovernorBox - FIXED: Use RefCell for interior mutability
pub struct CurrentGovernorBox {
    container: GtkBox,
    governor_label: Rc<RefCell<Label>>,
}

impl Clone for CurrentGovernorBox {
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            governor_label: self.governor_label.clone(),
        }
    }
}

impl CurrentGovernorBox {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Horizontal, 25);

        let static_label = Label::new(Some("Current Governor"));
        static_label.set_widget_name("bold");

        let governor_label = Label::new(Some(""));
        governor_label.set_halign(gtk::Align::End);

        container.append(&static_label);
        container.append(&governor_label);

        let mut box_widget = Self {
            container,
            governor_label: Rc::new(RefCell::new(governor_label)),
        };

        box_widget.refresh();
        box_widget
    }

    pub fn refresh(&mut self) {
        if let Ok(gov) = get_current_gov() {
            self.governor_label.borrow().set_text(&gov);
        }
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// BatteryInfoBox - FIXED: Use RefCell
pub struct BatteryInfoBox {
    container: GtkBox,
    status_label: Rc<RefCell<Label>>,
    percentage_label: Rc<RefCell<Label>>,
    ac_label: Rc<RefCell<Label>>,
    start_threshold_label: Rc<RefCell<Label>>,
    stop_threshold_label: Rc<RefCell<Label>>,
}

impl Clone for BatteryInfoBox {
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            status_label: self.status_label.clone(),
            percentage_label: self.percentage_label.clone(),
            ac_label: self.ac_label.clone(),
            start_threshold_label: self.start_threshold_label.clone(),
            stop_threshold_label: self.stop_threshold_label.clone(),
        }
    }
}

impl BatteryInfoBox {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 2);

        let header = Label::new(Some(&("-".repeat(20) + " Battery Stats " + &"-".repeat(20))));
        header.set_halign(gtk::Align::Start);

        let status_label = Label::new(Some(""));
        status_label.set_halign(gtk::Align::Start);

        let percentage_label = Label::new(Some(""));
        percentage_label.set_halign(gtk::Align::Start);

        let ac_label = Label::new(Some(""));
        ac_label.set_halign(gtk::Align::Start);

        let start_threshold_label = Label::new(Some(""));
        start_threshold_label.set_halign(gtk::Align::Start);

        let stop_threshold_label = Label::new(Some(""));
        stop_threshold_label.set_halign(gtk::Align::Start);

        container.append(&header);
        container.append(&status_label);
        container.append(&percentage_label);
        container.append(&ac_label);
        container.append(&start_threshold_label);
        container.append(&stop_threshold_label);

        let mut box_widget = Self {
            container,
            status_label: Rc::new(RefCell::new(status_label)),
            percentage_label: Rc::new(RefCell::new(percentage_label)),
            ac_label: Rc::new(RefCell::new(ac_label)),
            start_threshold_label: Rc::new(RefCell::new(start_threshold_label)),
            stop_threshold_label: Rc::new(RefCell::new(stop_threshold_label)),
        };

        box_widget.refresh();
        box_widget
    }

    pub fn refresh(&mut self) {
        let battery_info = SystemInfo::battery_info();

        let status = if battery_info.is_charging.unwrap_or(false) {
            "Charging"
        } else if battery_info.is_ac_plugged.unwrap_or(true) {
            "Charged"
        } else {
            "Discharging"
        };
        self.status_label.borrow().set_text(&format!("Battery status: {}", status));

        let percentage_text = battery_info.battery_level
            .map(|b| format!("{}%", b))
            .unwrap_or_else(|| "Unknown".to_string());
        self.percentage_label.borrow().set_text(&format!("Battery level: {}", percentage_text));

        let ac_text = battery_info.is_ac_plugged
            .map(|ac| if ac { "Yes" } else { "No" })
            .unwrap_or("Unknown");
        self.ac_label.borrow().set_text(&format!("AC plugged: {}", ac_text));

        let start_text = battery_info.charging_start_threshold
            .map(|t| format!("{}%", t))
            .unwrap_or_else(|| "Not set".to_string());
        self.start_threshold_label.borrow().set_text(&format!("Start threshold: {}", start_text));

        let stop_text = battery_info.charging_stop_threshold
            .map(|t| format!("{}%", t))
            .unwrap_or_else(|| "Not set".to_string());
        self.stop_threshold_label.borrow().set_text(&format!("Stop threshold: {}", stop_text));
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// CPUFreqScalingBox - FIXED: Use RefCell
pub struct CPUFreqScalingBox {
    container: GtkBox,
    governor_label: Rc<RefCell<Label>>,
    epp_label: Rc<RefCell<Label>>,
    epb_label: Rc<RefCell<Label>>,
}

impl Clone for CPUFreqScalingBox {
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            governor_label: self.governor_label.clone(),
            epp_label: self.epp_label.clone(),
            epb_label: self.epb_label.clone(),
        }
    }
}

impl CPUFreqScalingBox {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 2);

        let header = Label::new(Some(&("-".repeat(20) + " CPU Frequency Scaling " + &"-".repeat(20))));
        header.set_halign(gtk::Align::Start);

        let governor_label = Label::new(Some(""));
        governor_label.set_halign(gtk::Align::Start);

        let epp_label = Label::new(Some(""));
        epp_label.set_halign(gtk::Align::Start);

        let epb_label = Label::new(Some(""));
        epb_label.set_halign(gtk::Align::Start);

        container.append(&header);
        container.append(&governor_label);
        container.append(&epp_label);
        container.append(&epb_label);

        let mut box_widget = Self {
            container,
            governor_label: Rc::new(RefCell::new(governor_label)),
            epp_label: Rc::new(RefCell::new(epp_label)),
            epb_label: Rc::new(RefCell::new(epb_label)),
        };

        box_widget.refresh();
        box_widget
    }

    pub fn refresh(&mut self) {
        let mut sys = System::new_all();
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        
        self.refresh_with_system(&mut sys);
    }

    pub fn refresh_with_system(&mut self, sys: &mut System) {
        let report = SystemInfo::new().generate_system_report(sys);

        let gov = report.current_gov.unwrap_or_else(|| "Unknown".to_string());
        self.governor_label.borrow().set_text(&format!("Setting to use: \"{}\" governor", gov));

        if let Some(epp) = report.current_epp {
            self.epp_label.borrow().set_text(&format!("EPP setting: {}", epp));
            self.epp_label.borrow().set_visible(true);
        } else {
            self.epp_label.borrow().set_text("Not setting EPP (not supported by system)");
            self.epp_label.borrow().set_visible(true);
        }

        if let Some(epb) = report.current_epb {
            self.epb_label.borrow().set_text(&format!("Setting to use: \"{}\" EPB", epb));
            self.epb_label.borrow().set_visible(true);
        } else {
            self.epb_label.borrow().set_visible(false);
        }
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// SystemStatisticsBox - FIXED: Use RefCell
pub struct SystemStatisticsBox {
    container: GtkBox,
    cpu_usage_label: Rc<RefCell<Label>>,
    load_label: Rc<RefCell<Label>>,
    temp_label: Rc<RefCell<Label>>,
    fan_label: Rc<RefCell<Label>>,
    load_status_label: Rc<RefCell<Label>>,
    usage_status_label: Rc<RefCell<Label>>,
    turbo_label: Rc<RefCell<Label>>,
}

impl Clone for SystemStatisticsBox {
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            cpu_usage_label: self.cpu_usage_label.clone(),
            load_label: self.load_label.clone(),
            temp_label: self.temp_label.clone(),
            fan_label: self.fan_label.clone(),
            load_status_label: self.load_status_label.clone(),
            usage_status_label: self.usage_status_label.clone(),
            turbo_label: self.turbo_label.clone(),
        }
    }
}

impl SystemStatisticsBox {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 2);

        let header = Label::new(Some(&("-".repeat(20) + " System Statistics " + &"-".repeat(20))));
        header.set_halign(gtk::Align::Start);

        let cpu_usage_label = Label::new(Some(""));
        cpu_usage_label.set_halign(gtk::Align::Start);

        let load_label = Label::new(Some(""));
        load_label.set_halign(gtk::Align::Start);

        let temp_label = Label::new(Some(""));
        temp_label.set_halign(gtk::Align::Start);

        let fan_label = Label::new(Some(""));
        fan_label.set_halign(gtk::Align::Start);

        let load_status_label = Label::new(Some(""));
        load_status_label.set_halign(gtk::Align::Start);

        let usage_status_label = Label::new(Some(""));
        usage_status_label.set_halign(gtk::Align::Start);

        let turbo_label = Label::new(Some(""));
        turbo_label.set_halign(gtk::Align::Start);

        container.append(&header);
        container.append(&cpu_usage_label);
        container.append(&load_label);
        container.append(&temp_label);
        container.append(&fan_label);
        container.append(&load_status_label);
        container.append(&usage_status_label);
        container.append(&turbo_label);

        let mut box_widget = Self {
            container,
            cpu_usage_label: Rc::new(RefCell::new(cpu_usage_label)),
            load_label: Rc::new(RefCell::new(load_label)),
            temp_label: Rc::new(RefCell::new(temp_label)),
            fan_label: Rc::new(RefCell::new(fan_label)),
            load_status_label: Rc::new(RefCell::new(load_status_label)),
            usage_status_label: Rc::new(RefCell::new(usage_status_label)),
            turbo_label: Rc::new(RefCell::new(turbo_label)),
        };

        box_widget.refresh();
        box_widget
    }

    pub fn refresh(&mut self) {
        let mut sys = System::new_all();
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        
        self.refresh_with_system(&mut sys);
    }

    pub fn refresh_with_system(&mut self, sys: &mut System) {
        let report = SystemInfo::new().generate_system_report(sys);

        self.cpu_usage_label.borrow().set_text(&format!("Total CPU usage: {:.1} %", report.cpu_usage));
        self.load_label.borrow().set_text(&format!("Total system load: {:.2}", report.load));

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter().map(|c| c.temperature).sum::<f32>() / report.cores_info.len() as f32;
            self.temp_label.borrow().set_text(&format!("Average temp. of all cores: {:.2} °C", avg_temp));
            self.temp_label.borrow().set_visible(true);
        } else {
            self.temp_label.borrow().set_visible(false);
        }

        if let Some(fan) = report.cpu_fan_speed {
            self.fan_label.borrow().set_text(&format!("CPU fan speed: {} RPM", fan));
            self.fan_label.borrow().set_visible(true);
        } else {
            self.fan_label.borrow().set_visible(false);
        }

        if let Some((a, b, c)) = report.avg_load {
            let load_status = if report.load < 1.0 { "Load optimal" } else { "Load high" };
            self.load_status_label.borrow().set_text(&format!("{} (load average: {:.2}, {:.2}, {:.2})", load_status, a, b, c));
            self.load_status_label.borrow().set_visible(true);
        } else {
            self.load_status_label.borrow().set_visible(false);
        }

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter().map(|c| c.temperature).sum::<f32>() / report.cores_info.len() as f32;
            let usage_status = if report.cpu_usage < 70.0 { "Optimal" } else { "High" };
            let temp_status = if avg_temp > 75.0 { "high" } else { "normal" };
            self.usage_status_label.borrow().set_text(&format!("{} total CPU usage: {:.1}%, {} average core temp: {:.1}°C", usage_status, report.cpu_usage, temp_status, avg_temp));
            self.usage_status_label.borrow().set_visible(true);
        } else {
            self.usage_status_label.borrow().set_visible(false);
        }

        let turbo_status = match (report.is_turbo_on.0, report.is_turbo_on.1) {
            (Some(on), _) => if on { "On" } else { "Off" }.to_string(),
            (None, Some(auto)) => format!("Auto mode {}", if auto { "enabled" } else { "disabled" }),
            _ => "Unknown".into(),
        };
        self.turbo_label.borrow().set_text(&format!("Setting turbo boost: {}", turbo_status));
    }

    pub fn widget(&self) -> &GtkBox {
        &self.container
    }
}

// SystemStatsLabel - FIXED: Use RefCell
pub struct SystemStatsLabel {
    scrolled: ScrolledWindow,
    label: Rc<RefCell<Label>>,
}

impl Clone for SystemStatsLabel {
    fn clone(&self) -> Self {
        Self {
            scrolled: self.scrolled.clone(),
            label: self.label.clone(),
        }
    }
}

impl SystemStatsLabel {
    pub fn new() -> Self {
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        
        let label = Label::new(Some(""));
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Start);
        label.set_selectable(true);
        
        scrolled.set_child(Some(&label));

        let mut stats = Self { 
            scrolled, 
            label: Rc::new(RefCell::new(label)),
        };
        stats.refresh();
        stats
    }

    pub fn refresh(&mut self) {
        let mut sys = System::new_all();
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        
        self.refresh_with_system(&mut sys);
    }

    pub fn refresh_with_system(&mut self, sys: &mut System) {
        let sys_info = SystemInfo::new();
        
        let mut text = String::new();
        
        text.push_str("System Information\n\n");
        text.push_str(&format!("Linux distro: {} {}\n", sys_info.distro_name, sys_info.distro_version));
        text.push_str(&format!("Linux kernel: {}\n", sys_info.kernel_version));
        text.push_str(&format!("Processor: {}\n", sys_info.processor_model));
        text.push_str(&format!("Cores: {}\n", sys_info.total_cores.map_or("Unknown".to_string(), |c| c.to_string())));
        text.push_str(&format!("Architecture: {}\n", sys_info.architecture));
        text.push_str(&format!("Driver: {}\n\n", sys_info.cpu_driver.as_deref().unwrap_or("Unknown")));
        
        if crate::CONFIG.has_config() {
            text.push_str(&format!("Using settings defined in {} file\n\n", crate::CONFIG.get_path().display()));
        }
        
        text.push_str("Current CPU Stats\n\n");
        let max_freq = SystemInfo::cpu_max_freq();
        let min_freq = SystemInfo::cpu_min_freq();
        text.push_str(&format!("CPU max frequency: {} MHz\n", 
            max_freq.map_or("Unknown".to_string(), |f| format!("{:.0}", f))));
        text.push_str(&format!("CPU min frequency: {} MHz\n\n", 
            min_freq.map_or("Unknown".to_string(), |f| format!("{:.0}", f))));
        
        text.push_str("Core    Usage   Temperature     Frequency\n");
        
        let cores = SystemInfo::get_cpu_info(sys);
        for core in cores {
            text.push_str(&format!("CPU{:<2}    {:>4.1}%    {:>6.0} °C    {:>6.0} MHz\n",
                core.id, core.usage, core.temperature, core.frequency));
        }
        
        if let Some(fan) = SystemInfo::cpu_fan_speed() {
            text.push_str(&format!("\nCPU fan speed: {} RPM\n", fan));
        }

        self.label.borrow().set_text(&text);
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.scrolled
    }
}
