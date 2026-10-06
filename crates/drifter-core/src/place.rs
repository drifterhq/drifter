use serde::Deserialize;

pub const SLOTS: usize = 9;
pub const DOCK_PREFIX: &str = "drifter:place ";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    #[serde(default)]
    pub key: Option<String>,
}

impl Place {
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a place needs a name".into());
        }
        if !(self.x.is_finite() && self.y.is_finite()) {
            return Err(format!("place {} has no position", self.name));
        }
        if !(0.01..=8.0).contains(&self.zoom) {
            return Err(format!("place {} zoom runs from 0.01 to 8", self.name));
        }
        if let Some(key) = &self.key {
            crate::shortcut::Combo::parse(key)
                .map_err(|error| format!("place {}: {error}", self.name))?;
        }
        Ok(())
    }
}

pub fn slot_name(slot: usize) -> String {
    format!("Place {slot}")
}

pub fn go_key(slot: usize) -> String {
    format!("camera+{slot}")
}

pub fn save_key(slot: usize) -> String {
    format!("camera+ctrl+{slot}")
}

pub fn dock_command(name: &str) -> String {
    format!("{DOCK_PREFIX}{name}")
}

pub fn from_dock(command: &str) -> Option<&str> {
    command.strip_prefix(DOCK_PREFIX).map(str::trim).filter(|name| !name.is_empty())
}

pub fn icon(side: u32) -> crate::Image {
    use crate::{Rect, Vec2};
    let mut canvas = crate::paint::Canvas::new(side, side);
    let s = f64::from(side);
    let unit = s / 24.0;
    let tile = [52, 168, 140];
    let white = [255, 255, 255];
    canvas.round_rect(Rect::new(Vec2::splat(unit), Vec2::splat(s - 2.0 * unit)), 5.0 * unit, tile, 1.0);
    let middle = Vec2::splat(s / 2.0);
    canvas.circle(middle, 6.5 * unit, white, 1.0);
    canvas.circle(middle, 4.8 * unit, tile, 1.0);
    canvas.circle(middle, 2.2 * unit, white, 1.0);
    for (offset, size) in [
        (Vec2::new(-0.6, -9.5), Vec2::new(1.2, 3.5)),
        (Vec2::new(-0.6, 6.0), Vec2::new(1.2, 3.5)),
        (Vec2::new(-9.5, -0.6), Vec2::new(3.5, 1.2)),
        (Vec2::new(6.0, -0.6), Vec2::new(3.5, 1.2)),
    ] {
        canvas.round_rect(Rect::new(middle + offset * unit, size * unit), 0.4 * unit, white, 1.0);
    }
    canvas.image
}

pub fn table(place: &Place) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table.insert("name", toml_edit::value(place.name.as_str()));
    table.insert("x", toml_edit::value(place.x.round()));
    table.insert("y", toml_edit::value(place.y.round()));
    table.insert("zoom", toml_edit::value((place.zoom * 10_000.0).round() / 10_000.0));
    if let Some(key) = &place.key {
        table.insert("key", toml_edit::value(key.as_str()));
    }
    table
}

pub fn store(text: &str, place: &Place) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    let item = doc
        .entry("place")
        .or_insert(toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    let Some(list) = item.as_array_of_tables_mut() else {
        return Err("place in the config is not a list of [[place]] tables".into());
    };
    let found = list
        .iter_mut()
        .find(|table| table.get("name").and_then(|name| name.as_str()) == Some(place.name.as_str()));
    match found {
        Some(table) => {
            table.insert("x", toml_edit::value(place.x.round()));
            table.insert("y", toml_edit::value(place.y.round()));
            table.insert("zoom", toml_edit::value((place.zoom * 10_000.0).round() / 10_000.0));
        }
        None => list.push(table(place)),
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn place(name: &str, x: f64) -> Place {
        Place {
            name: name.into(),
            x,
            y: 200.0,
            zoom: 0.5,
            key: Some(go_key(1)),
        }
    }

    #[test]
    fn a_saved_place_is_added_then_moved_in_the_config() {
        let text = "# mine\n[lock]\nafter = 5\n";
        let once = store(text, &place("Place 1", 1000.4)).unwrap();
        assert!(once.starts_with("# mine"), "the rest of the file stays: {once}");
        let config = Config::parse(&once).unwrap();
        config.validate().unwrap();
        assert_eq!(config.place.len(), 1);
        assert_eq!((config.place[0].x, config.place[0].zoom), (1000.0, 0.5));
        assert_eq!(config.place[0].key.as_deref(), Some("camera+1"));
        assert_eq!(config.lock.after, 5);

        let twice = store(&once, &place("Place 1", 3000.0)).unwrap();
        let again = store(&twice, &place("Place 2", 50.0)).unwrap();
        let config = Config::parse(&again).unwrap();
        assert_eq!(config.place.len(), 2);
        assert_eq!(config.place[0].x, 3000.0, "the same name is moved, not added");
        assert_eq!(config.place[1].name, "Place 2");
    }

    #[test]
    fn a_dock_button_names_its_place() {
        let command = dock_command("Place 4");
        assert_eq!(from_dock(&command), Some("Place 4"));
        assert_eq!(from_dock("foot -e htop"), None);
        assert_eq!(from_dock(DOCK_PREFIX), None);
        let button = crate::config::Launch::Full {
            app: None,
            command: Some(command),
            icon: None,
        };
        assert_eq!(button.name(), "Place 4", "the dock tip names the place");
    }

    #[test]
    fn a_bad_place_is_refused() {
        assert!(place("A", 1.0).check().is_ok());
        assert!(Place { zoom: 0.0, ..place("A", 1.0) }.check().is_err());
        assert!(Place { name: " ".into(), ..place("A", 1.0) }.check().is_err());
        assert!(Place { key: Some("camera+".into()), ..place("A", 1.0) }.check().is_err());
        assert!(Config::parse("[[place]]\nname = \"A\"\nx = 1\ny = 2\nzoom = 1\nfly = true\n").is_err());
    }
}
