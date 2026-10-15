use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::text::Text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Insert,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerAction {
    Window {
        session_id: String,
        window_index: u32,
    },
    Session {
        session_name: String,
    },
    Unavailable {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub display_text: String,
    pub searchable_text: String,
    pub preview_title: String,
    pub pane_id: Option<String>,
    pub preview_message: Option<String>,
    pub current: bool,
    pub harpoon_slot: Option<u8>,
    pub action: PickerAction,
}

#[derive(Debug)]
pub struct FilteredItem {
    pub index: usize,
    pub matched: Vec<usize>,
}

#[derive(Debug)]
pub struct App {
    pub items: Vec<PickerEntry>,
    pub filtered: Vec<FilteredItem>,
    pub query: String,
    pub cursor_byte: usize,
    pub selected: usize,
    pub mode: InputMode,
    pub preview: Text<'static>,
    pub error: Option<String>,
    pub should_quit: bool,
    pub should_open: bool,
    pub preview_dirty: bool,
    pub pending_g: bool,
    pub pending_d: bool,
    pub should_delete: bool,
}

impl App {
    pub fn new(items: Vec<PickerEntry>) -> Self {
        let filtered = (0..items.len())
            .map(|index| FilteredItem {
                index,
                matched: Vec::new(),
            })
            .collect();
        Self {
            items,
            filtered,
            query: String::new(),
            cursor_byte: 0,
            selected: 0,
            mode: InputMode::Insert,
            preview: Text::default(),
            error: None,
            should_quit: false,
            should_open: false,
            preview_dirty: true,
            pending_g: false,
            pending_d: false,
            should_delete: false,
        }
    }

    pub fn selected_item(&self) -> Option<&PickerEntry> {
        self.filtered
            .get(self.selected)
            .and_then(|item| self.items.get(item.index))
    }

    pub fn insert_query(&mut self, character: char) {
        self.query.insert(self.cursor_byte, character);
        self.cursor_byte += character.len_utf8();
        self.refilter();
    }

    pub fn backspace_query(&mut self) {
        let Some((previous, _)) = self.query[..self.cursor_byte].char_indices().next_back() else {
            return;
        };
        self.query.drain(previous..self.cursor_byte);
        self.cursor_byte = previous;
        self.refilter();
    }

    pub fn enter_normal_mode(&mut self) {
        self.mode = InputMode::Normal;
        if let Some((previous, _)) = self.query[..self.cursor_byte].char_indices().next_back() {
            self.cursor_byte = previous;
        }
    }

    pub fn insert_before_cursor(&mut self) {
        self.mode = InputMode::Insert;
    }

    pub fn insert_after_cursor(&mut self) {
        if let Some(character) = self.query[self.cursor_byte..].chars().next() {
            self.cursor_byte += character.len_utf8();
        }
        self.mode = InputMode::Insert;
    }

    pub fn insert_at_end(&mut self) {
        self.cursor_byte = self.query.len();
        self.mode = InputMode::Insert;
    }

    pub fn move_cursor_left(&mut self) {
        if let Some((previous, _)) = self.query[..self.cursor_byte].char_indices().next_back() {
            self.cursor_byte = previous;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let Some(character) = self.query[self.cursor_byte..].chars().next() else {
            return;
        };
        let next = self.cursor_byte + character.len_utf8();
        if next < self.query.len() {
            self.cursor_byte = next;
        }
    }

    pub fn move_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
            self.preview_dirty = true;
        }
    }

    pub fn move_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.preview_dirty = true;
    }

    pub fn move_first(&mut self) {
        self.selected = 0;
        self.preview_dirty = true;
    }

    pub fn move_last(&mut self) {
        self.selected = self.filtered.len().saturating_sub(1);
        self.preview_dirty = true;
    }

    pub fn remove_selected_item(&mut self) -> Option<PickerEntry> {
        let item_index = self.filtered.get(self.selected)?.index;
        let previous_selection = self.selected;
        let removed = self.items.remove(item_index);
        self.refilter();
        self.selected = previous_selection.min(self.filtered.len().saturating_sub(1));
        self.preview_dirty = true;
        Some(removed)
    }

    fn refilter(&mut self) {
        let matcher = SkimMatcherV2::default().smart_case();
        let mut matches: Vec<(usize, i64, Vec<usize>)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if self.query.is_empty() {
                    Some((index, 0, Vec::new()))
                } else {
                    matcher
                        .fuzzy_indices(&item.searchable_text, &self.query)
                        .map(|(score, matched)| (index, score, matched))
                }
            })
            .collect();

        if !self.query.is_empty() {
            matches.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
        }

        self.filtered = matches
            .into_iter()
            .map(|(index, _, matched)| FilteredItem { index, matched })
            .collect();
        self.selected = 0;
        self.preview_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(session: &str, window: &str, command: &str) -> PickerEntry {
        let display_text = format!("{session}:1  {window}  {command}  /tmp");
        PickerEntry {
            searchable_text: display_text.clone(),
            display_text,
            preview_title: format!("{session} / {window}"),
            pane_id: Some("%1".into()),
            preview_message: None,
            current: false,
            harpoon_slot: None,
            action: PickerAction::Window {
                session_id: "$1".into(),
                window_index: 1,
            },
        }
    }

    #[test]
    fn searches_across_session_window_command_and_path() {
        let mut app = App::new(vec![
            target("swing-app", "agent", "opencode"),
            target("tmux-enhance", "editor", "nvim"),
        ]);

        app.query = "enh nvim".into();
        app.refilter();

        assert_eq!(app.filtered.len(), 1);
        assert_eq!(app.filtered[0].index, 1);
        assert!(!app.filtered[0].matched.is_empty());
    }

    #[test]
    fn selection_stays_in_bounds() {
        let mut app = App::new(vec![target("one", "editor", "nvim")]);
        app.move_next();
        app.move_next();
        assert_eq!(app.selected, 0);
        app.move_previous();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn supports_vim_style_insert_positions() {
        let mut app = App::new(vec![target("one", "editor", "nvim")]);
        app.insert_at_end();
        app.insert_query('a');
        app.insert_query('界');
        app.enter_normal_mode();
        assert_eq!(app.cursor_byte, 1);

        app.insert_before_cursor();
        app.insert_query('b');
        assert_eq!(app.query, "ab界");

        app.enter_normal_mode();
        app.insert_after_cursor();
        app.insert_query('c');
        assert_eq!(app.query, "abc界");

        app.enter_normal_mode();
        app.insert_at_end();
        app.insert_query('d');
        assert_eq!(app.query, "abc界d");
    }

    #[test]
    fn moves_normal_cursor_across_unicode_characters() {
        let mut app = App::new(vec![target("one", "editor", "nvim")]);
        app.query = "a界c".into();
        app.cursor_byte = app.query.len();
        app.enter_normal_mode();

        assert_eq!(app.cursor_byte, 4);
        app.move_cursor_left();
        assert_eq!(app.cursor_byte, 1);
        app.move_cursor_left();
        assert_eq!(app.cursor_byte, 0);
        app.move_cursor_left();
        assert_eq!(app.cursor_byte, 0);
        app.move_cursor_right();
        assert_eq!(app.cursor_byte, 1);
        app.move_cursor_right();
        assert_eq!(app.cursor_byte, 4);
        app.move_cursor_right();
        assert_eq!(app.cursor_byte, 4);
    }
}
