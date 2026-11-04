#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

#[cfg(target_os = "linux")]
mod config;
#[cfg(target_os = "linux")]
mod ffi;
#[cfg(target_os = "linux")]
mod font;
#[cfg(target_os = "linux")]
mod pty;
#[cfg(target_os = "linux")]
mod render;
#[cfg(any(target_os = "linux", test))]
mod select;
#[cfg(any(target_os = "linux", test))]
mod tabs;
#[cfg(target_os = "linux")]
mod ui;
#[cfg(target_os = "linux")]
mod vt;
#[cfg(target_os = "linux")]
mod window;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match window::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log!("drifter-terminal: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    log!("drifter-terminal runs on Linux under Drifter");
}
