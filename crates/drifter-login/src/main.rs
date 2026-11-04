#[cfg(target_os = "linux")]
mod daemon;
#[cfg(target_os = "linux")]
mod pam;
#[cfg(target_os = "linux")]
mod worker;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    if let Some(fd) = std::env::var("DRIFTER_LOGIN_WORKER").ok().and_then(|fd| fd.parse().ok()) {
        return worker::run(fd);
    }
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("drifter-login runs as root, from drifter-login.service");
        return std::process::ExitCode::FAILURE;
    }
    daemon::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {}
