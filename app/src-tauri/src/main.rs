// A release build is a GUI program: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fpsframer_app::run();
}
