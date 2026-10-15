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
pub struct PreviewPane {
    pub title: String,
    pub pane_id: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct RenderedPreview {
    pub title: String,
    pub text: Text<'static>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerHierarchy {
    Parent { id: String, expanded: bool },
    Child { parent_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub display_text: String,
    pub searchable_text: String,
    pub preview_title: String,
    pub preview_panes: Vec<PreviewPane>,
    pub preview_message: Option<String>,
    pub current: bool,
    pub harpoon_slot: Option<u8>,
    pub hierarchy: Option<PickerHierarchy>,
    pub visible: bool,
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
    pub rendered_previews: Vec<RenderedPreview>,
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
        let filtered = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.visible)
            .map(|(index, _)| FilteredItem {
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
            rendered_previews: Vec::new(),
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

    pub fn toggle_selected_expansion(&mut self) {
        let Some(item_index) = self.filtered.get(self.selected).map(|item| item.index) else {
            return;
        };
        let Some(PickerHierarchy::Parent { id, expanded }) = self
            .items
            .get_mut(item_index)
            .and_then(|item| item.hierarchy.as_mut())
        else {
            return;
        };
        *expanded = !*expanded;
        let expanded = *expanded;
        let id = id.clone();

        for item in &mut self.items {
            if matches!(
                &item.hierarchy,
                Some(PickerHierarchy::Child { parent_id }) if parent_id == &id
            ) {
                item.visible = expanded;
            }
        }
        self.refilter();
        self.selected = self
            .filtered
            .iter()
            .position(|item| item.index == item_index)
            .unwrap_or_default();
        self.preview_dirty = true;
    }

    pub fn toggle_all_expansions(&mut self) {
        let expand = self.items.iter().any(|item| {
            matches!(
                &item.hierarchy,
                Some(PickerHierarchy::Parent {
                    expanded: false,
                    ..
                })
            )
        });
        if !self
            .items
            .iter()
            .any(|item| matches!(&item.hierarchy, Some(PickerHierarchy::Parent { .. })))
        {
            return;
        }

        let previous_selection = self.selected;
        let selected_item = self.filtered.get(self.selected).map(|item| item.index);
        let selected_parent = selected_item.and_then(|index| match &self.items[index].hierarchy {
            Some(PickerHierarchy::Child { parent_id }) => Some(parent_id.clone()),
            _ => None,
        });
        for item in &mut self.items {
            match &mut item.hierarchy {
                Some(PickerHierarchy::Parent { expanded, .. }) => *expanded = expand,
                Some(PickerHierarchy::Child { .. }) => item.visible = expand,
                None => {}
            }
        }
        self.refilter();
        if let Some(selected_item) = selected_item {
            self.selected = self
                .filtered
                .iter()
                .position(|item| item.index == selected_item)
                .or_else(|| {
                    let parent_id = selected_parent.as_ref()?;
                    self.filtered.iter().position(|filtered| {
                        matches!(
                            &self.items[filtered.index].hierarchy,
                            Some(PickerHierarchy::Parent { id, .. }) if id == parent_id
                        )
                    })
                })
                .unwrap_or_else(|| previous_selection.min(self.filtered.len().saturating_sub(1)));
        }
        self.preview_dirty = true;
    }

    fn refilter(&mut self) {
        let matcher = SkimMatcherV2::default().smart_case();
        if self.query.is_empty() {
            self.filtered = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.visible)
                .map(|(index, _)| FilteredItem {
                    index,
                    matched: Vec::new(),
                })
                .collect();
            self.selected = 0;
            self.preview_dirty = true;
            return;
        }

        let matches: Vec<Option<(i64, Vec<usize>)>> = self
            .items
            .iter()
            .map(|item| matcher.fuzzy_indices(&item.searchable_text, &self.query))
            .collect();

        let mut groups: Vec<(i64, usize, Vec<FilteredItem>)> = Vec::new();
        let mut index = 0;
        while index < self.items.len() {
            match &self.items[index].hierarchy {
                Some(PickerHierarchy::Parent { id, .. }) => {
                    let group_id = id.clone();
                    let parent_match = matches[index].clone();
                    let mut child_matches = Vec::new();
                    let mut next = index + 1;
                    while next < self.items.len()
                        && matches!(
                            &self.items[next].hierarchy,
                            Some(PickerHierarchy::Child { parent_id }) if parent_id == &group_id
                        )
                    {
                        if let Some((score, matched)) = &matches[next] {
                            child_matches.push((next, *score, matched.clone()));
                        }
                        next += 1;
                    }

                    if parent_match.is_some() || !child_matches.is_empty() {
                        let parent_score = parent_match.as_ref().map_or(i64::MIN, |item| item.0);
                        let child_score = child_matches
                            .iter()
                            .map(|(_, score, _)| *score)
                            .max()
                            .unwrap_or(i64::MIN);
                        let mut entries = vec![FilteredItem {
                            index,
                            matched: parent_match.map_or_else(Vec::new, |item| item.1),
                        }];
                        entries.extend(
                            child_matches
                                .into_iter()
                                .map(|(index, _, matched)| FilteredItem { index, matched }),
                        );
                        groups.push((parent_score.max(child_score), index, entries));
                    }
                    index = next;
                }
                Some(PickerHierarchy::Child { .. }) => {
                    index += 1;
                }
                None => {
                    if let Some((score, matched)) = &matches[index] {
                        groups.push((
                            *score,
                            index,
                            vec![FilteredItem {
                                index,
                                matched: matched.clone(),
                            }],
                        ));
                    }
                    index += 1;
                }
            }
        }

        groups.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        self.filtered = groups
            .into_iter()
            .flat_map(|(_, _, entries)| entries)
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
            preview_panes: vec![PreviewPane {
                title: format!("{session} / {window}"),
                pane_id: "%1".into(),
                active: true,
            }],
            preview_message: None,
            current: false,
            harpoon_slot: None,
            hierarchy: None,
            visible: true,
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

    #[test]
    fn toggles_hierarchical_children() {
        let mut parent = target("project", "session", "");
        parent.hierarchy = Some(PickerHierarchy::Parent {
            id: "project".into(),
            expanded: false,
        });
        let mut child = target("project", "editor", "nvim");
        child.hierarchy = Some(PickerHierarchy::Child {
            parent_id: "project".into(),
        });
        child.visible = false;
        let mut app = App::new(vec![parent, child]);

        assert_eq!(app.filtered.len(), 1);
        app.toggle_selected_expansion();
        assert_eq!(app.filtered.len(), 2);
        app.toggle_selected_expansion();
        assert_eq!(app.filtered.len(), 1);
    }

    #[test]
    fn toggles_all_hierarchical_children() {
        let mut first_parent = target("one", "session", "");
        first_parent.hierarchy = Some(PickerHierarchy::Parent {
            id: "one".into(),
            expanded: false,
        });
        let mut first_child = target("one", "editor", "nvim");
        first_child.hierarchy = Some(PickerHierarchy::Child {
            parent_id: "one".into(),
        });
        first_child.visible = false;
        let mut second_parent = target("two", "session", "");
        second_parent.hierarchy = Some(PickerHierarchy::Parent {
            id: "two".into(),
            expanded: false,
        });
        let mut second_child = target("two", "agent", "opencode");
        second_child.hierarchy = Some(PickerHierarchy::Child {
            parent_id: "two".into(),
        });
        second_child.visible = false;
        let mut app = App::new(vec![first_parent, first_child, second_parent, second_child]);

        app.toggle_all_expansions();
        assert_eq!(app.filtered.len(), 4);
        app.toggle_all_expansions();
        assert_eq!(app.filtered.len(), 2);
    }

    #[test]
    fn reveals_matching_children_without_expanding_the_parent() {
        let mut parent = target("project", "2 windows", "editor agent");
        parent.hierarchy = Some(PickerHierarchy::Parent {
            id: "project".into(),
            expanded: false,
        });
        let mut editor = target("project", "editor", "nvim");
        editor.hierarchy = Some(PickerHierarchy::Child {
            parent_id: "project".into(),
        });
        editor.visible = false;
        let mut agent = target("project", "agent", "opencode");
        agent.hierarchy = Some(PickerHierarchy::Child {
            parent_id: "project".into(),
        });
        agent.visible = false;
        let mut app = App::new(vec![parent, editor, agent]);

        app.insert_query('a');
        app.insert_query('g');
        app.insert_query('e');
        app.insert_query('n');
        app.insert_query('t');
        assert_eq!(app.filtered.len(), 2);
        assert_eq!(app.filtered[0].index, 0);
        assert_eq!(app.filtered[1].index, 2);

        while app.cursor_byte > 0 {
            app.backspace_query();
        }
        assert_eq!(app.filtered.len(), 1);
    }
}
