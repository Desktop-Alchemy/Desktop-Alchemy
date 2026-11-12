#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if std::env::args().any(|a| a == "--check-installation") {
        if let Err(e) = desktop_alchemy_core::app::check_installation() {
            eprintln!("{e:#}");
            std::process::exit(1)
        }
        return;
    }
    desktop_alchemy_core::app::run()
}
