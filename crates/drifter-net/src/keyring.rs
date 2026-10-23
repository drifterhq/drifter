use std::collections::HashMap;

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const SECRETS: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SERVICE: &str = "org.freedesktop.Secret.Service";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const ITEM: &str = "org.freedesktop.Secret.Item";
const DEFAULT: &str = "/org/freedesktop/secrets/aliases/default";
const SCHEMA: &str = "org.freedesktop.NetworkManager.Connection";

type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

fn session() -> Option<(Connection, OwnedObjectPath)> {
    let bus = Connection::session().ok()?;
    let reply = bus
        .call_method(Some(SECRETS), SERVICE_PATH, Some(SERVICE), "OpenSession", &("plain", Value::from("")))
        .ok()?;
    let (_, opened) = reply.body().deserialize::<(OwnedValue, OwnedObjectPath)>().ok()?;
    Some((bus, opened))
}

fn find(bus: &Connection, attributes: &HashMap<&str, &str>) -> Vec<OwnedObjectPath> {
    bus.call_method(Some(SECRETS), SERVICE_PATH, Some(SERVICE), "SearchItems", &(attributes,))
        .ok()
        .and_then(|reply| reply.body().deserialize::<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>)>().ok())
        .map(|(unlocked, _)| unlocked)
        .unwrap_or_default()
}

pub fn lookup(uuid: &str, part: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some((bus, opened)) = session() else {
        return out;
    };
    let mut attributes: HashMap<&str, &str> = HashMap::new();
    attributes.insert("connection-uuid", uuid);
    attributes.insert("setting-name", part);
    let items = find(&bus, &attributes);
    if items.is_empty() {
        return out;
    }
    let Some(found) = bus
        .call_method(Some(SECRETS), SERVICE_PATH, Some(SERVICE), "GetSecrets", &(&items, &opened))
        .ok()
        .and_then(|reply| reply.body().deserialize::<HashMap<OwnedObjectPath, Secret>>().ok())
    else {
        return out;
    };
    for (item, (_, _, value, _)) in found {
        let key = bus
            .call_method(Some(SECRETS), item.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &(ITEM, "Attributes"))
            .ok()
            .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
            .and_then(|value| HashMap::<String, String>::try_from(value).ok())
            .and_then(|attributes| attributes.get("setting-key").cloned());
        if let (Some(key), Ok(value)) = (key, String::from_utf8(value)) {
            out.insert(key, value);
        }
    }
    let _ = bus.call_method(Some(SECRETS), opened.as_str(), Some("org.freedesktop.Secret.Session"), "Close", &());
    out
}

pub fn store(uuid: &str, id: &str, part: &str, key: &str, secret: &str) -> bool {
    let Some((bus, opened)) = session() else {
        return false;
    };
    let mut attributes: HashMap<&str, &str> = HashMap::new();
    attributes.insert("connection-uuid", uuid);
    attributes.insert("setting-name", part);
    attributes.insert("setting-key", key);
    attributes.insert("xdg:schema", SCHEMA);
    let mut properties: HashMap<&str, Value> = HashMap::new();
    properties.insert("org.freedesktop.Secret.Item.Label", Value::from(format!("Network secret for {id}/{part}/{key}")));
    properties.insert("org.freedesktop.Secret.Item.Attributes", Value::from(attributes));
    let secret = (opened.clone(), Vec::<u8>::new(), secret.as_bytes().to_vec(), "text/plain");
    let done = bus
        .call_method(Some(SECRETS), DEFAULT, Some(COLLECTION), "CreateItem", &(properties, secret, true))
        .is_ok();
    let _ = bus.call_method(Some(SECRETS), opened.as_str(), Some("org.freedesktop.Secret.Session"), "Close", &());
    done
}

pub fn forget(uuid: &str) {
    let Ok(bus) = Connection::session() else {
        return;
    };
    let mut attributes: HashMap<&str, &str> = HashMap::new();
    attributes.insert("connection-uuid", uuid);
    for item in find(&bus, &attributes) {
        let _ = bus.call_method(Some(SECRETS), item.as_str(), Some(ITEM), "Delete", &());
    }
}
