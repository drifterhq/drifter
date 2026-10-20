macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

#[cfg(any(target_os = "linux", test))]
mod draw;
#[cfg(any(target_os = "linux", test))]
mod look;
#[cfg(any(target_os = "linux", test))]
mod markup;
#[cfg(target_os = "linux")]
mod window;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(std::path::PathBuf::from) else {
        log!("usage: drifter-markup <picture.png|picture.jpg>");
        return std::process::ExitCode::FAILURE;
    };
    match window::run(path) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log!("drifter-markup: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    log!("drifter-markup runs on Linux under Drifter");
}
