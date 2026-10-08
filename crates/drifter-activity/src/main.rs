macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

#[cfg(target_os = "linux")]
mod access;
#[cfg(target_os = "linux")]
mod probe;
#[cfg(target_os = "linux")]
mod window;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match window::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log!("drifter-activity: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    log!("drifter-activity runs on Linux under Drifter");
}
