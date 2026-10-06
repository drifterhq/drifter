use std::collections::HashMap;

use smithay::reexports::calloop::channel::{Channel, Sender, channel};
use zbus::zvariant::OwnedValue;

pub struct Heard {
    pub app: String,
    pub count: Option<i64>,
    pub progress: Option<f64>,
    pub urgent: bool,
}

pub fn start() -> Result<Channel<Heard>, String> {
    let (tx, rx) = channel::<Heard>();
    std::thread::Builder::new()
        .name("drifter-badges".to_string())
        .spawn(move || listen(tx))
        .map_err(|e| format!("cannot start the badge listener: {e}"))?;
    Ok(rx)
}

pub fn app_of(uri: &str) -> String {
    uri.trim_start_matches("application://")
        .trim_end_matches(".desktop")
        .to_string()
}

fn listen(tx: Sender<Heard>) {
    let Ok(address) = crate::tray::bus_address() else {
        return;
    };
    let Ok(builder) = zbus::blocking::connection::Builder::address(address.as_str()) else {
        return;
    };
    let Ok(connection) = builder.build() else {
        log!("drifter: no session bus for dock badges");
        return;
    };
    let Ok(rule) = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("com.canonical.Unity.LauncherEntry")
        .map(|rule| rule.build())
    else {
        return;
    };
    let Ok(messages) = zbus::blocking::MessageIterator::for_match_rule(rule, &connection, None)
    else {
        return;
    };
    let mut last: HashMap<String, Heard> = HashMap::new();
    for message in messages.flatten() {
        if message.header().member().map(|m| m.as_str()) != Some("Update") {
            continue;
        }
        let Ok((uri, props)) = message.body().deserialize::<(String, HashMap<String, OwnedValue>)>()
        else {
            continue;
        };
        let app = app_of(&uri);
        let held = last.remove(&app).unwrap_or(Heard {
            app: app.clone(),
            count: None,
            progress: None,
            urgent: false,
        });
        let mut count = held.count;
        let mut progress = held.progress;
        let mut urgent = held.urgent;
        let mut count_on = count.is_some();
        let mut progress_on = progress.is_some();
        for (key, value) in &props {
            match key.as_str() {
                "count" => count = value.downcast_ref::<i64>().ok().or(count),
                "count-visible" => count_on = value.downcast_ref::<bool>().ok().unwrap_or(count_on),
                "progress" => progress = value.downcast_ref::<f64>().ok().or(progress),
                "progress-visible" => progress_on = value.downcast_ref::<bool>().ok().unwrap_or(progress_on),
                "urgent" => urgent = value.downcast_ref::<bool>().ok().unwrap_or(urgent),
                _ => {}
            }
        }
        let heard = Heard {
            app: app.clone(),
            count: count.filter(|_| count_on),
            progress: progress.filter(|_| progress_on),
            urgent,
        };
        last.insert(
            app.clone(),
            Heard {
                app: app.clone(),
                count: heard.count,
                progress: heard.progress,
                urgent,
            },
        );
        if tx.send(heard).is_err() {
            return;
        }
    }
}
