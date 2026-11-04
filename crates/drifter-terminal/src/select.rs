use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Spot {
    pub row: u64,
    pub col: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    Cell,
    Word,
    Line,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sel {
    pub anchor: Spot,
    pub head: Spot,
    pub unit: Unit,
}

pub type Lines = BTreeMap<u64, Vec<String>>;

fn wordy(text: &str) -> bool {
    !text.is_empty() && !text.chars().all(char::is_whitespace) && !text.chars().any(|c| "()[]{}<>'\"`|,;".contains(c))
}

fn cell(lines: &Lines, row: u64, col: u16) -> &str {
    lines.get(&row).and_then(|cells| cells.get(usize::from(col))).map_or("", String::as_str)
}

fn width(lines: &Lines, row: u64) -> u16 {
    lines.get(&row).map_or(0, |cells| cells.len() as u16)
}

impl Sel {
    pub fn at(spot: Spot, unit: Unit) -> Sel {
        Sel { anchor: spot, head: spot, unit }
    }

    pub fn empty(&self) -> bool {
        self.unit == Unit::Cell && self.anchor == self.head
    }

    pub fn bounds(&self, lines: &Lines) -> (Spot, Spot) {
        let (mut start, mut end) = match self.anchor <= self.head {
            true => (self.anchor, self.head),
            false => (self.head, self.anchor),
        };
        match self.unit {
            Unit::Cell => {}
            Unit::Line => {
                start.col = 0;
                end.col = width(lines, end.row).saturating_sub(1).max(end.col);
            }
            Unit::Word => {
                if wordy(cell(lines, start.row, start.col)) {
                    while start.col > 0 && wordy(cell(lines, start.row, start.col - 1)) {
                        start.col -= 1;
                    }
                }
                if wordy(cell(lines, end.row, end.col)) {
                    while end.col + 1 < width(lines, end.row) && wordy(cell(lines, end.row, end.col + 1)) {
                        end.col += 1;
                    }
                }
            }
        }
        (start, end)
    }

    pub fn text(&self, lines: &Lines) -> String {
        let (start, end) = self.bounds(lines);
        let mut out: Vec<String> = Vec::new();
        for row in start.row..=end.row {
            let cells = lines.get(&row).map(Vec::as_slice).unwrap_or(&[]);
            let from = if row == start.row { usize::from(start.col) } else { 0 };
            let to = if row == end.row { (usize::from(end.col) + 1).min(cells.len()) } else { cells.len() };
            let line: String = cells
                .get(from..to.max(from))
                .unwrap_or(&[])
                .iter()
                .map(|text| if text.is_empty() { " " } else { text.as_str() })
                .collect();
            out.push(line.trim_end().to_string());
        }
        out.join("\n")
    }
}

pub fn holds(bounds: (Spot, Spot), row: u64, col: u16) -> bool {
    let (start, end) = bounds;
    let here = Spot { row, col };
    start <= here && here <= end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(rows: &[&str]) -> Lines {
        rows.iter()
            .enumerate()
            .map(|(at, row)| (at as u64 + 10, row.chars().map(|c| if c == ' ' { String::new() } else { c.to_string() }).collect()))
            .collect()
    }

    fn spot(row: u64, col: u16) -> Spot {
        Spot { row, col }
    }

    #[test]
    fn a_drag_copies_across_rows_in_either_direction() {
        let held = lines(&["hello world   ", "second line   "]);
        let forward = Sel { anchor: spot(10, 6), head: spot(11, 5), unit: Unit::Cell };
        assert_eq!(forward.text(&held), "world\nsecond");
        let back = Sel { anchor: spot(11, 5), head: spot(10, 6), unit: Unit::Cell };
        assert_eq!(back.text(&held), "world\nsecond");
    }

    #[test]
    fn a_double_click_takes_the_word_and_a_triple_click_the_line() {
        let held = lines(&["run cargo build now"]);
        assert_eq!(Sel::at(spot(10, 6), Unit::Word).text(&held), "cargo");
        assert_eq!(Sel::at(spot(10, 6), Unit::Line).text(&held), "run cargo build now");
    }

    #[test]
    fn rows_that_were_never_seen_copy_as_empty_lines() {
        let held = lines(&["one"]);
        let sel = Sel { anchor: spot(10, 0), head: spot(12, 2), unit: Unit::Cell };
        assert_eq!(sel.text(&held), "one\n\n");
    }

    #[test]
    fn a_click_without_a_drag_selects_nothing() {
        assert!(Sel::at(spot(1, 1), Unit::Cell).empty());
        assert!(!Sel::at(spot(1, 1), Unit::Word).empty());
        let bounds = Sel { anchor: spot(3, 4), head: spot(1, 2), unit: Unit::Cell }.bounds(&Lines::new());
        assert!(holds(bounds, 2, 0));
        assert!(!holds(bounds, 1, 1));
        assert!(holds(bounds, 3, 4));
        assert!(!holds(bounds, 3, 5));
    }
}
