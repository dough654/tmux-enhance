use std::{
    io,
    time::{Duration, Instant},
};

use ansi_to_tui::IntoText as _;
use anyhow::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListDirection, ListItem, ListState, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{
        App, InputMode, PickerAction, PickerBadgeTone, PickerEntry, PickerHierarchy, PreviewPane,
        RenderedPreview,
    },
    store::Store,
    theme::Theme,
    tmux::Tmux,
};

const PREVIEW_REFRESH: Duration = Duration::from_millis(500);
const ENTRY_REFRESH: Duration = Duration::from_secs(1);
const UI_VERTICAL_CHROME: u16 = 6;
const UI_HORIZONTAL_PADDING: u16 = 1;

type EntryRefresher<'a> = dyn FnMut() -> Result<Vec<PickerEntry>> + 'a;

pub fn run() -> Result<()> {
    let tmux = Tmux::from_env();
    let entries = tmux
        .windows()?
        .into_iter()
        .map(|window| {
            let display_text = window.searchable_text();
            PickerEntry {
                searchable_text: display_text.clone(),
                display_text,
                secondary_text: None,
                badge: None,
                preview_title: format!("{} / {}", window.session_name, window.window_name),
                preview_panes: vec![PreviewPane {
                    title: format!("{} / {}", window.session_name, window.window_name),
                    pane_id: window.pane_id,
                    active: true,
                }],
                preview_message: None,
                current: window.current,
                harpoon_slot: None,
                hierarchy: None,
                visible: true,
                action: PickerAction::Window {
                    session_id: window.session_id,
                    window_index: window.window_index,
                },
            }
        })
        .collect();
    run_entries("windows", "Windows", entries)
}

pub fn run_entries(source: &str, list_title: &str, entries: Vec<PickerEntry>) -> Result<()> {
    run_entries_inner(source, list_title, entries, None)
}

pub fn run_entries_live(
    source: &str,
    list_title: &str,
    entries: Vec<PickerEntry>,
    refresh: &mut EntryRefresher<'_>,
) -> Result<()> {
    run_entries_inner(source, list_title, entries, Some(refresh))
}

fn run_entries_inner(
    source: &str,
    list_title: &str,
    entries: Vec<PickerEntry>,
    refresh: Option<&mut EntryRefresher<'_>>,
) -> Result<()> {
    let tmux = Tmux::from_env();
    let theme = Theme::from_tmux(&tmux);
    let mut app = App::new(entries);
    refresh_preview(&tmux, &mut app, 80, theme);

    let mut terminal = setup_terminal()?;
    let _cleanup = TerminalCleanup;
    let result = run_loop(
        &mut terminal,
        &tmux,
        &mut app,
        theme,
        source,
        list_title,
        refresh,
    );
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tmux: &Tmux,
    app: &mut App,
    theme: Theme,
    source: &str,
    list_title: &str,
    mut refresh: Option<&mut EntryRefresher<'_>>,
) -> Result<()> {
    let mut last_preview = Instant::now();
    let mut last_entry_refresh = Instant::now();
    let mut cursor_mode = None;

    loop {
        if cursor_mode != Some(app.mode) {
            set_cursor_style(app.mode)?;
            cursor_mode = Some(app.mode);
        }
        terminal.draw(|frame| render(frame, app, theme, source, list_title))?;
        if app.should_quit {
            return Ok(());
        }
        if app.should_delete {
            app.should_delete = false;
            if let Some(slot) = app.selected_item().and_then(|item| item.harpoon_slot) {
                Store::open()?.remove(slot)?;
                app.remove_selected_item();
                let preview_height = terminal.size()?.height.saturating_sub(UI_VERTICAL_CHROME);
                refresh_preview(tmux, app, preview_height, theme);
            }
        }
        if app.should_open {
            let action = app.selected_item().map(|item| item.action.clone());
            match action {
                Some(PickerAction::Window {
                    session_id,
                    window_index,
                }) => tmux.switch_to_window(&session_id, window_index)?,
                Some(PickerAction::Session { session_name }) => {
                    tmux.switch_to_session(&session_name)?
                }
                Some(PickerAction::Unavailable { message }) => {
                    app.error = Some(message);
                    app.should_open = false;
                    continue;
                }
                None => {}
            }
            return Ok(());
        }

        if event::poll(Duration::from_millis(75))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key, source == "harpoon");
        }

        if app.preview_dirty || last_preview.elapsed() >= PREVIEW_REFRESH {
            let preview_height = terminal.size()?.height.saturating_sub(UI_VERTICAL_CHROME);
            refresh_preview(tmux, app, preview_height, theme);
            last_preview = Instant::now();
        }

        if last_entry_refresh.elapsed() >= ENTRY_REFRESH {
            if let Some(refresh) = refresh.as_mut()
                && let Ok(items) = refresh()
            {
                app.replace_items(items);
            }
            last_entry_refresh = Instant::now();
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent, allow_delete: bool) {
    match app.mode {
        InputMode::Insert => match key.code {
            KeyCode::Esc => {
                app.enter_normal_mode();
                app.pending_g = false;
                app.pending_d = false;
            }
            KeyCode::Enter => app.should_open = true,
            KeyCode::Backspace => app.backspace_query(),
            KeyCode::Tab => app.toggle_selected_expansion(),
            KeyCode::BackTab => app.toggle_all_expansions(),
            KeyCode::Down => app.move_previous(),
            KeyCode::Up => app.move_next(),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.insert_query(character)
            }
            _ => {}
        },
        InputMode::Normal => {
            let completing_gg = key.code == KeyCode::Char('g') && app.pending_g;
            let completing_dd = allow_delete && key.code == KeyCode::Char('d') && app.pending_d;
            app.pending_g = false;
            app.pending_d = false;
            match key.code {
                KeyCode::Char('g') if completing_gg => app.move_last(),
                KeyCode::Char('g') => app.pending_g = true,
                KeyCode::Char('d') if completing_dd => app.should_delete = true,
                KeyCode::Char('d') if allow_delete => app.pending_d = true,
                KeyCode::Char('j') | KeyCode::Down => app.move_previous(),
                KeyCode::Char('k') | KeyCode::Up => app.move_next(),
                KeyCode::Char('h') | KeyCode::Left => app.move_cursor_left(),
                KeyCode::Char('l') | KeyCode::Right => app.move_cursor_right(),
                KeyCode::Char('G') => app.move_first(),
                KeyCode::Tab => app.toggle_selected_expansion(),
                KeyCode::BackTab => app.toggle_all_expansions(),
                KeyCode::Char('i') => app.insert_before_cursor(),
                KeyCode::Char('a') => app.insert_after_cursor(),
                KeyCode::Char('A') | KeyCode::Char('/') => app.insert_at_end(),
                KeyCode::Enter => app.should_open = true,
                KeyCode::Esc | KeyCode::Char('q') => app.should_quit = true,
                _ => {}
            }
        }
    }
}

fn refresh_preview(tmux: &Tmux, app: &mut App, height: u16, theme: Theme) {
    let Some(item) = app.selected_item() else {
        app.preview = Default::default();
        app.rendered_previews.clear();
        app.error = None;
        app.preview_dirty = false;
        return;
    };
    let preview_panes = item.preview_panes.clone();
    let preview_message = item.preview_message.clone();
    if preview_panes.is_empty() {
        app.preview = preview_message.unwrap_or_default().into();
        app.rendered_previews.clear();
        app.error = None;
        app.preview_dirty = false;
        return;
    }

    let rows = if preview_panes.len() > 2 { 2 } else { 1 };
    let pane_height = (height / rows).saturating_sub(2).max(4);
    app.rendered_previews = preview_panes
        .into_iter()
        .map(|pane| {
            let text = tmux
                .capture_preview(&pane.pane_id, pane_height)
                .and_then(|preview| preview_text(&preview, theme.base()))
                .unwrap_or_else(|error| format!("Preview unavailable: {error}").into());
            RenderedPreview {
                title: pane.title,
                text,
                active: pane.active,
            }
        })
        .collect();
    app.preview = Default::default();
    app.error = None;
    app.preview_dirty = false;
}

fn preview_text(preview: &str, base: Style) -> Result<Text<'static>> {
    let mut text = preview.into_text()?;
    // ANSI resets mean the picker's defaults, not tmux's separate popup defaults.
    for span in text.lines.iter_mut().flat_map(|line| &mut line.spans) {
        if span.style.fg == Some(Color::Reset) {
            span.style.fg = base.fg;
        }
        if span.style.bg == Some(Color::Reset) {
            span.style.bg = base.bg;
        }
    }
    Ok(text)
}

fn render(frame: &mut Frame, app: &mut App, theme: Theme, source: &str, list_title: &str) {
    frame.render_widget(Block::default().style(theme.base()), frame.area());
    let frame_area = frame.area();
    let content_area = Rect::new(
        frame_area.x.saturating_add(UI_HORIZONTAL_PADDING),
        frame_area.y,
        frame_area
            .width
            .saturating_sub(UI_HORIZONTAL_PADDING.saturating_mul(2)),
        frame_area.height,
    );
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(content_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(outer[1]);
    render_items(frame, app, columns[0], theme, list_title);
    render_preview(frame, app, columns[1], theme);
    render_search(frame, app, outer[2], theme);
    render_footer(frame, app, outer[3], theme, source);
}

fn render_search(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let mode = match app.mode {
        InputMode::Insert => Span::styled(
            " INSERT ",
            Style::default()
                .fg(theme.foreground)
                .bg(theme.accent)
                .bold(),
        ),
        InputMode::Normal => Span::styled(
            " NORMAL ",
            Style::default()
                .fg(theme.normal_mode_foreground)
                .bg(theme.normal_mode_background)
                .bold(),
        ),
    };
    let query = Paragraph::new(Line::from(vec![
        mode,
        Span::raw("  "),
        Span::raw(&app.query),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .style(theme.base()),
    );
    frame.render_widget(query, area);

    let cursor_offset = app.query[..app.cursor_byte].width() as u16;
    let x = (area.x + 11 + cursor_offset).min(area.right().saturating_sub(2));
    frame.set_cursor_position(Position::new(x, area.y + 1));
}

fn render_items(frame: &mut Frame, app: &mut App, area: Rect, theme: Theme, title: &str) {
    let item_width = usize::from(area.width.saturating_sub(4));
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .enumerate()
        .filter_map(|(row, filtered)| {
            app.items
                .get(filtered.index)
                .map(|item| (row, item, &filtered.matched))
        })
        .map(|(row, item, matched)| {
            let marker = if item.current { "●" } else { " " };
            let mut spans = vec![Span::styled(
                format!("{marker} "),
                Style::default().fg(theme.accent),
            )];
            let mut continuation_indent = 2;
            if let Some(PickerHierarchy::Parent { expanded, .. }) = &item.hierarchy {
                spans.push(Span::styled(
                    if *expanded { "▾ " } else { "▸ " },
                    Style::default().fg(theme.accent),
                ));
                continuation_indent += 2;
            }
            let badge_width = item
                .badge
                .as_ref()
                .map_or(0, |badge| badge.label.as_str().width() + 2);
            let display_width = item_width
                .saturating_sub(continuation_indent)
                .saturating_sub(badge_width)
                .saturating_sub(usize::from(item.badge.is_some()));
            let display_text = truncate_to_width(&item.display_text, display_width);
            spans.extend(highlighted_spans(
                &display_text,
                matched,
                0,
                theme.text(),
                theme,
            ));
            if let Some(badge) = &item.badge {
                let used_width = continuation_indent + display_text.as_str().width() + badge_width;
                spans.push(Span::raw(" ".repeat(item_width.saturating_sub(used_width))));
                spans.push(Span::styled(
                    format!(" {} ", badge.label),
                    badge_style(badge.tone, theme),
                ));
            }
            let mut lines = vec![Line::from(spans)];
            if let Some(secondary) = &item.secondary_text {
                let offset = item.display_text.chars().count() + 1;
                let mut secondary_spans = vec![Span::raw(" ".repeat(continuation_indent))];
                secondary_spans.extend(highlighted_spans(
                    secondary,
                    matched,
                    offset,
                    theme.text().add_modifier(Modifier::DIM),
                    theme,
                ));
                lines.push(Line::from(secondary_spans));
            }
            let list_item = ListItem::new(Text::from(lines));
            if row == app.selected {
                list_item.style(
                    Style::default()
                        .bg(theme.selection_background)
                        .fg(theme.selection_foreground),
                )
            } else {
                list_item
            }
        })
        .collect();

    let mut state =
        ListState::default().with_selected((!app.filtered.is_empty()).then_some(app.selected));
    let list = List::new(items)
        .direction(ListDirection::BottomToTop)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title_style(theme.text().add_modifier(Modifier::BOLD))
                .style(theme.base())
                .title(format!(" {title} ({}) ", app.filtered.len())),
        )
        .highlight_symbol("› ")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, area, &mut state);
}

fn truncate_to_width(text: &str, maximum_width: usize) -> String {
    let mut width = 0;
    text.chars()
        .take_while(|character| {
            let character_width = character.width().unwrap_or_default();
            if width + character_width > maximum_width {
                return false;
            }
            width += character_width;
            true
        })
        .collect()
}

fn highlighted_spans(
    text: &str,
    matched: &[usize],
    offset: usize,
    base_style: Style,
    theme: Theme,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut segment = String::new();
    let mut segment_is_match = false;

    for (index, character) in text.chars().enumerate() {
        let is_match = matched.binary_search(&(index + offset)).is_ok();
        if !segment.is_empty() && is_match != segment_is_match {
            let style = if segment_is_match {
                theme.match_style()
            } else {
                base_style
            };
            spans.push(Span::styled(std::mem::take(&mut segment), style));
        }
        segment_is_match = is_match;
        segment.push(character);
    }
    if !segment.is_empty() {
        let style = if segment_is_match {
            theme.match_style()
        } else {
            base_style
        };
        spans.push(Span::styled(segment, style));
    }
    spans
}

fn badge_style(tone: PickerBadgeTone, theme: Theme) -> Style {
    match tone {
        PickerBadgeTone::Processing => Style::default()
            .fg(theme.foreground)
            .bg(theme.agent_processing_background)
            .add_modifier(Modifier::BOLD),
        PickerBadgeTone::Permission => Style::default()
            .fg(theme.normal_mode_foreground)
            .bg(theme.agent_permission_background)
            .add_modifier(Modifier::BOLD),
        PickerBadgeTone::Input => Style::default()
            .fg(theme.foreground)
            .bg(theme.agent_input_background)
            .add_modifier(Modifier::BOLD),
        PickerBadgeTone::Ready => Style::default()
            .fg(theme.foreground)
            .bg(theme.agent_ready_background)
            .add_modifier(Modifier::BOLD),
        PickerBadgeTone::Muted => Style::default()
            .fg(theme.foreground)
            .bg(theme.selection_background)
            .add_modifier(Modifier::BOLD),
    }
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    if !app.rendered_previews.is_empty() {
        render_preview_grid(frame, &app.rendered_previews, area, theme);
        return;
    }

    let title = app
        .selected_item()
        .map(|item| format!(" {} ", item.preview_title))
        .unwrap_or_else(|| " Preview ".into());
    let preview = if let Some(error) = app.error.as_deref() {
        Paragraph::new(error)
    } else {
        Paragraph::new(app.preview.clone())
    }
    .style(theme.base())
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title_style(theme.text().add_modifier(Modifier::BOLD))
            .style(theme.base())
            .title(title),
    );
    frame.render_widget(preview, area);
}

fn render_preview_grid(frame: &mut Frame, previews: &[RenderedPreview], area: Rect, theme: Theme) {
    let areas = preview_areas(area, previews.len());
    for (preview, area) in previews.iter().zip(areas) {
        let border = if preview.active {
            theme.accent
        } else {
            theme.border
        };
        let widget = Paragraph::new(preview.text.clone())
            .style(theme.base())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border))
                    .title_style(theme.text().add_modifier(Modifier::BOLD))
                    .style(theme.base())
                    .title(format!(" {} ", preview.title)),
            );
        frame.render_widget(widget, area);
    }
}

fn preview_areas(area: Rect, count: usize) -> Vec<Rect> {
    match count {
        0 => Vec::new(),
        1 => vec![area],
        2 => Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
            .to_vec(),
        _ => {
            let rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let mut areas = Vec::with_capacity(4);
            for row in rows.iter() {
                areas.extend(
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(*row)
                        .iter()
                        .copied(),
                );
            }
            areas.truncate(count.min(4));
            areas
        }
    }
}

fn legend_actions(mode: InputMode, source: &str) -> Vec<(&'static str, &'static str)> {
    match (mode, source) {
        (InputMode::Insert, "sessions") => vec![
            ("Esc", "Normal mode"),
            ("↑/↓", "Select"),
            ("Tab", "Expand/collapse session"),
            ("Shift+Tab", "Expand/collapse all sessions"),
            ("Enter", "Open"),
            ("Ctrl-C", "Close"),
        ],
        (InputMode::Normal, "sessions") => vec![
            ("j/k", "Select"),
            ("Tab", "Expand/collapse session"),
            ("Shift+Tab", "Expand/collapse all sessions"),
            ("i", "Search"),
            ("Enter", "Open"),
            ("q", "Close"),
        ],
        (InputMode::Insert, _) => vec![
            ("Esc", "Normal mode"),
            ("↑/↓", "Select"),
            ("Enter", "Open"),
            ("Ctrl-C", "Close"),
        ],
        (InputMode::Normal, _) => vec![
            ("j/k", "Select"),
            ("i", "Search"),
            ("Enter", "Open"),
            ("q", "Close"),
        ],
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect, theme: Theme, source: &str) {
    frame.render_widget(
        Paragraph::new(legend(legend_actions(app.mode, source), theme)),
        area,
    );
}

fn legend(actions: Vec<(&'static str, &'static str)>, theme: Theme) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, action)) in actions.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                "  ·  ",
                theme.text().add_modifier(Modifier::DIM),
            ));
        }
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(theme.selection_foreground)
                .bg(theme.selection_background)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {action}"),
            theme.text().add_modifier(Modifier::DIM),
        ));
    }
    Line::from(spans)
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;
    Ok(terminal)
}

fn set_cursor_style(mode: InputMode) -> Result<()> {
    let style = match mode {
        InputMode::Insert => SetCursorStyle::BlinkingBar,
        InputMode::Normal => SetCursorStyle::SteadyBlock,
    };
    execute!(io::stdout(), style)?;
    Ok(())
}

struct TerminalCleanup;

impl Drop for TerminalCleanup {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            SetCursorStyle::DefaultUserShape,
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_resets_use_picker_colors_without_changing_explicit_colors() {
        let base = Style::default()
            .fg(Color::Rgb(248, 248, 242))
            .bg(Color::Rgb(40, 42, 54));
        let text = preview_text(
            "\x1b[31;48;2;10;20;30mexplicit\x1b[39;49mdefault\x1b[1mbold\x1b[0mreset",
            base,
        )
        .unwrap();
        let spans = &text.lines[0].spans;
        assert_eq!(spans[0].style.fg, Some(Color::Red));
        assert_eq!(spans[0].style.bg, Some(Color::Rgb(10, 20, 30)));
        for span in &spans[1..] {
            assert_eq!(span.style.fg, base.fg);
            assert_eq!(span.style.bg, base.bg);
        }
        assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
        assert!(!spans[3].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn preview_defaults_follow_light_picker_colors() {
        let base = Style::default()
            .fg(Color::Rgb(30, 30, 30))
            .bg(Color::Rgb(245, 245, 245));
        let text = preview_text("\x1b[0mlight\n\x1b[44mblue\x1b[49mdefault", base).unwrap();
        assert_eq!(text.lines[0].spans[0].style.fg, base.fg);
        assert_eq!(text.lines[0].spans[0].style.bg, base.bg);
        assert_eq!(text.lines[1].spans[0].style.bg, Some(Color::Blue));
        assert_eq!(text.lines[1].spans[1].style.bg, base.bg);
    }
}
