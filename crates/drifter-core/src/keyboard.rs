#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub code: String,
    pub name: String,
}

pub const RULES: [&str; 2] = [
    "/usr/share/X11/xkb/rules/evdev.lst",
    "/usr/share/X11/xkb/rules/base.lst",
];

pub fn parse(text: &str) -> Vec<Layout> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in text.lines() {
        if let Some(section) = line.strip_prefix('!') {
            inside = section.trim() == "layout";
            continue;
        }
        if !inside {
            continue;
        }
        let line = line.trim();
        let Some((code, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        found.push(Layout {
            code: code.to_string(),
            name: name.trim().to_string(),
        });
    }
    found.sort_by_key(|layout| layout.name.to_lowercase());
    found
}

pub fn installed() -> Vec<Layout> {
    RULES
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

pub fn name_of(layouts: &[Layout], code: &str) -> String {
    if code.is_empty() {
        return "System default".to_string();
    }
    layouts
        .iter()
        .find(|layout| layout.code == code)
        .map(|layout| layout.name.clone())
        .unwrap_or_else(|| code.to_string())
}

pub fn matching<'a>(layouts: &'a [Layout], text: &str) -> Vec<&'a Layout> {
    let wanted = text.trim().to_lowercase();
    layouts
        .iter()
        .filter(|layout| {
            wanted.is_empty()
                || layout.name.to_lowercase().contains(&wanted)
                || layout.code.to_lowercase() == wanted
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "! model\n  pc105  Generic 105-key PC\n\n! layout\n  us              English (US)\n  de              German\n  gb              English (UK)\n\n! variant\n  dvorak          us: English (Dvorak)\n";

    #[test]
    fn only_the_layout_section_is_read_and_it_is_sorted_by_name() {
        let layouts = parse(LIST);
        let codes: Vec<&str> = layouts.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["gb", "us", "de"]);
        assert_eq!(layouts[1].name, "English (US)");
    }

    #[test]
    fn a_search_matches_names_and_whole_codes() {
        let layouts = parse(LIST);
        assert_eq!(matching(&layouts, "english").len(), 2);
        assert_eq!(matching(&layouts, "DE")[0].code, "de");
        assert_eq!(matching(&layouts, "zz").len(), 0);
        assert_eq!(matching(&layouts, "  ").len(), 3);
    }

    #[test]
    fn a_name_falls_back_to_the_code() {
        let layouts = parse(LIST);
        assert_eq!(name_of(&layouts, "de"), "German");
        assert_eq!(name_of(&layouts, "xx"), "xx");
        assert_eq!(name_of(&layouts, ""), "System default");
    }
}
