use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::menu;
use crate::text::Font;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Slide { least: f64, most: f64, step: f64 },
    Pick { options: Vec<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub table: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub value: f64,
}

impl Field {
    pub fn reading(&self) -> String {
        match &self.kind {
            Kind::Slide { .. } => format!("{}", self.value.round() as i64),
            Kind::Pick { options } => options
                .get(self.value.round().max(0.0) as usize)
                .cloned()
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub row: f64,
    pub gap: f64,
    pub preview: Vec2,
    pub radius: f64,
    pub label_width: f64,
    pub knob: f64,
    pub text: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub back: [u8; 3],
    pub back_alpha: u8,
    pub track: [u8; 3],
    pub live: [u8; 3],
}

impl Default for Look {
    fn default() -> Self {
        Look {
            width: 540.0,
            pad: 18.0,
            row: 34.0,
            gap: 8.0,
            preview: Vec2::new(288.0, 162.0),
            radius: 12.0,
            label_width: 150.0,
            knob: 14.0,
            text: 13.0,
            ink: [236, 238, 244],
            dim: [140, 146, 158],
            back: [28, 31, 40],
            back_alpha: 250,
            track: [62, 68, 86],
            live: [104, 170, 255],
        }
    }
}

impl Look {
    pub fn size(&self, fields: &[Field]) -> Vec2 {
        let rows = fields.len() as f64 * (self.row + self.gap);
        Vec2::new(
            self.width,
            self.pad * 3.0 + self.preview.y + rows + self.row,
        )
    }

    pub fn preview_rect(&self, size: Vec2) -> Rect {
        Rect::new(
            Vec2::new((size.x - self.preview.x) / 2.0, self.pad),
            self.preview,
        )
    }

    pub fn row_rect(&self, size: Vec2, index: usize) -> Rect {
        let top = self.pad * 2.0 + self.preview.y + index as f64 * (self.row + self.gap);
        Rect::new(
            Vec2::new(self.pad, top),
            Vec2::new(size.x - self.pad * 2.0, self.row),
        )
    }

    pub fn control_rect(&self, size: Vec2, index: usize) -> Rect {
        let row = self.row_rect(size, index);
        Rect::new(
            Vec2::new(row.origin.x + self.label_width, row.origin.y),
            Vec2::new((row.size.x - self.label_width).max(0.0), row.size.y),
        )
    }

    pub fn track_rect(&self, size: Vec2, index: usize) -> Rect {
        let area = self.control_rect(size, index);
        let tall = 4.0;
        Rect::new(
            Vec2::new(
                area.origin.x + self.knob / 2.0,
                area.center().y - tall / 2.0,
            ),
            Vec2::new((area.size.x - 64.0 - self.knob).max(10.0), tall),
        )
    }

    pub fn knob_rect(&self, size: Vec2, index: usize, field: &Field) -> Rect {
        let track = self.track_rect(size, index);
        let Kind::Slide { least, most, .. } = field.kind else {
            return Rect::new(track.origin, Vec2::ZERO);
        };
        let span = (most - least).max(1e-9);
        let along = ((field.value - least) / span).clamp(0.0, 1.0);
        Rect::new(
            Vec2::new(
                track.origin.x + track.size.x * along - self.knob / 2.0,
                track.center().y - self.knob / 2.0,
            ),
            Vec2::splat(self.knob),
        )
    }

    pub fn option_rect(&self, size: Vec2, index: usize, slot: usize, count: usize) -> Rect {
        let area = self.control_rect(size, index);
        let each = area.size.x / count.max(1) as f64;
        Rect::new(
            Vec2::new(area.origin.x + each * slot as f64, area.origin.y),
            Vec2::new(each, area.size.y),
        )
    }

    pub fn value_at(&self, size: Vec2, index: usize, field: &Field, inside: Vec2) -> Option<f64> {
        let row = self.row_rect(size, index);
        if inside.y < row.origin.y || inside.y >= row.max().y {
            return None;
        }

        match &field.kind {
            Kind::Slide { least, most, step } => {
                let track = self.track_rect(size, index);
                let along = ((inside.x - track.origin.x) / track.size.x).clamp(0.0, 1.0);
                let raw = least + (most - least) * along;
                let stepped = match *step > 0.0 {
                    true => (raw / step).round() * step,
                    false => raw,
                };
                Some(stepped.clamp(*least, *most))
            }
            Kind::Pick { options } => {
                let count = options.len();
                (0..count)
                    .find(|slot| self.option_rect(size, index, *slot, count).contains(inside))
                    .map(|slot| slot as f64)
            }
        }
    }

    pub fn row_at(&self, size: Vec2, fields: &[Field], inside: Vec2) -> Option<usize> {
        (0..fields.len()).find(|index| self.row_rect(size, *index).contains(inside))
    }

    pub fn paint(
        &self,
        fields: &[Field],
        size: Vec2,
        wallpaper: Option<&Image>,
        font: Option<&Font>,
    ) -> Image {
        let width = size.x.round().max(1.0) as u32;
        let height = size.y.round().max(1.0) as u32;
        let mut image = Image {
            width,
            height,
            rgba: vec![0u8; (width as usize) * (height as usize) * 4],
        };

        let alpha = f64::from(self.back_alpha) / 255.0;
        for y in 0..height {
            let py = f64::from(y) + 0.5;
            for x in 0..width {
                let px = f64::from(x) + 0.5;
                let coverage = (menu::corner(px, py, size, self.radius) + 0.5).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let here = alpha * coverage;
                let index = ((y as usize) * (width as usize) + x as usize) * 4;
                for (channel, value) in self.back.iter().enumerate() {
                    image.rgba[index + channel] =
                        (f64::from(*value) * here).round().clamp(0.0, 255.0) as u8;
                }
                image.rgba[index + 3] = (here * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }

        let preview = self.preview_rect(size);
        if let Some(art) = wallpaper {
            let shown = art.scaled(preview.size.x as u32, preview.size.y as u32);
            menu::stamp(&mut image, &shown, preview.origin);
        } else {
            menu::fill(&mut image, preview, self.track);
        }

        for (index, field) in fields.iter().enumerate() {
            let row = self.row_rect(size, index);
            if let Some(font) = font {
                let label = font.paint(field.label, self.ink);
                let at = Vec2::new(row.origin.x, row.center().y - f64::from(label.height) / 2.0);
                menu::stamp(&mut image, &label, at);
            }

            match &field.kind {
                Kind::Slide { .. } => {
                    let track = self.track_rect(size, index);
                    menu::fill(&mut image, track, self.track);
                    let knob = self.knob_rect(size, index, field);
                    let filled = Rect::new(
                        track.origin,
                        Vec2::new((knob.center().x - track.origin.x).max(0.0), track.size.y),
                    );
                    menu::fill(&mut image, filled, self.live);
                    menu::fill(&mut image, knob, self.ink);

                    if let Some(font) = font {
                        let reading = font.paint(&field.reading(), self.ink);
                        let at = Vec2::new(
                            track.max().x + 14.0,
                            row.center().y - f64::from(reading.height) / 2.0,
                        );
                        menu::stamp(&mut image, &reading, at);
                    }
                }
                Kind::Pick { options } => {
                    let count = options.len();
                    for (slot, name) in options.iter().enumerate() {
                        let cell = self.option_rect(size, index, slot, count);
                        let chosen = field.value.round() as usize == slot;
                        let dot = Rect::new(
                            Vec2::new(cell.origin.x, cell.center().y - 5.0),
                            Vec2::splat(10.0),
                        );
                        menu::fill(&mut image, dot, if chosen { self.live } else { self.track });
                        if let Some(font) = font {
                            let text = font.paint(name, if chosen { self.ink } else { self.dim });
                            let at = Vec2::new(
                                dot.max().x + 8.0,
                                cell.center().y - f64::from(text.height) / 2.0,
                            );
                            menu::stamp(&mut image, &text, at);
                        }
                    }
                }
            }
        }

        image
    }
}

pub fn write_back(text: &str, fields: &[Field]) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;

    for field in fields {
        let table = doc
            .entry(field.table)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        let Some(table) = table.as_table_like_mut() else {
            continue;
        };
        let fresh = match &field.kind {
            Kind::Slide { .. } => toml_edit::value(field.value.round() as i64),
            Kind::Pick { options } => {
                let Some(name) = options.get(field.value.round().max(0.0) as usize) else {
                    continue;
                };
                toml_edit::value(name.as_str())
            }
        };

        match table.get_mut(field.key) {
            Some(held) => {
                let kept = held.as_value().map(|value| value.decor().clone());
                *held = fresh;
                if let (Some(kept), Some(value)) = (kept, held.as_value_mut()) {
                    *value.decor_mut() = kept;
                }
            }
            None => {
                table.insert(field.key, fresh);
            }
        }
    }

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Vec<Field> {
        vec![
            Field {
                table: "workspace",
                key: "width",
                label: "World width",
                kind: Kind::Slide {
                    least: 1.0,
                    most: 8.0,
                    step: 1.0,
                },
                value: 3.0,
            },
            Field {
                table: "workspace",
                key: "wallpaper_mode",
                label: "Wallpaper",
                kind: Kind::Pick {
                    options: vec!["fixed".into(), "pan".into(), "parallax".into()],
                },
                value: 0.0,
            },
        ]
    }

    const SAMPLE: &str =
        "# keep me\n[workspace]\n# and me\nwidth = 3\nheight = 3\nwallpaper_mode = \"fixed\"\n";

    #[test]
    fn a_slider_reads_as_a_whole_number() {
        let field = &fields()[0];
        assert_eq!(field.reading(), "3");
    }

    #[test]
    fn a_pick_reads_as_its_chosen_name() {
        let mut field = fields()[1].clone();
        field.value = 2.0;
        assert_eq!(field.reading(), "parallax");
    }

    #[test]
    fn the_rows_stack_without_overlapping() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let first = look.row_rect(size, 0);
        let second = look.row_rect(size, 1);
        assert!(second.origin.y >= first.max().y);
    }

    #[test]
    fn every_row_sits_inside_the_sheet() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        for index in 0..fields.len() {
            let row = look.row_rect(size, index);
            assert!(row.origin.x >= 0.0 && row.max().x <= size.x);
            assert!(row.max().y <= size.y);
        }
    }

    #[test]
    fn the_preview_sits_at_the_top_and_is_centered() {
        let look = Look::default();
        let size = look.size(&fields());
        let preview = look.preview_rect(size);
        assert_eq!(preview.center().x, size.x / 2.0);
        assert_eq!(preview.origin.y, look.pad);
        assert!(preview.max().y < look.row_rect(size, 0).origin.y);
    }

    #[test]
    fn the_knob_walks_the_track_with_the_value() {
        let look = Look::default();
        let mut fields = fields();
        let size = look.size(&fields);
        let track = look.track_rect(size, 0);

        fields[0].value = 1.0;
        let low = look.knob_rect(size, 0, &fields[0]);
        fields[0].value = 8.0;
        let high = look.knob_rect(size, 0, &fields[0]);

        assert!((low.center().x - track.origin.x).abs() < 1.0);
        assert!((high.center().x - track.max().x).abs() < 1.0);
    }

    #[test]
    fn clicking_a_track_gives_the_value_under_the_pointer() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let track = look.track_rect(size, 0);
        let middle = Vec2::new(track.center().x, look.row_rect(size, 0).center().y);
        let value = look.value_at(size, 0, &fields[0], middle).unwrap();
        assert!((value - 4.0).abs() <= 1.0, "got {value}");
    }

    #[test]
    fn a_slider_never_leaves_its_range() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let row = look.row_rect(size, 0);
        let far_left = look.value_at(size, 0, &fields[0], Vec2::new(-500.0, row.center().y));
        let far_right = look.value_at(size, 0, &fields[0], Vec2::new(5000.0, row.center().y));
        assert_eq!(far_left, Some(1.0));
        assert_eq!(far_right, Some(8.0));
    }

    #[test]
    fn a_slider_lands_on_whole_steps() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let row = look.row_rect(size, 0);
        for x in 0..40 {
            let at = Vec2::new(row.origin.x + x as f64 * 10.0, row.center().y);
            if let Some(value) = look.value_at(size, 0, &fields[0], at) {
                assert_eq!(value, value.round(), "the step was ignored");
            }
        }
    }

    #[test]
    fn clicking_an_option_picks_that_option() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let third = look.option_rect(size, 1, 2, 3);
        let value = look.value_at(size, 1, &fields[1], third.center());
        assert_eq!(value, Some(2.0));
    }

    #[test]
    fn a_click_outside_every_row_changes_nothing() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let preview = look.preview_rect(size);
        assert_eq!(look.row_at(size, &fields, preview.center()), None);
        assert_eq!(look.value_at(size, 0, &fields[0], preview.center()), None);
    }

    #[test]
    fn saving_keeps_the_comments() {
        let out = write_back(SAMPLE, &fields()).unwrap();
        assert!(out.contains("# keep me"));
        assert!(out.contains("# and me"));
    }

    #[test]
    fn saving_writes_the_new_values() {
        let mut fields = fields();
        fields[0].value = 6.0;
        fields[1].value = 2.0;
        let out = write_back(SAMPLE, &fields).unwrap();
        assert!(out.contains("width = 6"), "{out}");
        assert!(out.contains("wallpaper_mode = \"parallax\""), "{out}");
        assert!(out.contains("height = 3"), "an untouched key was lost");
    }

    #[test]
    fn saving_makes_a_table_that_is_not_there_yet() {
        let fields = vec![Field {
            table: "tray",
            key: "size",
            label: "Clock size",
            kind: Kind::Slide {
                least: 8.0,
                most: 32.0,
                step: 1.0,
            },
            value: 15.0,
        }];
        let out = write_back(SAMPLE, &fields).unwrap();
        assert!(out.contains("[tray]"), "{out}");
        assert!(out.contains("size = 15"), "{out}");
    }

    #[test]
    fn saving_refuses_a_config_it_cannot_read() {
        assert!(write_back("this is not toml = = =", &fields()).is_err());
    }

    #[test]
    fn the_sheet_paints_at_the_size_it_asked_for() {
        let look = Look::default();
        let fields = fields();
        let size = look.size(&fields);
        let image = look.paint(&fields, size, None, None);
        assert_eq!(image.width, size.x as u32);
        assert_eq!(image.height, size.y as u32);
        let corner = image.rgba[3];
        assert_eq!(corner, 0, "the sheet has square corners");
    }
}
