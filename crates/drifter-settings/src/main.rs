//! The settings window. It reads the config, shows it, and writes a change
//! back the moment it is made. Drifter sees the file change and applies it.

#[cfg(target_os = "linux")]
mod window;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match window::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("drifter-settings: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("drifter-settings runs on Linux under Drifter");
}
