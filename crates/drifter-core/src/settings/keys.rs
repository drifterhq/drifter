use crate::geom::{Rect, Vec2};

use super::{Control, Group, Hit, Id, Look, Model, Row, Section, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spot {
    Sidebar,
    Row(Id, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Next,
    Back,
    Up,
    Down,
    Left,
    Right,
    Go,
}

pub enum Move {
    Stay,
    Focus(Spot),
    Section(Section),
    Act(Hit),
}

fn stops(groups: &[Group]) -> Vec<Row> {
    groups
        .iter()
        .flat_map(|group| group.rows.iter())
        .filter(|row| row.enabled)
        .filter(|row| {
            row.opens
                || match &row.control {
                    Control::Note(_) => false,
                    Control::Buttons(marks) => !marks.is_empty(),
                    _ => true,
                }
        })
        .cloned()
        .collect()
}

fn parts(row: &Row) -> usize {
    match &row.control {
        Control::Buttons(marks) => marks.len().max(1),
        Control::Swatches { colors, .. } => colors.len().max(1),
        _ => 1,
    }
}

impl Model {
    pub fn key_move(&self, section: Section, spot: Option<Spot>, press: Press) -> Move {
        let rows = stops(&self.groups(section));
        let shown = self.shown();
        let place = shown.iter().position(|each| *each == section);
        let spot = match spot {
            Some(Spot::Row(id, part)) if rows.iter().any(|row| row.id == id) => Spot::Row(id, part),
            Some(Spot::Row(..)) | None => match press {
                Press::Next | Press::Down | Press::Back | Press::Up => {
                    return Move::Focus(Spot::Sidebar);
                }
                _ => Spot::Sidebar,
            },
            Some(Spot::Sidebar) => Spot::Sidebar,
        };
        let first = || rows.first().map(|row| Spot::Row(row.id, 0)).unwrap_or(Spot::Sidebar);
        let last = || rows.last().map(|row| Spot::Row(row.id, 0)).unwrap_or(Spot::Sidebar);
        match spot {
            Spot::Sidebar => match press {
                Press::Next | Press::Right | Press::Go => Move::Focus(first()),
                Press::Back => Move::Focus(last()),
                Press::Up => place
                    .and_then(|at| at.checked_sub(1))
                    .and_then(|at| shown.get(at))
                    .map_or(Move::Stay, |each| Move::Section(*each)),
                Press::Down => place
                    .map(|at| at + 1)
                    .and_then(|at| shown.get(at))
                    .map_or(Move::Stay, |each| Move::Section(*each)),
                Press::Left => Move::Stay,
            },
            Spot::Row(id, part) => {
                let at = rows.iter().position(|row| row.id == id).unwrap_or(0);
                let row = &rows[at];
                let neighbour = |step: isize| -> Spot {
                    let to = at as isize + step;
                    match usize::try_from(to).ok().and_then(|to| rows.get(to)) {
                        Some(row) => Spot::Row(row.id, 0),
                        None => Spot::Sidebar,
                    }
                };
                match press {
                    Press::Next | Press::Down => Move::Focus(neighbour(1)),
                    Press::Back | Press::Up => Move::Focus(neighbour(-1)),
                    Press::Go => match &row.control {
                        Control::Switch(on) => Move::Act(Hit::Change(id, Value::Switch(!on))),
                        Control::Buttons(marks) if !marks.is_empty() => {
                            Move::Act(Hit::Press(id, part.min(marks.len() - 1)))
                        }
                        Control::Swatches { .. } => Move::Act(Hit::Change(id, Value::Choice(part))),
                        _ if row.opens => Move::Act(Hit::Open(id)),
                        _ => Move::Stay,
                    },
                    Press::Left | Press::Right => {
                        let way: isize = if press == Press::Right { 1 } else { -1 };
                        match &row.control {
                            Control::Slider { .. } => self
                                .nudge(section, id, way as f64)
                                .map_or(Move::Stay, |value| Move::Act(Hit::Change(id, value))),
                            Control::Segments { options, chosen } => {
                                let to = (*chosen as isize + way).clamp(0, options.len() as isize - 1) as usize;
                                match to == *chosen {
                                    true => Move::Stay,
                                    false => Move::Act(Hit::Change(id, Value::Choice(to))),
                                }
                            }
                            Control::Switch(on) if (*on) != (way > 0) => {
                                Move::Act(Hit::Change(id, Value::Switch(way > 0)))
                            }
                            _ if parts(row) > 1 => {
                                let to = (part as isize + way).clamp(0, parts(row) as isize - 1) as usize;
                                Move::Focus(Spot::Row(id, to))
                            }
                            _ => Move::Stay,
                        }
                    }
                }
            }
        }
    }

    pub fn spoken(&self, section: Section, spot: Spot) -> Option<Row> {
        match spot {
            Spot::Sidebar => None,
            Spot::Row(id, _) => self.groups(section).into_iter().flat_map(|group| group.rows).find(|row| row.id == id),
        }
    }
}

impl Look {
    pub fn spot_rect(&self, size: Vec2, section: Section, model: &Model, scroll: f64, spot: Spot) -> Option<Rect> {
        match spot {
            Spot::Sidebar => {
                let shown = model.shown();
                let at = shown.iter().position(|each| *each == section)?;
                Some(self.item_rect_in(&shown, at))
            }
            Spot::Row(id, part) => {
                let groups = model.groups(section);
                let placed = self.place(size, section, &groups, scroll);
                for (group, spots) in groups.iter().zip(&placed) {
                    for (row, rect) in group.rows.iter().zip(&spots.rows) {
                        if row.id != id {
                            continue;
                        }
                        return Some(match &row.control {
                            Control::Switch(_) => self.switch_rect(*rect),
                            Control::Buttons(marks) if !marks.is_empty() => {
                                self.mark_rects(*rect, marks)[part.min(marks.len() - 1)]
                            }
                            Control::Segments { options, chosen } => {
                                self.segment_rect(*rect, options.len(), *chosen)
                            }
                            _ => *rect,
                        });
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn model() -> Model {
        super::super::tests::model(&Config::default())
    }

    #[test]
    fn tab_walks_from_the_sidebar_through_the_rows_and_back() {
        let model = model();
        let section = Section::Time;
        let Move::Focus(first) = model.key_move(section, Some(Spot::Sidebar), Press::Next) else {
            panic!("tab leaves the sidebar");
        };
        assert!(matches!(first, Spot::Row(..)));
        let mut spot = first;
        let mut seen = 1;
        loop {
            match model.key_move(section, Some(spot), Press::Next) {
                Move::Focus(Spot::Sidebar) => break,
                Move::Focus(next) => {
                    spot = next;
                    seen += 1;
                }
                _ => panic!("tab only moves focus"),
            }
            assert!(seen < 100);
        }
        assert!(seen >= 3, "the time page has several stops, saw {seen}");
    }

    #[test]
    fn space_flips_a_switch_and_arrows_step_segments() {
        let model = model();
        let section = Section::Time;
        assert!(matches!(
            model.key_move(section, Some(Spot::Row(Id::Seconds, 0)), Press::Go),
            Move::Act(Hit::Change(Id::Seconds, Value::Switch(false)))
        ));
        assert!(matches!(
            model.key_move(section, Some(Spot::Row(Id::Language, 0)), Press::Right),
            Move::Act(Hit::Change(Id::Language, Value::Choice(1)))
        ));
        assert!(matches!(model.key_move(section, Some(Spot::Row(Id::Language, 0)), Press::Left), Move::Stay));
    }

    #[test]
    fn up_and_down_in_the_sidebar_change_the_section() {
        let model = model();
        assert!(matches!(
            model.key_move(Section::Desktop, Some(Spot::Sidebar), Press::Down),
            Move::Section(section) if section != Section::Desktop
        ));
        assert!(matches!(model.key_move(Section::Desktop, Some(Spot::Sidebar), Press::Up), Move::Stay));
    }

    #[test]
    fn the_focus_ring_finds_its_row() {
        let model = model();
        let look = Look::default();
        let size = Vec2::new(900.0, 700.0);
        let switch = look.spot_rect(size, Section::Time, &model, 0.0, Spot::Row(Id::Seconds, 0)).unwrap();
        assert!(switch.size.x > 10.0 && switch.size.x < 80.0);
        assert!(look.spot_rect(size, Section::Time, &model, 0.0, Spot::Sidebar).is_some());
    }
}
