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
use glib;

use crate::core::*;
use crate::globals::*;
use crate::power_helper::BLUETOOTHCTL_EXISTS;
use crate::modules::system_info::{SystemInfo, SystemReport};
use super::objects::*;

const HBOX_PADDING: i32 = 20;

fn css_file() -> &'static str {
    if *IS_INSTALLED_WITH_SNAP {
        "/snap/auto-cpufreq/current/style.css"
    } else {
        "/usr/local/share/auto-cpufreq/scripts/style.css"
    }
}

fn icon_file() -> &'static str {
    if *IS_INSTALLED_WITH_SNAP {
        "/snap/auto-cpufreq/current/icon.png"
    } else {
        "/usr/local/share/auto-cpufreq/images/icon.png"
    }
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
        if *IS_INSTALLED_WITH_SNAP {
            self.build_snap_view();
        } else if is_running("auto-cpufreq", "--daemon") {
            self.build_main_view();
        } else {
            self.build_daemon_not_running_view();
        }
    }

    fn build_snap_view(&self) {
        let vbox = GtkBox::new(Orientation::Vertical, 10);
        vbox.set_halign(gtk::Align::Center);
        vbox.set_valign(gtk::Align::Center);

        let label = Label::new(Some(
            "GUI not available due to Snap package confinement limitations.\n\
             Please install auto-cpufreq using auto-cpufreq-installer\n\
             Visit the GitHub repo for more info"
        ));
        label.set_justify(gtk::Justification::Center);

        let button = Button::with_label("GitHub Repo");
        button.connect_clicked(|_| {
            let _ = open::that(GITHUB);
        });

        vbox.append(&label);
        vbox.append(&button);

        self.window.set_child(Some(&vbox));
    }

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
                // Remove current child
                window.set_child(gtk::Widget::NONE);
                
                // Build monitor view
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
        use std::process::Command;

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

// Monitor Mode View
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

        // Header
        let header = GtkBox::new(Orientation::Horizontal, 0);
        header.set_margin_bottom(10);

        let title = Label::new(Some("Monitor Mode"));
        title.set_widget_name("bold");
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        header.append(&title);

        let back_button = Button::with_label("Back");
        let window_weak = parent_window.downgrade();
        back_button.connect_clicked(move |_| {
            if let Some(window) = window_weak.upgrade() {
                // Remove current child
                window.set_child(gtk::Widget::NONE);
                
                // Rebuild daemon not running view
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

        // Two column layout
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

        let running = Rc::new(RefCell::new(true));
        
        let mut view = Self {
            container,
            title,
            left_box,
            right_box,
            running,
        };

        view.refresh();
        view.setup_refresh();
        view
    }

    fn setup_refresh(&self) {
        let left_box = self.left_box.clone();
        let right_box = self.right_box.clone();
        let title = self.title.clone();
        let running = self.running.clone();

        glib::timeout_add_seconds_local(5, move || {
            if !*running.borrow() {
                return glib::ControlFlow::Break;
            }

            let report = SystemInfo::new().generate_system_report();
            Self::update_display(&left_box, &right_box, &title, &report);
            glib::ControlFlow::Continue
        });
    }

    fn refresh(&mut self) {
        let report = SystemInfo::new().generate_system_report();
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

        // Update title with timestamp
        let current_time = chrono::Local::now().format("%H:%M:%S");
        title.set_text(&format!("Monitor Mode - {}", current_time));

        // Left column - System Information
        left_box.append(&Self::create_separator("System Information"));
        left_box.append(&Self::create_label(&format!("Linux distro: {} {}", report.distro_name, report.distro_ver), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Linux kernel: {}", report.kernel_version), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Processor: {}", report.processor_model), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Cores: {:?}", report.total_core), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Architecture: {}", report.arch), gtk::Align::Start));
        left_box.append(&Self::create_label(&format!("Driver: {:?}", report.cpu_driver), gtk::Align::Start));

        if crate::CONFIG.has_config() {
            left_box.append(&Self::create_label(&format!("\nUsing settings defined in {}", crate::CONFIG.get_path().display()), gtk::Align::Start));
        }

        left_box.append(&Self::create_label("", gtk::Align::Start));

        // CPU Stats
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

        // Right column - Battery Stats
        right_box.append(&Self::create_separator("Battery Stats"));
        right_box.append(&Self::create_label(&format!("Battery status: {:?}", report.battery_info), gtk::Align::Start));
        
        let battery_level = report.battery_info.battery_level
            .map(|b| format!("{}%", b))
            .unwrap_or_else(|| "Unknown".to_string());
        right_box.append(&Self::create_label(&format!("Battery percentage: {}", battery_level), gtk::Align::Start));

        let ac_status = report.battery_info.is_ac_plugged
            .map(|ac| if ac { "Yes" } else { "No" })
            .unwrap_or("Unknown");
        right_box.append(&Self::create_label(&format!("AC plugged: {}", ac_status), gtk::Align::Start));

        let start_threshold = report.battery_info.charging_start_threshold
            .map(|t| t.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        right_box.append(&Self::create_label(&format!("Charging start threshold: {}", start_threshold), gtk::Align::Start));

        let stop_threshold = report.battery_info.charging_stop_threshold
            .map(|t| t.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        right_box.append(&Self::create_label(&format!("Charging stop threshold: {}", stop_threshold), gtk::Align::Start));
        right_box.append(&Self::create_label("", gtk::Align::Start));

        // CPU Frequency Scaling
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

        // System Statistics
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
            let suggested_turbo = SystemInfo::turbo_on_suggestion();
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
        
        // Try to set icon if it exists
        if std::fs::metadata(icon_file()).is_ok() {
            let _ = tool_window.borrow().window.set_icon_name(Some("auto-cpufreq"));
        }
        
        tool_window.borrow_mut().build();
        tool_window.borrow().show();
    });

    app.run();
}
