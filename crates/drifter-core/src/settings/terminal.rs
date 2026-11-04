use super::{Control, Group, Id, Mark, Model, Row, drop_key, put};
use crate::sheet::Choice;
use crate::config::{TERM_OPACITY, TERM_PADDING, TERM_TEXT, TermColors, TermCursor, TermTabs};

fn at<T: PartialEq>(list: &[T], wanted: T) -> usize {
    list.iter().position(|each| *each == wanted).unwrap_or(0)
}

fn slider(least: u32, most: u32, value: u32, unit: &'static str) -> Control {
    Control::Slider {
        least: f64::from(least),
        most: f64::from(most),
        value: f64::from(value),
        unit,
        zero: None,
    }
}

impl Model {
    pub(super) fn terminal_groups(&self) -> Vec<Group> {
        let t = &self.terminal;
        vec![
            Group {
                header: Some(crate::lang::t("Look")),
                rows: vec![
                    Row::new(
                        Id::TermColors,
                        crate::lang::t("Colors"),
                        Control::Segments {
                            options: vec![crate::lang::t("Auto"), crate::lang::t("Dark"), crate::lang::t("Light")],
                            chosen: at(&TermColors::ALL, t.colors),
                        },
                    )
                    .detail(crate::lang::t("Auto follows your Light or Dark look")),
                    Row::new(
                        Id::TermFont,
                        crate::lang::t("Font"),
                        Control::Buttons(match t.font.is_some() {
                            true => vec![Mark::Text(crate::lang::t("Choose")), Mark::Text(crate::lang::t("Reset"))],
                            false => vec![Mark::Text(crate::lang::t("Choose"))],
                        }),
                    )
                    .detail(t.font.as_deref().unwrap_or(crate::lang::t("JetBrains Mono, built in"))),
                    Row::new(
                        Id::TermText,
                        crate::lang::t("Text size"),
                        slider(TERM_TEXT.0, TERM_TEXT.1, t.text_size, " px"),
                    )
                    .detail(crate::lang::t("Control + and Control - change it for one window")),
                    Row::new(
                        Id::TermCursor,
                        crate::lang::t("Cursor"),
                        Control::Segments {
                            options: vec![crate::lang::t("Block"), crate::lang::t("Bar"), crate::lang::t("Underline")],
                            chosen: at(&TermCursor::ALL, t.cursor),
                        },
                    ),
                    Row::new(
                        Id::TermOpacity,
                        crate::lang::t("Background opacity"),
                        slider(TERM_OPACITY.0, TERM_OPACITY.1, t.opacity, " %"),
                    ),
                    Row::new(
                        Id::TermPadding,
                        crate::lang::t("Space around the text"),
                        slider(TERM_PADDING.0, TERM_PADDING.1, t.padding, " px"),
                    ),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some(crate::lang::t("Tabs")),
                rows: vec![
                    Row::new(
                        Id::TermTabs,
                        crate::lang::t("Tab bar"),
                        Control::Segments {
                            options: vec![
                                crate::lang::t("Top"),
                                crate::lang::t("Bottom"),
                                crate::lang::t("Left"),
                                crate::lang::t("Right"),
                            ],
                            chosen: at(&TermTabs::ALL, t.tabs),
                        },
                    ),
                    Row::new(
                        Id::TermTabsAlways,
                        crate::lang::t("Always show the tab bar"),
                        Control::Switch(t.always_show_tabs),
                    )
                    .detail(crate::lang::t("Also when only one tab is open")),
                ],
                footer: Some(
                    crate::lang::t("Control T opens a tab, Control W closes it, Control Tab goes to the next one.").into(),
                ),
                thumbs: None,
            },
        ]
    }

    pub(super) fn write_terminal(&self, before: &Model, doc: &mut toml_edit::DocumentMut) {
        let (now, was) = (&self.terminal, &before.terminal);
        let int = |n: u32| toml_edit::Value::from(i64::from(n));
        if now.colors != was.colors {
            put(doc, "terminal", "colors", toml_edit::Value::from(now.colors.word()));
        }
        if now.font != was.font {
            match &now.font {
                Some(font) => put(doc, "terminal", "font", toml_edit::Value::from(font.as_str())),
                None => drop_key(doc, "terminal", "font"),
            }
        }
        if now.cursor != was.cursor {
            put(doc, "terminal", "cursor", toml_edit::Value::from(now.cursor.word()));
        }
        if now.tabs != was.tabs {
            put(doc, "terminal", "tabs", toml_edit::Value::from(now.tabs.word()));
        }
        if now.always_show_tabs != was.always_show_tabs {
            put(doc, "terminal", "always_show_tabs", toml_edit::Value::from(now.always_show_tabs));
        }
        if now.text_size != was.text_size {
            put(doc, "terminal", "text_size", int(now.text_size));
        }
        if now.opacity != was.opacity {
            put(doc, "terminal", "opacity", int(now.opacity));
        }
        if now.padding != was.padding {
            put(doc, "terminal", "padding", int(now.padding));
        }
    }
}

pub const BUILT_IN: &str = "JetBrains Mono";

pub fn mono_fonts() -> Vec<Choice> {
    let Ok(out) = std::process::Command::new("fc-list").args([":spacing=mono", "family", "color"]).output() else {
        return Vec::new();
    };
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.contains(":color=True"))
        .filter_map(|line| line.split(':').next()?.split(',').next().map(|name| name.trim().replace('\\', "")))
        .filter(|name| !name.is_empty())
        .collect();
    names.retain(|name| name != BUILT_IN);
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    let mut out = vec![Choice::new(BUILT_IN, crate::lang::t("JetBrains Mono, built in"))];
    out.extend(names.into_iter().map(|name| Choice::new(&name, &name)));
    out
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, TermColors, TermCursor, TermTabs};
    use crate::settings::{Id, Model, Section, Value};

    #[test]
    fn terminal_page_writes_what_it_shows() {
        let config = Config::parse("").unwrap();
        let before = Model::read(&config, &[], |_| None);
        assert_eq!(before.groups(Section::Terminal).len(), 2);
        let mut after = before.clone();
        after.set(Id::TermColors, Value::Choice(2));
        after.set(Id::TermCursor, Value::Choice(1));
        after.set(Id::TermTabs, Value::Choice(3));
        after.set(Id::TermTabsAlways, Value::Switch(true));
        after.set(Id::TermText, Value::Number(99.0));
        after.set(Id::TermOpacity, Value::Number(85.0));
        after.set(Id::TermPadding, Value::Number(4.0));
        assert_eq!(after.press(Id::TermFont, 0), Some(crate::settings::Ask::Choose(crate::sheet::For::TermFont)));
        after.choose(crate::sheet::For::TermFont, "Source Code Pro".into());
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap().terminal;
        assert_eq!(back.colors, TermColors::Light);
        assert_eq!(back.cursor, TermCursor::Bar);
        assert_eq!(back.tabs, TermTabs::Right);
        assert!(back.always_show_tabs);
        assert_eq!(back.text_size, 32);
        assert_eq!(back.opacity, 85);
        assert_eq!(back.padding, 4);
        assert_eq!(back.font.as_deref(), Some("Source Code Pro"));
        let mut plain = after.clone();
        plain.choose(crate::sheet::For::TermFont, super::BUILT_IN.into());
        assert_eq!(plain.terminal.font, None);
    }
}
