// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // EAC-FR-NDDZ: the sandbox catalog guard is this executable, started again
    // with an argument of its own. It must not start the application.
    #[cfg(target_os = "macos")]
    if let Some(code) = synthesis_lib::catalog_guard_main() {
        std::process::exit(code);
    }
    synthesis_lib::run()
}
