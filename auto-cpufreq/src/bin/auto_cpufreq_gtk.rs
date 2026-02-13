// src/bin/auto_cpufreq_gtk.rs
//! GTK frontend binary. Delegates to `auto_cpufreq::gui::app::run_app()` when
//! built with the `gui` feature.

#[cfg(feature = "gui")]
fn main() {
    auto_cpufreq::gui::app::run_app();
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("GUI support not compiled. Build with --features gui");
    std::process::exit(1);
}
