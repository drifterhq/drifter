#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Field {
    pub text: String,
    pub caret: usize,
    pub anchor: Option<usize>,
}

impl Field {
    pub fn new(text: &str) -> Field {
        Field { text: text.to_string(), caret: text.len(), anchor: None }
    }

    pub fn all(text: &str) -> Field {
        Field { text: text.to_string(), caret: text.len(), anchor: Some(0) }
    }

    pub fn stem(text: &str) -> Field {
        let end = match text.rfind('.') {
            Some(at) if at > 0 => at,
            _ => text.len(),
        };
        Field { text: text.to_string(), caret: end, anchor: Some(0) }
    }

    pub fn picked(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        (anchor != self.caret).then(|| (anchor.min(self.caret), anchor.max(self.caret)))
    }

    fn drop_picked(&mut self) -> bool {
        match self.picked() {
            Some((from, to)) => {
                self.text.replace_range(from..to, "");
                self.caret = from;
                self.anchor = None;
                true
            }
            None => {
                self.anchor = None;
                false
            }
        }
    }

    pub fn put(&mut self, words: &str) {
        self.drop_picked();
        let clean: String = words.chars().filter(|c| !c.is_control()).collect();
        self.text.insert_str(self.caret, &clean);
        self.caret += clean.len();
    }

    pub fn back(&mut self, word: bool) {
        if self.drop_picked() || self.caret == 0 {
            return;
        }
        let from = if word { self.word_left() } else { self.left_of(self.caret) };
        self.text.replace_range(from..self.caret, "");
        self.caret = from;
    }

    pub fn forward_delete(&mut self, word: bool) {
        if self.drop_picked() || self.caret >= self.text.len() {
            return;
        }
        let to = if word { self.word_right() } else { self.right_of(self.caret) };
        self.text.replace_range(self.caret..to, "");
    }

    fn left_of(&self, at: usize) -> usize {
        self.text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
    }

    fn right_of(&self, at: usize) -> usize {
        self.text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
    }

    fn word_left(&self) -> usize {
        let mut at = self.caret;
        while at > 0 && !self.text[..at].chars().next_back().is_some_and(char::is_alphanumeric) {
            at = self.left_of(at);
        }
        while at > 0 && self.text[..at].chars().next_back().is_some_and(char::is_alphanumeric) {
            at = self.left_of(at);
        }
        at
    }

    fn word_right(&self) -> usize {
        let mut at = self.caret;
        while at < self.text.len() && !self.text[at..].chars().next().is_some_and(char::is_alphanumeric) {
            at = self.right_of(at);
        }
        while at < self.text.len() && self.text[at..].chars().next().is_some_and(char::is_alphanumeric) {
            at = self.right_of(at);
        }
        at
    }

    fn go(&mut self, to: usize, keep: bool) {
        match keep {
            true => {
                self.anchor.get_or_insert(self.caret);
            }
            false => self.anchor = None,
        }
        self.caret = to;
    }

    pub fn left(&mut self, word: bool, keep: bool) {
        if !keep && let Some((from, _)) = self.picked() {
            self.go(from, false);
            return;
        }
        let to = if word { self.word_left() } else { self.left_of(self.caret) };
        self.go(to, keep);
    }

    pub fn right(&mut self, word: bool, keep: bool) {
        if !keep && let Some((_, to)) = self.picked() {
            self.go(to, false);
            return;
        }
        let to = if word { self.word_right() } else { self.right_of(self.caret) };
        self.go(to, keep);
    }

    pub fn home(&mut self, keep: bool) {
        self.go(0, keep);
    }

    pub fn end(&mut self, keep: bool) {
        self.go(self.text.len(), keep);
    }

    pub fn pick_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    pub fn picked_text(&self) -> Option<&str> {
        self.picked().map(|(from, to)| &self.text[from..to])
    }

    pub fn cut(&mut self) -> Option<String> {
        let out = self.picked_text()?.to_string();
        self.drop_picked();
        Some(out)
    }

    pub fn caret_at(&mut self, at: usize, keep: bool) {
        let mut at = at.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        self.go(at, keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_replaces_the_picked_part() {
        let mut field = Field::stem("holiday.jpg");
        assert_eq!(field.picked_text(), Some("holiday"));
        field.put("beach");
        assert_eq!(field.text, "beach.jpg");
        assert_eq!(field.caret, 5);
    }

    #[test]
    fn words_jump_and_delete() {
        let mut field = Field::new("/home/me/Pictures");
        field.back(true);
        assert_eq!(field.text, "/home/me/");
        field.left(true, false);
        assert_eq!(field.caret, 6);
        field.home(false);
        field.forward_delete(false);
        assert_eq!(field.text, "home/me/");
    }

    #[test]
    fn wide_letters_keep_whole() {
        let mut field = Field::new("über");
        field.home(false);
        field.right(false, true);
        assert_eq!(field.picked_text(), Some("ü"));
        field.end(false);
        field.back(false);
        field.back(false);
        field.back(false);
        assert_eq!(field.text, "ü");
        field.back(false);
        assert_eq!(field.text, "");
        field.caret_at(9, false);
        assert_eq!(field.caret, 0);
    }
}
