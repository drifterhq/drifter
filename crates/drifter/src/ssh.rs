use std::path::{Path, PathBuf};

const AGENTS: [(&str, &str); 3] = [
    ("gcr-ssh-agent.socket", "gcr/ssh"),
    ("ssh-agent.socket", "openssh_agent"),
    ("drifter-ssh-agent.service", "drifter-ssh-agent"),
];

const UNIT_DIRS: [&str; 4] = [
    "/etc/systemd/user",
    "/usr/local/lib/systemd/user",
    "/usr/lib/systemd/user",
    "/lib/systemd/user",
];

static SOCKET: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn socket() -> Option<String> {
    SOCKET.get().cloned()
}

fn is_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
}

fn unit_file(unit: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(unit)).find(|path| path.is_file())
}

fn listen_place(text: &str, runtime: &Path) -> Option<String> {
    let runtime = runtime.to_string_lossy();
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("ListenStream="))
        .map(str::trim)
        .find(|place| place.starts_with('/') || place.starts_with("%t/"))
        .map(|place| place.replacen("%t", &runtime, 1))
}

fn pick(runtime: &Path, dirs: &[PathBuf]) -> Option<(&'static str, String)> {
    AGENTS.iter().find_map(|(unit, fallback)| {
        let file = unit_file(unit, dirs)?;
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let place = listen_place(&text, runtime)
            .unwrap_or_else(|| runtime.join(fallback).to_string_lossy().into_owned());
        Some((*unit, place))
    })
}

fn unit_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        dirs.push(PathBuf::from(config).join("systemd/user"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config/systemd/user"));
    }
    dirs.extend(UNIT_DIRS.iter().map(PathBuf::from));
    dirs
}

pub fn start() {
    if let Some(held) = std::env::var_os("SSH_AUTH_SOCK").filter(|path| is_socket(Path::new(path))) {
        let held = held.to_string_lossy().into_owned();
        out!("ssh         agent already at {held}");
        let _ = SOCKET.set(held);
        return;
    }
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) else {
        log!("drifter: XDG_RUNTIME_DIR is not set, so there is no SSH agent");
        return;
    };
    let Some((unit, path)) = pick(Path::new(&runtime), &unit_dirs()) else {
        log!("drifter: no SSH agent unit is installed; install openssh or run scripts/install.sh");
        return;
    };
    crate::apps::set_env("SSH_AUTH_SOCK", Some(path.clone()));
    let _ = SOCKET.set(path.clone());
    let spawned = std::thread::Builder::new()
        .name("drifter-ssh".to_string())
        .spawn(move || match listen(unit) {
            Ok(()) => out!("ssh         agent {unit} at {path}"),
            Err(error) => log!("drifter: cannot start the SSH agent {unit}: {error}"),
        });
    if let Err(error) = spawned {
        log!("drifter: cannot start the SSH agent thread: {error}");
    }
}

fn listen(unit: &str) -> Result<(), String> {
    let address = crate::tray::bus_address()?;
    let bus = zbus::blocking::connection::Builder::address(address.as_str())
        .and_then(|builder| builder.method_timeout(std::time::Duration::from_secs(5)).build())
        .map_err(|e| e.to_string())?;
    bus.call_method(
        Some("org.freedesktop.systemd1"),
        "/org/freedesktop/systemd1",
        Some("org.freedesktop.systemd1.Manager"),
        "StartUnit",
        &(unit, "replace"),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("drifter-ssh-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_keyring_agent_wins_over_openssh() {
        let dir = folder("both");
        std::fs::write(dir.join("ssh-agent.socket"), "[Socket]\nListenStream=%t/ssh-agent.socket\n").unwrap();
        std::fs::write(dir.join("gcr-ssh-agent.socket"), "[Socket]\nPriority=6\nListenStream=%t/gcr/ssh\n").unwrap();
        let picked = pick(Path::new("/run/user/7"), &[dir.clone()]);
        assert_eq!(picked, Some(("gcr-ssh-agent.socket", "/run/user/7/gcr/ssh".to_string())));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn openssh_socket_place_comes_from_its_unit() {
        let dir = folder("openssh");
        std::fs::write(dir.join("ssh-agent.socket"), "[Socket]\nSocketMode=0600\nListenStream=%t/openssh_agent\n").unwrap();
        let picked = pick(Path::new("/run/user/7"), &[dir.clone()]);
        assert_eq!(picked, Some(("ssh-agent.socket", "/run/user/7/openssh_agent".to_string())));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn drifter_agent_is_the_last_choice() {
        let dir = folder("last");
        let empty = folder("empty");
        std::fs::write(dir.join("drifter-ssh-agent.service"), "[Service]\n").unwrap();
        let picked = pick(Path::new("/run/user/7"), &[empty.clone(), dir.clone()]);
        assert_eq!(
            picked,
            Some(("drifter-ssh-agent.service", "/run/user/7/drifter-ssh-agent".to_string()))
        );
        assert_eq!(pick(Path::new("/run/user/7"), &[empty.clone()]), None);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(empty);
    }
}
