// src/gui/app.rs

use glib::clone;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, 
    Label, Orientation, ScrolledWindow, Separator, MessageDialog, MessageType, ButtonsType,
    DialogFlags, STYLE_PROVIDER_PRIORITY_APPLICATION
};
use gdk::Display;
use std::cell::RefCell;
use std::rc::Rc;
use std::process::Command;
use glib;
use sysinfo::System; 
use crate::core::*;
use crate::power_helper::BLUETOOTHCTL_EXISTS;
use crate::modules::system_info::{SystemInfo, SystemReport};
use super::objects::*;

const HBOX_PADDING: i32 = 20;

fn css_file() -> &'static str {
    "/usr/local/share/auto-cpufreq/scripts/style.css"
}

fn icon_file() -> &'static str {
    "/usr/local/share/auto-cpufreq/images/icon.png"
}

pub struct ToolWindow {
    window: ApplicationWindow,
    main_box: Option<GtkBox>,
    system_stats: Option<SystemStatsLabel>,
    current_governor: Option<CurrentGovernorBox>,
    battery_info: Option<BatteryInfoBox>,
    cpu_freq_scaling: Option<CPUFreqScalingBox>,
    system_stats_box: Option<SystemStatisticsBox>,
}

impl ToolWindow {
    pub fn new(app: &Application) -> Rc<RefCell<Self>> {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("auto-cpufreq")
            .default_width(900)
            .default_height(650)
            .build();

        window.set_resizable(true);

        let tool_window = Rc::new(RefCell::new(Self {
            window,
            main_box: None,
            system_stats: None,
            current_governor: None,
            battery_info: None,
            cpu_freq_scaling: None,
            system_stats_box: None,
        }));

        tool_window
    }

    fn load_css() {
        let provider = CssProvider::new();
        provider.load_from_path(css_file());
        gtk::style_context_add_provider_for_display(
            &Display::default().expect("Could not connect to display"),
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }


pub fn build(&mut self) {
    let daemon_is_running = Self::check_daemon_running();

    if daemon_is_running {
        self.build_main_view();
    } else {
        self.build_daemon_not_running_view();
    }
}
fn check_daemon_running() -> bool {
    // Method 1: Check via is_running (process list)
    if is_running("auto-cpufreq", "--daemon") {
        return true;
    }

    // Method 2: Check systemd service
    if let Ok(output) = Command::new("systemctl")
        .args(&["is-active", "auto-cpufreq"])
        .output()
    {
        let status = String::from_utf8_lossy(&output.stdout);
        if status.trim() == "active" {
            return true;
        }
    }

    // Method 3: Check if stats file is recent (fallback)
    let stats_path = "/var/run/auto-cpufreq.stats";
    if let Ok(metadata) = std::fs::metadata(stats_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                if elapsed.as_secs() < 20 {
                    return true;
                }
            }
        }
    }

    false
}
    // MERGED: Daemon not running view from version 2
    fn build_daemon_not_running_view(&mut self) {
        let vbox = GtkBox::new(Orientation::Vertical, 10);
        vbox.set_halign(gtk::Align::Center);
        vbox.set_valign(gtk::Align::Center);

        let label = Label::new(Some("auto-cpufreq daemon is not running"));
        let sublabel = Label::new(Some(
            "Install the daemon for permanent optimization, or use Monitor mode to preview"
        ));

        let button_box = GtkBox::new(Orientation::Horizontal, 10);
        button_box.set_halign(gtk::Align::Center);

        let install_button = Button::with_label("Install Daemon");
        let monitor_button = Button::with_label("Monitor Mode");

        // Clone window for closures
        let window_clone = self.window.clone();
        install_button.connect_clicked(move |_| {
            Self::install_daemon(&window_clone);
        });

        let window_weak = self.window.downgrade();
        monitor_button.connect_clicked(move |_| {
            if let Some(window) = window_weak.upgrade() {
                let monitor_view = MonitorModeView::new(&window);
                window.set_child(Some(monitor_view.widget()));
                window.show();
            }
        });

        button_box.append(&install_button);
        button_box.append(&monitor_button);

        vbox.append(&label);
        vbox.append(&sublabel);
        vbox.append(&button_box);

        self.window.set_child(Some(&vbox));
    }

    fn build_main_view(&mut self) {
        let hbox = GtkBox::new(Orientation::Horizontal, HBOX_PADDING);

        // Left side - System stats
        let system_stats = SystemStatsLabel::new();
        hbox.append(system_stats.widget());

        // Right side - Controls
        let vbox_right = GtkBox::new(Orientation::Vertical, 15);
        vbox_right.set_vexpand(true);
        vbox_right.set_hexpand(true);

        // Current Governor
        let current_governor = CurrentGovernorBox::new();
        vbox_right.append(current_governor.widget());

        // Radio buttons for governor override
        let radio_view = RadioButtonView::new();
        vbox_right.append(radio_view.widget());

        // Turbo override (if supported)
        let stats = get_stats();
        if !stats.contains("Warning: CPU turbo is not available") {
            let turbo_view = CPUTurboOverride::new();
            vbox_right.append(turbo_view.widget());
        }

        // Battery info
        let battery_info = BatteryInfoBox::new();
        vbox_right.append(battery_info.widget());

        // CPU Freq Scaling
        let cpu_freq_scaling = CPUFreqScalingBox::new();
        vbox_right.append(cpu_freq_scaling.widget());

        // System Statistics
        let system_stats_box = SystemStatisticsBox::new();
        vbox_right.append(system_stats_box.widget());

        // Bluetooth control
        if *BLUETOOTHCTL_EXISTS {
            let bluetooth_control = BluetoothBootControl::new();
            vbox_right.append(bluetooth_control.widget());
        }

        let scrolled_right = ScrolledWindow::new();
        scrolled_right.set_child(Some(&vbox_right));
        scrolled_right.set_vexpand(true);

        hbox.append(&scrolled_right);

        self.window.set_child(Some(&hbox));

        // Store references for refresh
        self.main_box = Some(hbox);
        self.system_stats = Some(system_stats);
        self.current_governor = Some(current_governor);
        self.battery_info = Some(battery_info);
        self.cpu_freq_scaling = Some(cpu_freq_scaling);
        self.system_stats_box = Some(system_stats_box);

        // Setup auto-refresh
        self.setup_refresh();
    }

    fn setup_refresh(&self) {
        let system_stats = self.system_stats.clone();
        let current_governor = self.current_governor.clone();
        let battery_info = self.battery_info.clone();
        let cpu_freq_scaling = self.cpu_freq_scaling.clone();
        let system_stats_box = self.system_stats_box.clone();

        glib::timeout_add_seconds_local(5, move || {
            if let Some(ref stats) = system_stats {
                let mut stats_mut = stats.clone();
                stats_mut.refresh();
            }
            if let Some(ref gov) = current_governor {
                let mut gov_mut = gov.clone();
                gov_mut.refresh();
            }
            if let Some(ref bat) = battery_info {
                let mut bat_mut = bat.clone();
                bat_mut.refresh();
            }
            if let Some(ref freq) = cpu_freq_scaling {
                let mut freq_mut = freq.clone();
                freq_mut.refresh();
            }
            if let Some(ref stats_box) = system_stats_box {
                let mut stats_box_mut = stats_box.clone();
                stats_box_mut.refresh();
            }

            glib::ControlFlow::Continue
        });
    }

    fn install_daemon(window: &ApplicationWindow) {
        let result = Command::new("pkexec")
            .arg("auto-cpufreq")
            .arg("--install")
            .status();

        match result {
            Ok(status) if status.success() => {
                let dialog = MessageDialog::new(
                    Some(window),
                    DialogFlags::MODAL,
                    MessageType::Info,
                    ButtonsType::Ok,
                    "Daemon successfully installed",
                );
                dialog.set_secondary_text(Some(
                    "The app will now close. Please reopen to apply changes"
                ));
                dialog.connect_response(clone!(@weak window => move |dialog, _| {
                    dialog.close();
                    window.close();
                }));
                dialog.present();
            }
            Ok(status) if status.code() == Some(126) || status.code() == Some(127) => {
                let dialog = MessageDialog::new(
                    Some(window),
                    DialogFlags::MODAL,
                    MessageType::Error,
                    ButtonsType::Ok,
                    "Error installing daemon",
                );
                dialog.set_secondary_text(Some("Authorization Failed"));
                dialog.connect_response(|dialog, _| {
                    dialog.close();
                });
                dialog.present();
            }
            Err(e) => {
                let dialog = MessageDialog::new(
                    Some(window),
                    DialogFlags::MODAL,
                    MessageType::Error,
                    ButtonsType::Ok,
                    "Daemon install failed",
                );
                dialog.set_secondary_text(Some(&format!("Error: {}", e)));
                dialog.connect_response(|dialog, _| {
                    dialog.close();
                });
                dialog.present();
            }
            _ => {}
        }
    }

    pub fn show(&self) {
        self.window.show();
    }

    pub fn window(&self) -> &ApplicationWindow {
        &self.window
    }
}

// MERGED: Monitor Mode View from version 2
pub struct MonitorModeView {
    container: GtkBox,
    title: Label,
    left_box: GtkBox,
    right_box: GtkBox,
    running: Rc<RefCell<bool>>,
}

impl MonitorModeView {
    pub fn new(parent_window: &ApplicationWindow) -> Self {
        let container = GtkBox::new(Orientation::Vertical, 5);
        container.set_margin_start(10);
        container.set_margin_end(10);
        container.set_margin_top(10);
        container.set_margin_bottom(10);

        let header = GtkBox::new(Orientation::Horizontal, 0);
        header.set_margin_bottom(10);

        let title = Label::new(Some("Monitor Mode"));
        title.set_widget_name("bold");
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        header.append(&title);

        let back_button = Button::with_label("Back");
        let window_weak = parent_window.downgrade();
        let running = Rc::new(RefCell::new(true));
        let running_clone = running.clone();
        
        back_button.connect_clicked(move |_| {
            *running_clone.borrow_mut() = false;
            
            if let Some(window) = window_weak.upgrade() {
                let vbox = GtkBox::new(Orientation::Vertical, 10);
                vbox.set_halign(gtk::Align::Center);
                vbox.set_valign(gtk::Align::Center);

                let label = Label::new(Some("auto-cpufreq daemon is not running"));
                let sublabel = Label::new(Some(
                    "Install the daemon for permanent optimization, or use Monitor mode to preview"
                ));

                let button_box = GtkBox::new(Orientation::Horizontal, 10);
                button_box.set_halign(gtk::Align::Center);

                let install_button = Button::with_label("Install Daemon");
                let monitor_button = Button::with_label("Monitor Mode");

                let window_clone = window.clone();
                install_button.connect_clicked(move |_| {
                    ToolWindow::install_daemon(&window_clone);
                });

                let window_weak2 = window.downgrade();
                monitor_button.connect_clicked(move |_| {
                    if let Some(win) = window_weak2.upgrade() {
                        let monitor_view = MonitorModeView::new(&win);
                        win.set_child(Some(monitor_view.widget()));
                        win.show();
                    }
                });

                button_box.append(&install_button);
                button_box.append(&monitor_button);

                vbox.append(&label);
                vbox.append(&sublabel);
                vbox.append(&button_box);

                window.set_child(Some(&vbox));
                window.show();
            }
        });
        header.append(&back_button);

        container.append(&header);

        let columns = GtkBox::new(Orientation::Horizontal, 20);
        columns.set_vexpand(true);
        columns.set_hexpand(true);

        let left_scroll = ScrolledWindow::new();
        left_scroll.set_vexpand(true);
        left_scroll.set_hexpand(true);
        
        let left_box = GtkBox::new(Orientation::Vertical, 2);
        left_box.set_valign(gtk::Align::Start);
        left_scroll.set_child(Some(&left_box));

        let separator = Separator::new(Orientation::Vertical);

        let right_scroll = ScrolledWindow::new();
        right_scroll.set_vexpand(true);
        right_scroll.set_hexpand(true);
        
        let right_box = GtkBox::new(Orientation::Vertical, 2);
        right_box.set_valign(gtk::Align::Start);
        right_scroll.set_child(Some(&right_box));

        columns.append(&left_scroll);
        columns.append(&separator);
        columns.append(&right_scroll);

        container.append(&columns);

        let mut view = Self {
            container,
            title,
            left_box,
            right_box,
            running,
        };

        view.do_refresh();
        view.setup_refresh();
        view
    }

    fn setup_refresh(&self) {
        let left_box = self.left_box.clone();
        let right_box = self.right_box.clone();
        let title = self.title.clone();
        let running = Rc::downgrade(&self.running);

        glib::timeout_add_local(std::time::Duration::from_secs(2), move || {
            let running_strong = match running.upgrade() {
                Some(r) => r,
                None => return glib::ControlFlow::Break,
            };

            if !*running_strong.borrow() {
                return glib::ControlFlow::Break;
            }

            let mut sys = System::new_all();
            sys.refresh_cpu();
            std::thread::sleep(std::time::Duration::from_millis(200));
            sys.refresh_cpu();
            
            let report = SystemInfo::new().generate_system_report(&mut sys);

            Self::update_display(&left_box, &right_box, &title, &report);
            glib::ControlFlow::Continue
        });
    }

    fn do_refresh(&mut self) {
        let mut sys = System::new_all();
        sys.refresh_cpu();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu();
        
        let report = SystemInfo::new().generate_system_report(&mut sys);
        Self::update_display(&self.left_box, &self.right_box, &self.title, &report);
    }

    fn clear_box(box_widget: &GtkBox) {
        while let Some(child) = box_widget.first_child() {
            box_widget.remove(&child);
        }
    }

    fn create_label(text: &str, halign: gtk::Align) -> Label {
        let label = Label::new(Some(text));
        label.set_halign(halign);
        label.set_selectable(true);
        label
    }

    fn create_separator(text: &str) -> Label {
        let label = Label::new(Some(&format!("{} {} {}", "-".repeat(28), text, "-".repeat(28))));
        label.set_halign(gtk::Align::Start);
        label
    }

    fn update_display(left_box: &GtkBox, right_box: &GtkBox, title: &Label, report: &SystemReport) {
        Self::clear_box(left_box);
        Self::clear_box(right_box);

        let current_time = chrono::Local::now().format("%H:%M:%S");
        title.set_text(&format!("Monitor Mode - {}", current_time));

        left_box.append(&Self::create_separator("System Information"));
        left_box.append(&Self::create_label(&format!("Linux distro: {} {}", report.distro_name, report.distro_ver), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Linux kernel: {}", report.kernel_version), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Processor: {}", report.processor_model), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Cores: {}", report.total_core.map_or("Unknown".to_string(), |c| c.to_string())), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Architecture: {}", report.arch), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Driver: {}", report.cpu_driver.as_deref().unwrap_or("Unknown")), gtk::Align::Start));

        if crate::CONFIG.has_config() {
            left_box.append(&Self::create_label(&format!("\nUsing settings defined in {}", crate::CONFIG.get_path().display()), gtk::Align::Start));
        }

        left_box.append(&Self::create_label("", gtk::Align::Start));

        left_box.append(&Self::create_separator("Current CPU Stats"));
        left_box.append(&Self::create_label(
            &format!("CPU max frequency: {} MHz", report.cpu_max_freq.map_or("Unknown".to_string(), |f| format!("{:.0}", f))),
            gtk::Align::Start
        ));
        left_box.append(&Self::create_label(
            &format!("CPU min frequency: {} MHz", report.cpu_min_freq.map_or("Unknown".to_string(), |f| format!("{:.0}", f))),
            gtk::Align::Start
        ));
        left_box.append(&Self::create_label("", gtk::Align::Start));
        left_box.append(&Self::create_label("Core    Usage   Temperature     Frequency", gtk::Align::Start));

        for core in &report.cores_info {
            left_box.append(&Self::create_label(
                &format!("CPU{:<2}    {:>4.1}%    {:>6.0} °C    {:>6.0} MHz", core.id, core.usage, core.temperature, core.frequency),
                gtk::Align::Start
            ));
        }

        if let Some(fan) = report.cpu_fan_speed {
            left_box.append(&Self::create_label("", gtk::Align::Start));
            left_box.append(&Self::create_label(&format!("CPU fan speed: {} RPM", fan), gtk::Align::Start));
        }

        right_box.append(&Self::create_separator("Battery Stats"));
        
        let battery_status = if report.battery_info.is_charging.unwrap_or(false) {
            "Charging"
        } else if report.battery_info.is_ac_plugged.unwrap_or(true) {
            "Charged"
        } else {
            "Discharging"
        };
        right_box.append(&Self::create_label(&format!("Battery status: {}", battery_status), gtk::Align::Start));
        
        let battery_level = report.battery_info.battery_level
            .map(|b| format!("{}%", b))
            .unwrap_or_else(|| "Unknown".to_string());
        right_box.append(&Self::create_label(&format!("Battery percentage: {}", battery_level), gtk::Align::Start));

        let ac_status = report.battery_info.is_ac_plugged
            .map(|ac| if ac { "Yes" } else { "No" })
            .unwrap_or("Unknown");
        right_box.append(&Self::create_label(&format!("AC plugged: {}", ac_status), gtk::Align::Start));

        let start_threshold = report.battery_info.charging_start_threshold
            .map(|t| format!("{}%", t))
            .unwrap_or_else(|| "Not set".to_string());
        right_box.append(&Self::create_label(&format!("Charging start threshold: {}", start_threshold), gtk::Align::Start));

        let stop_threshold = report.battery_info.charging_stop_threshold
            .map(|t| format!("{}%", t))
            .unwrap_or_else(|| "Not set".to_string());
        right_box.append(&Self::create_label(&format!("Charging stop threshold: {}", stop_threshold), gtk::Align::Start));
        right_box.append(&Self::create_label("", gtk::Align::Start));

        right_box.append(&Self::create_separator("CPU Frequency Scaling"));
        let current_gov = report.current_gov.as_deref().unwrap_or("Unknown");
        right_box.append(&Self::create_label(&format!("Setting to use: \"{}\" governor", current_gov), gtk::Align::Start));

        let suggested_gov = SystemInfo::governor_suggestion();
        if let (Some(current), Some(suggested)) = (&report.current_gov, &suggested_gov) {
            if current != suggested {
                right_box.append(&Self::create_label(&format!("Suggesting use of: \"{}\" governor", suggested), gtk::Align::Start));
            }
        }

        if let Some(epp) = &report.current_epp {
            right_box.append(&Self::create_label(&format!("EPP setting: {}", epp), gtk::Align::Start));
        } else {
            right_box.append(&Self::create_label("Not setting EPP (not supported by system)", gtk::Align::Start));
        }

        if let Some(epb) = &report.current_epb {
            right_box.append(&Self::create_label(&format!("Setting to use: \"{}\" EPB", epb), gtk::Align::Start));
        }

        right_box.append(&Self::create_label("", gtk::Align::Start));

        right_box.append(&Self::create_separator("System Statistics"));
        right_box.append(&Self::create_label(&format!("Total CPU usage: {:.1} %", report.cpu_usage), gtk::Align::Start));
        right_box.append(&Self::create_label(&format!("Total system load: {:.2}", report.load), gtk::Align::Start));

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter().map(|c| c.temperature).sum::<f32>() / report.cores_info.len() as f32;
            right_box.append(&Self::create_label(&format!("Average temp. of all cores: {:.2} °C", avg_temp), gtk::Align::Start));
        }

        if let Some((a, b, c)) = report.avg_load {
            let load_status = if report.load < 1.0 { "Load optimal" } else { "Load high" };
            right_box.append(&Self::create_label(
                &format!("{} (load average: {:.2}, {:.2}, {:.2})", load_status, a, b, c),
                gtk::Align::Start
            ));
        }

        if !report.cores_info.is_empty() {
            let avg_temp: f32 = report.cores_info.iter().map(|c| c.temperature).sum::<f32>() / report.cores_info.len() as f32;
            let usage_status = if report.cpu_usage < 70.0 { "Optimal" } else { "High" };
            let temp_status = if avg_temp > 75.0 { "high" } else { "normal" };
            right_box.append(&Self::create_label(
                &format!("{} total CPU usage: {:.1}%, {} average core temp: {:.1}°C", usage_status, report.cpu_usage, temp_status, avg_temp),
                gtk::Align::Start
            ));
        }

        let turbo_status = match (report.is_turbo_on.0, report.is_turbo_on.1) {
            (Some(on), _) => if on { "On".to_string() } else { "Off".to_string() },
            (None, Some(auto)) => format!("Auto mode {}", if auto { "enabled" } else { "disabled" }),
            _ => "Unknown".to_string(),
        };
        right_box.append(&Self::create_label(&format!("Setting turbo boost: {}", turbo_status), gtk::Align::Start));

        if let Some(on) = report.is_turbo_on.0 {
            let mut temp_sys = System::new_all();
            let suggested_turbo = SystemInfo::turbo_on_suggestion(&mut temp_sys);
            if suggested_turbo != on {
                let turbo_text = if suggested_turbo { "on" } else { "off" };
                right_box.append(&Self::create_label(&format!("Suggesting to set turbo boost: {}", turbo_text), gtk::Align::Start));
            }
        }
    }
    
    pub fn widget(&self) -> &GtkBox {  
        &self.container
    }

    pub fn cleanup(&self) {  
        *self.running.borrow_mut() = false;
    }
}

impl Drop for MonitorModeView {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub fn run_app() {
    let app = Application::builder()
        .application_id("org.auto_cpufreq.GUI")
        .build();

    app.connect_activate(|app| {
        let tool_window = ToolWindow::new(app);
        ToolWindow::load_css();
        
        if std::fs::metadata(icon_file()).is_ok() {
            let borrowed = tool_window.borrow();
            let _ = borrowed.window.set_icon_name(Some("auto-cpufreq"));
        }
        
        {
            let mut tw = tool_window.borrow_mut();
            tw.build();
        }
        
        tool_window.borrow().show();
    });

    app.run();
}
