use ksni::{Tray, TrayService, MenuItem, ToolTip};
use ksni::menu::StandardItem;
use std::process::Command;

fn get_icon_path() -> String {
    "/usr/local/share/auto-cpufreq/images/icon.png".to_string()
}

pub struct AutoCpufreqTray;
pub struct TrayApp;

impl Tray for AutoCpufreqTray {
    fn id(&self) -> String {
        "auto-cpufreq-tray".into()
    }

    fn icon_theme_path(&self) -> String {
        let icon_path = get_icon_path();
        let path = std::path::Path::new(&icon_path);
        path.parent()
            .unwrap_or_else(|| std::path::Path::new("/"))
            .to_string_lossy()
            .into()
    }

    fn icon_name(&self) -> String {
        "icon".into()
    }

    fn title(&self) -> String {
        "auto-cpufreq".into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "auto-cpufreq".into(),
            description: "CPU Power Management Tool".into(),
            icon_name: "icon".into(),
            icon_pixmap: Vec::new(), 
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::MenuItem::*;
        vec![
            Standard(StandardItem {
                label: "Open GUI".into(),
                activate: Box::new(|_| {
                    let _ = Command::new("auto-cpufreq-gtk").spawn();
                }),
                ..Default::default()
            }),
            Separator,
            Standard(StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }),
        ]
    }
}

impl TrayApp {
    pub fn run() {
        let service = TrayService::new(AutoCpufreqTray);
        service.spawn();
        println!("auto-cpufreq tray icon is running via D-Bus...");
    }
}
