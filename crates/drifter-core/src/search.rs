use std::path::{Path, PathBuf};

use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const MOST: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    Setting,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub kind: Kind,
    pub title: String,
    pub detail: String,
    pub key: String,
    pub extra: String,
    pub icon: Option<String>,
}

pub fn apps(installed: &[crate::desktop::App]) -> Vec<Found> {
    installed
        .iter()
        .map(|app| Found {
            kind: Kind::App,
            title: app.name.clone(),
            detail: "Application".to_string(),
            key: app.id.clone(),
            extra: format!("{} {}", app.id, app.words),
            icon: app.icon.clone(),
        })
        .collect()
}

pub fn settings() -> Vec<Found> {
    crate::settings::Section::ALL
        .into_iter()
        .map(|section| Found {
            kind: Kind::Setting,
            title: section.name().to_string(),
            detail: "Settings".to_string(),
            key: section.name().to_string(),
            extra: section.words().to_string(),
            icon: None,
        })
        .collect()
}

pub fn files(home: &Path) -> Vec<Found> {
    let roots = ["Desktop", "Documents", "Downloads", "Pictures", "Music", "Videos"];
    let mut found = Vec::new();
    for root in roots {
        walk(&home.join(root), 3, &mut found);
    }
    found
        .into_iter()
        .map(|path| Found {
            kind: Kind::File,
            title: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            detail: path
                .parent()
                .map(|parent| shorten(parent, home))
                .unwrap_or_default(),
            key: path.to_string_lossy().into_owned(),
            extra: String::new(),
            icon: None,
        })
        .collect()
}

fn shorten(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn walk(folder: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if out.len() >= 5000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        out.push(path.clone());
        if is_dir && depth > 0 {
            walk(&path, depth - 1, out);
        }
    }
}

fn score(found: &Found, query: &str) -> Option<u32> {
    let title = found.title.to_lowercase();
    let kind = match found.kind {
        Kind::App => 30,
        Kind::Setting => 20,
        Kind::File => 0,
    };
    let base = if title == query {
        400
    } else if title.starts_with(query) {
        300
    } else if title
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(query))
    {
        200
    } else if title.contains(query) {
        120
    } else if found
        .extra
        .to_lowercase()
        .split_whitespace()
        .any(|word| word.starts_with(query))
    {
        80
    } else {
        return None;
    };
    Some(base + kind)
}

pub fn rank(query: &str, all: &[Found]) -> Vec<Found> {
    ranked(query, all, MOST)
}

pub fn rank_all(query: &str, all: &[Found]) -> Vec<Found> {
    ranked(query, all, usize::MAX)
}

fn ranked(query: &str, all: &[Found], most: usize) -> Vec<Found> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u32, &Found)> = all
        .iter()
        .filter_map(|found| score(found, &query).map(|points| (points, found)))
        .collect();
    hits.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.title.len().cmp(&b.1.title.len()))
            .then(a.1.title.cmp(&b.1.title))
    });
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Found> = Vec::new();
    for (_, found) in hits {
        if out.len() >= most {
            break;
        }
        if found.kind != Kind::File && !seen.insert((found.kind, found.title.as_str())) {
            continue;
        }
        out.push(found.clone());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub field: f64,
    pub row: f64,
    pub radius: f64,
    pub pad: f64,
    pub icon: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub rule: [u8; 3],
    pub accent: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 640.0,
            field: 56.0,
            row: 44.0,
            radius: 16.0,
            pad: 8.0,
            icon: 30.0,
            back: p.back,
            back_alpha: 0.96,
            ink: p.ink,
            dim: p.dim,
            rule: p.rule,
            accent,
        }
    }

    pub fn size(&self, results: usize) -> Vec2 {
        let list = match results {
            0 => 0.0,
            n => self.pad * 2.0 + self.row * n as f64,
        };
        Vec2::new(self.width, self.field + list)
    }

    pub fn rect(&self, screen: Vec2, results: usize) -> Rect {
        let size = self.size(results);
        let x = ((screen.x - size.x) / 2.0).round();
        let y = (screen.y * 0.22).round();
        Rect::new(Vec2::new(x, y), size)
    }

    pub fn row_rect(&self, index: usize) -> Rect {
        Rect::new(
            Vec2::new(self.pad, self.field + self.pad + self.row * index as f64),
            Vec2::new(self.width - self.pad * 2.0, self.row),
        )
    }

    pub fn row_at(&self, inside: Vec2, results: usize) -> Option<usize> {
        (0..results).find(|at| self.row_rect(*at).contains(inside))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        query: &str,
        results: &[Found],
        icons: &[Option<Image>],
        selected: usize,
        big: &Font,
        small: &Font,
    ) -> Image {
        let size = self.size(results.len());
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, self.back_alpha);

        let glass = Vec2::new(self.pad * 2.5 + 9.0, self.field / 2.0 - 2.0);
        canvas.circle(glass, 8.0, self.dim, 1.0);
        canvas.circle(glass, 6.0, self.back, 1.0);
        for step in 0..8 {
            let t = f64::from(step) * 0.8;
            canvas.circle(glass + Vec2::new(6.5 + t, 6.5 + t), 1.6, self.dim, 1.0);
        }
        let (text, ink) = match query.is_empty() {
            true => ("Search apps, settings and files", self.dim),
            false => (query, self.ink),
        };
        let words = big.paint(text, ink);
        let left = self.pad * 2.5 + 30.0;
        let at = Vec2::new(left, (self.field - crate::paint::logical(&words).y) / 2.0);
        canvas.stamp(&words, at, 1.0);
        if !query.is_empty() {
            let caret = Rect::new(
                Vec2::new(at.x + crate::paint::logical(&words).x + 2.0, self.field / 2.0 - 12.0),
                Vec2::new(2.0, 24.0),
            );
            canvas.round_rect(caret, 1.0, self.accent, 1.0);
        }

        if results.is_empty() {
            return canvas.image;
        }
        canvas.round_rect(
            Rect::new(Vec2::new(0.0, self.field), Vec2::new(size.x, 1.0)),
            0.0,
            self.rule,
            1.0,
        );
        for (index, found) in results.iter().enumerate() {
            let row = self.row_rect(index);
            let lit = index == selected;
            if lit {
                canvas.round_rect(row, 8.0, self.accent, 1.0);
            }
            let ink = if lit { [255, 255, 255] } else { self.ink };
            let dim = if lit { [235, 240, 255] } else { self.dim };
            let spot = Rect::new(
                Vec2::new(row.origin.x + 8.0, row.center().y - self.icon / 2.0),
                Vec2::splat(self.icon),
            );
            match icons.get(index).and_then(|icon| icon.as_ref()) {
                Some(image) => canvas.stamp(image, spot.origin, 1.0),
                None => glyph(&mut canvas, spot, found.kind, ink),
            }
            let title = small.paint(&found.title, ink);
            let detail = small.paint(&found.detail, dim);
            let x = spot.max().x + 12.0;
            canvas.stamp(&title, Vec2::new(x, row.center().y - crate::paint::logical(&title).y / 2.0), 1.0);
            let right = row.max().x - 12.0 - crate::paint::logical(&detail).x;
            if right > x + crate::paint::logical(&title).x + 16.0 {
                canvas.stamp(&detail, Vec2::new(right, row.center().y - crate::paint::logical(&detail).y / 2.0), 1.0);
            }
        }
        canvas.image
    }
}

fn glyph(canvas: &mut Canvas, spot: Rect, kind: Kind, ink: [u8; 3]) {
    match kind {
        Kind::Setting => {
            let cog = crate::dock::paint_cog(spot.size.x as u32, ink);
            canvas.stamp(&cog, spot.origin, 1.0);
        }
        Kind::File | Kind::App => {
            let page = Rect::new(
                spot.origin + Vec2::new(spot.size.x * 0.2, spot.size.y * 0.08),
                Vec2::new(spot.size.x * 0.6, spot.size.y * 0.84),
            );
            canvas.round_rect(page, 3.0, ink, 0.9);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: Kind, title: &str, extra: &str) -> Found {
        Found {
            kind,
            title: title.into(),
            detail: String::new(),
            key: title.into(),
            extra: extra.into(),
            icon: None,
        }
    }

    #[test]
    fn search_at_density_two_matches_density_one() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let big = crate::text::Font::find(None, 20.0).unwrap();
        let small = crate::text::Font::find(None, 13.0).unwrap();
        let results = vec![item(Kind::App, "Terminal", ""), item(Kind::Setting, "Displays", "")];
        let icons = vec![None, None];
        crate::paint::same_at_two("search", || look.paint("ter", &results, &icons, 0, &big, &small));
    }

    #[test]
    fn a_name_that_starts_with_the_query_wins() {
        let all = vec![
            item(Kind::File, "firefox-notes.txt", ""),
            item(Kind::App, "Firefox", ""),
            item(Kind::App, "Thunderbird", "mail"),
            item(Kind::Setting, "Wi-Fi", "wifi wireless network"),
        ];
        let hits = rank("fire", &all);
        assert_eq!(hits[0].title, "Firefox", "apps first among equals");
        assert_eq!(hits[1].title, "firefox-notes.txt");
        assert_eq!(hits.len(), 2);
        assert_eq!(rank("wireless", &all)[0].title, "Wi-Fi", "a setting is found by its words");
        assert!(rank("   ", &all).is_empty());
        assert!(rank("zzz", &all).is_empty());
    }

    #[test]
    fn apps_are_found_by_their_generic_name_and_keywords() {
        let foot = crate::desktop::read(
            "foot",
            "[Desktop Entry]\nType=Application\nName=Foot\nGenericName=Terminal\nKeywords=shell;prompt;command;commandline;\nCategories=System;TerminalEmulator;\n",
        )
        .unwrap();
        let files = crate::desktop::read(
            "pcmanfm",
            "[Desktop Entry]\nType=Application\nName=PCManFM\nGenericName=File Manager\nKeywords=folder;manager;explore;disk;filesystem\nCategories=System;FileTools;FileManager;\n",
        )
        .unwrap();
        let mut all = apps(&[foot, files]);
        all.extend(settings());
        assert_eq!(rank("terminal", &all)[0].title, "Foot");
        assert_eq!(rank("folder", &all)[0].title, "PCManFM");
        assert_eq!(rank("shell", &all)[0].title, "Foot");
    }

    #[test]
    fn at_most_eight_results_come_back() {
        let all: Vec<Found> = (0..20).map(|n| item(Kind::File, &format!("note{n}"), "")).collect();
        assert_eq!(rank("note", &all).len(), MOST);
    }

    #[test]
    fn every_settings_page_can_be_found() {
        let pages = settings();
        assert_eq!(pages.len(), crate::settings::Section::ALL.len());
        assert_eq!(rank("blue", &pages)[0].title, "Bluetooth");
    }

    #[test]
    fn files_are_found_under_the_home_folders_but_not_hidden_ones() {
        let home = std::env::temp_dir().join(format!("drifter-search-{}", std::process::id()));
        std::fs::create_dir_all(home.join("Documents/work")).unwrap();
        std::fs::create_dir_all(home.join("Documents/.secret")).unwrap();
        std::fs::write(home.join("Documents/work/plan.txt"), b"x").unwrap();
        std::fs::write(home.join("Documents/.secret/key.txt"), b"x").unwrap();
        let all = files(&home);
        let plan = rank("plan", &all);
        assert_eq!(plan[0].detail, "~/Documents/work");
        assert!(rank("key", &all).is_empty());
    }

    #[test]
    fn rows_are_found_by_where_the_pointer_is() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let second = look.row_rect(1).center();
        assert_eq!(look.row_at(second, 3), Some(1));
        assert_eq!(look.row_at(Vec2::new(5.0, 5.0), 3), None, "the field is not a row");
        assert_eq!(look.size(0).y, look.field);
    }
}
