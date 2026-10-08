use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

pub const LANGUAGES: [(&str, &str); 4] = [("en", "English"), ("es", "Español"), ("de", "Deutsch"), ("fr", "Français")];

const SPANISH: &str = include_str!("../lang/es.txt");
const GERMAN: &str = include_str!("../lang/de.txt");
const FRENCH: &str = include_str!("../lang/fr.txt");

type Catalog = HashMap<&'static str, &'static str>;

static NOW: RwLock<Option<&'static Catalog>> = RwLock::new(None);
static CATALOGS: OnceLock<[Catalog; 3]> = OnceLock::new();

pub fn parse(text: &'static str) -> Catalog {
    let mut out = HashMap::new();
    let mut english: Option<&'static str> = None;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            english = None;
            continue;
        }
        match (line.strip_prefix("= "), english) {
            (Some(said), Some(from)) => {
                out.insert(from, said);
                english = None;
            }
            (Some(_), None) => {}
            (None, _) => english = Some(line),
        }
    }
    out
}

fn catalogs() -> &'static [Catalog; 3] {
    CATALOGS.get_or_init(|| [parse(SPANISH), parse(GERMAN), parse(FRENCH)])
}

pub fn from_env() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

pub fn pick(chosen: Option<&str>, system: &str) -> &'static str {
    let wanted = match chosen.map(str::trim) {
        Some(code) if !code.is_empty() && code != "auto" => code,
        _ => system,
    };
    let code = wanted.split(['_', '.', '@', '-']).next().unwrap_or("").to_ascii_lowercase();
    LANGUAGES
        .iter()
        .map(|(each, _)| *each)
        .find(|each| *each == code)
        .unwrap_or("en")
}

pub fn set(code: &str) {
    let catalog = match code {
        "es" => Some(&catalogs()[0]),
        "de" => Some(&catalogs()[1]),
        "fr" => Some(&catalogs()[2]),
        _ => None,
    };
    if let Ok(mut now) = NOW.write() {
        *now = catalog;
    }
}

pub fn use_config(chosen: Option<&str>) -> &'static str {
    let code = pick(chosen, &from_env());
    set(code);
    code
}

pub fn t(english: &str) -> &str {
    let Ok(now) = NOW.read() else {
        return english;
    };
    match now.and_then(|catalog| catalog.get(english)) {
        Some(said) => said,
        None => english,
    }
}

pub fn tf(template: &str, args: &[&dyn Display]) -> String {
    let template = t(template);
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        match args.next() {
            Some(arg) => out.push_str(&arg.to_string()),
            None => out.push_str("{}"),
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalog_pairs_each_line_with_the_next_answer() {
        let catalog = parse("Wi-Fi\n= Wifi\n\nJoin\n= Unirse\n# note\nLost\n");
        assert_eq!(catalog.get("Join"), Some(&"Unirse"));
        assert_eq!(catalog.get("Wi-Fi"), Some(&"Wifi"));
        assert!(!catalog.contains_key("Lost"));
    }

    #[test]
    fn the_language_comes_from_the_config_or_the_system() {
        assert_eq!(pick(None, "de_DE.UTF-8"), "de");
        assert_eq!(pick(Some("auto"), "fr_CA.UTF-8"), "fr");
        assert_eq!(pick(Some("es"), "de_DE.UTF-8"), "es");
        assert_eq!(pick(None, "ja_JP.UTF-8"), "en");
        assert_eq!(pick(None, ""), "en");
    }

    #[test]
    fn placeholders_fill_in_order() {
        assert_eq!(tf("Joining {}… {} left", &[&"Home", &3]), "Joining Home… 3 left");
    }

    fn used(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                used(&path, out);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let text = text.split("\n#[cfg(test)]\nmod tests").next().unwrap_or("");
            for marker in ["lang::t(\"", "lang::tf(\""] {
                for piece in text.split(marker).skip(1) {
                    if let Some(end) = piece.find('"') {
                        out.push(piece[..end].to_string());
                    }
                }
            }
        }
    }

    #[test]
    fn every_string_in_the_code_has_a_translation() {
        const SAME: [&str; 6] = ["Wi-Fi", "Bluetooth", "Drifter", "2.4 GHz", "5 GHz", "6 GHz"];
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = Vec::new();
        used(&crates, &mut found);
        assert!(found.len() > 500, "the code was scanned");
        for (code, catalog) in [("es", parse(SPANISH)), ("de", parse(GERMAN)), ("fr", parse(FRENCH))] {
            let missing: Vec<&String> = found
                .iter()
                .filter(|each| !catalog.contains_key(each.as_str()) && !SAME.contains(&each.as_str()))
                .collect();
            assert!(missing.is_empty(), "{code} has no translation for {missing:?}");
        }
    }

    #[test]
    fn every_catalog_keeps_the_placeholders_of_its_english() {
        for (code, catalog) in [("es", parse(SPANISH)), ("de", parse(GERMAN)), ("fr", parse(FRENCH))] {
            for (from, said) in catalog {
                assert_eq!(
                    from.matches("{}").count(),
                    said.matches("{}").count(),
                    "{code}: {from:?} -> {said:?}"
                );
            }
        }
    }
}
