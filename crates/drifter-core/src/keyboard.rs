pub use crate::sheet::{Choice as Layout, matching};

pub const RULES: [&str; 2] = [
    "/usr/share/X11/xkb/rules/evdev.lst",
    "/usr/share/X11/xkb/rules/base.lst",
];

fn section<'a>(text: &'a str, wanted: &str) -> Vec<(&'a str, &'a str)> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in text.lines() {
        if let Some(section) = line.strip_prefix('!') {
            inside = section.trim() == wanted;
            continue;
        }
        if !inside {
            continue;
        }
        let line = line.trim();
        if let Some((code, name)) = line.split_once(char::is_whitespace) {
            found.push((code, name.trim()));
        }
    }
    found
}

pub fn parse(text: &str) -> Vec<Layout> {
    let mut found: Vec<Layout> = section(text, "layout")
        .into_iter()
        .map(|(code, name)| Layout::new(code, name))
        .collect();
    found.sort_by_key(|layout| layout.name.to_lowercase());
    found
}

pub fn parse_variants(text: &str, layout: &str) -> Vec<Layout> {
    let mut found: Vec<Layout> = section(text, "variant")
        .into_iter()
        .filter_map(|(code, rest)| {
            let (owner, name) = rest.split_once(':')?;
            (owner.trim() == layout).then(|| Layout::new(code, name.trim()))
        })
        .collect();
    found.sort_by_key(|variant| variant.name.to_lowercase());
    found.insert(0, Layout::new("", crate::lang::t("Plain")));
    found
}

fn rules() -> Option<String> {
    RULES
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
}

pub fn installed() -> Vec<Layout> {
    rules().map(|text| parse(&text)).unwrap_or_default()
}

pub fn variants(layout: &str) -> Vec<Layout> {
    let layout = match layout.split(',').next().unwrap_or("").trim() {
        "" => "us",
        code => code,
    };
    parse_variants(&rules().unwrap_or_default(), layout)
}

pub const CAPS: [(&str, &str); 3] = [
    ("", "Caps Lock"),
    ("caps:escape", "Escape"),
    ("ctrl:nocaps", "Ctrl"),
];
pub const COMPOSE: [(&str, &str); 3] = [
    ("", "None"),
    ("compose:ralt", "Right Alt"),
    ("compose:menu", "Menu"),
];

fn is_caps(option: &str) -> bool {
    option.starts_with("caps:") || matches!(option, "ctrl:nocaps" | "ctrl:swapcaps")
}

fn is_compose(option: &str) -> bool {
    option.starts_with("compose:")
}

fn split(options: &str) -> Vec<&str> {
    options
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .collect()
}

fn chosen(options: &str, set: &[(&str, &str); 3]) -> usize {
    let held = split(options);
    set.iter()
        .position(|(code, _)| !code.is_empty() && held.contains(code))
        .unwrap_or(0)
}

pub fn caps(options: &str) -> usize {
    chosen(options, &CAPS)
}

pub fn compose(options: &str) -> usize {
    chosen(options, &COMPOSE)
}

pub fn others(options: &str) -> String {
    split(options)
        .into_iter()
        .filter(|option| !is_caps(option) && !is_compose(option))
        .collect::<Vec<_>>()
        .join(",")
}

fn replace(options: &str, drop: fn(&str) -> bool, add: &str) -> String {
    let mut kept: Vec<&str> = split(options)
        .into_iter()
        .filter(|option| !drop(option))
        .collect();
    if !add.is_empty() {
        kept.push(add);
    }
    kept.join(",")
}

pub fn set_caps(options: &str, at: usize) -> String {
    replace(options, is_caps, CAPS[at.min(2)].0)
}

pub fn set_compose(options: &str, at: usize) -> String {
    replace(options, is_compose, COMPOSE[at.min(2)].0)
}

pub fn name_of(layouts: &[Layout], code: &str) -> String {
    if code.is_empty() {
        return crate::lang::t("System default").to_string();
    }
    layouts
        .iter()
        .find(|layout| layout.code == code)
        .map(|layout| layout.name.clone())
        .unwrap_or_else(|| code.to_string())
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
    fn variants_belong_to_their_layout_and_plain_comes_first() {
        let list = "! variant\n  dvorak  us: English (Dvorak)\n  intl  us: English (US, intl.)\n  nodeadkeys  de: German (no dead keys)\n";
        let codes: Vec<String> = parse_variants(list, "us")
            .into_iter()
            .map(|v| v.code)
            .collect();
        assert_eq!(codes, ["", "dvorak", "intl"]);
        assert_eq!(parse_variants(list, "de")[1].name, "German (no dead keys)");
    }

    #[test]
    fn caps_and_compose_options_swap_and_keep_the_rest() {
        let options = "grp:alt_shift_toggle,caps:escape";
        assert_eq!(caps(options), 1);
        assert_eq!(compose(options), 0);
        let options = set_caps(options, 2);
        assert_eq!(options, "grp:alt_shift_toggle,ctrl:nocaps");
        let options = set_compose(&options, 1);
        assert_eq!(options, "grp:alt_shift_toggle,ctrl:nocaps,compose:ralt");
        assert_eq!(others(&options), "grp:alt_shift_toggle");
        assert_eq!(
            set_caps(&set_compose(&options, 0), 0),
            "grp:alt_shift_toggle"
        );
    }

    #[test]
    fn a_name_falls_back_to_the_code() {
        let layouts = parse(LIST);
        assert_eq!(name_of(&layouts, "de"), "German");
        assert_eq!(name_of(&layouts, "xx"), "xx");
        assert_eq!(name_of(&layouts, ""), "System default");
    }
}
