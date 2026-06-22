// Hide the console window in release builds; keep it during debug for logging.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// Off Windows only the platform stubs are reachable, so suppress the resulting
// dead-code/unused-import noise there. The real target is Windows.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod autostart;
mod brightness;
mod config;
mod hook;
mod logging;

#[cfg(windows)]
mod app;

fn main() {
    #[cfg(windows)]
    app::run();

    #[cfg(not(windows))]
    eprintln!("brightvol is a Windows-only application.");
}
