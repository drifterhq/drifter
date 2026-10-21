macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

#[cfg(target_os = "linux")]
mod access;
#[cfg(target_os = "linux")]
mod heads;
#[cfg(target_os = "linux")]
mod logo;
#[cfg(target_os = "linux")]
mod net;
#[cfg(target_os = "linux")]
mod users;
#[cfg(target_os = "linux")]
mod window;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match window::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log!("drifter-settings: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    log!("drifter-settings runs on Linux under Drifter");
}
