use std::collections::HashMap;

const UNITS: [&str; 2] = [
    "xdg-desktop-portal-wlr.service",
    "xdg-desktop-portal.service",
];

pub fn announce(socket: &str) {
    let mut pairs: HashMap<String, String> = HashMap::new();
    pairs.insert("WAYLAND_DISPLAY".into(), socket.to_string());
    pairs.insert(
        "XDG_CURRENT_DESKTOP".into(),
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "drifter".into()),
    );
    pairs.insert("XDG_SESSION_TYPE".into(), "wayland".into());
    if let Ok(theme) = std::env::var("QT_QPA_PLATFORMTHEME") {
        pairs.insert("QT_QPA_PLATFORMTHEME".into(), theme);
    }
    if let Some(display) = crate::apps::x11_display() {
        pairs.insert("DISPLAY".into(), display);
    }
    let spawned = std::thread::Builder::new()
        .name("drifter-portal".to_string())
        .spawn(move || {
            if let Err(error) = tell(&pairs) {
                log!("drifter: the desktop portals may not find drifter: {error}");
            }
        });
    if let Err(error) = spawned {
        log!("drifter: cannot tell the session about drifter: {error}");
    }
}

fn tell(pairs: &HashMap<String, String>) -> Result<(), String> {
    let address = crate::tray::bus_address()?;
    let bus = zbus::blocking::connection::Builder::address(address.as_str())
        .and_then(|builder| {
            builder
                .method_timeout(std::time::Duration::from_secs(2))
                .build()
        })
        .map_err(|e| e.to_string())?;
    bus.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "UpdateActivationEnvironment",
        pairs,
    )
    .map_err(|e| format!("activation environment: {e}"))?;
    let lines: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let systemd = |method: &str, body: &dyn Fn() -> zbus::Result<zbus::message::Message>| {
        body().map_err(|e| format!("systemd {method}: {e}"))
    };
    systemd("SetEnvironment", &|| {
        bus.call_method(
            Some("org.freedesktop.systemd1"),
            "/org/freedesktop/systemd1",
            Some("org.freedesktop.systemd1.Manager"),
            "SetEnvironment",
            &(lines.clone(),),
        )
    })?;
    for unit in UNITS {
        if let Err(error) = systemd("TryRestartUnit", &|| {
            bus.call_method(
                Some("org.freedesktop.systemd1"),
                "/org/freedesktop/systemd1",
                Some("org.freedesktop.systemd1.Manager"),
                "TryRestartUnit",
                &(unit, "replace"),
            )
        }) {
            log!("drifter: {unit}: {error}");
        }
    }
    Ok(())
}
