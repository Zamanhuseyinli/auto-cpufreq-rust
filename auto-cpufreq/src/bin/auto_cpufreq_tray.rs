fn main() {
    auto_cpufreq::gui::tray::TrayApp::run();
    loop {
        std::thread::park();
    }
}
