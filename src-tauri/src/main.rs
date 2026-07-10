// Prevent a console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop entry point. All logic lives in the shared library so the same
//! `run()` powers both the desktop binary and the Android app.

fn main() {
    crosstrace_lib::run();
}
