#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    scrobble_bridge_desktop_lib::run();
}
