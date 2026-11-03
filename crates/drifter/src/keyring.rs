
const PROGRAM: &str = "gnome-keyring-daemon";

pub fn start() {
    let spawned = std::thread::Builder::new()
        .name("drifter-keyring".to_string())
        .spawn(|| {
            let mut command = std::process::Command::new(PROGRAM);
            crate::apps::fresh_signals(&mut command);
            let result = command
                .args(["--start", "--components=secrets"])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output();
            match result {
                Ok(output) if output.status.success() => out!("keyring     secret service from {PROGRAM}"),
                Ok(output) => log!("drifter: {PROGRAM} stopped with {}, so apps cannot keep passwords", output.status),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    log!("drifter: install gnome-keyring so apps can keep passwords")
                }
                Err(error) => log!("drifter: cannot start {PROGRAM}: {error}"),
            }
        });
    if let Err(error) = spawned {
        log!("drifter: cannot start the keyring thread: {error}");
    }
}
