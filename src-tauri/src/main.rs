#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tailtask_remote::process::worker_entry();
    tailtask_remote::lifecycle::background_entry();
    tailtask_desktop_lib::run();
}
