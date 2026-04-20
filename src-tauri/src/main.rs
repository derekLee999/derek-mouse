// Prevents an additional console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    tauri_app_lib::run()
}
