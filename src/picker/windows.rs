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
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{
        App, InputMode, PickerAction, PickerEntry, PickerHierarchy, PreviewPane, RenderedPreview,
    },
    store::Store,
    theme::Theme,
    tmux::Tmux,
};

const PREVIEW_REFRESH: Duration = Duration::from_millis(500);

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
    let tmux = Tmux::from_env();
    let theme = Theme::from_tmux(&tmux);
    let mut app = App::new(entries);
    refresh_preview(&tmux, &mut app, 80);

    let mut terminal = setup_terminal()?;
    let _cleanup = TerminalCleanup;
    let result = run_loop(&mut terminal, &tmux, &mut app, theme, source, list_title);
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
) -> Result<()> {
    let mut last_preview = Instant::now();
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
                let preview_height = terminal.size()?.height.saturating_sub(5);
                refresh_preview(tmux, app, preview_height);
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
            let preview_height = terminal.size()?.height.saturating_sub(5);
            refresh_preview(tmux, app, preview_height);
            last_preview = Instant::now();
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
            KeyCode::Down => app.move_next(),
            KeyCode::Up => app.move_previous(),
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
                KeyCode::Char('g') if completing_gg => app.move_first(),
                KeyCode::Char('g') => app.pending_g = true,
                KeyCode::Char('d') if completing_dd => app.should_delete = true,
                KeyCode::Char('d') if allow_delete => app.pending_d = true,
                KeyCode::Char('j') | KeyCode::Down => app.move_next(),
                KeyCode::Char('k') | KeyCode::Up => app.move_previous(),
                KeyCode::Char('h') | KeyCode::Left => app.move_cursor_left(),
                KeyCode::Char('l') | KeyCode::Right => app.move_cursor_right(),
                KeyCode::Char('G') => app.move_last(),
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

fn refresh_preview(tmux: &Tmux, app: &mut App, height: u16) {
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
                .and_then(|preview| preview.into_text().map_err(Into::into))
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

fn render(frame: &mut Frame, app: &mut App, theme: Theme, source: &str, list_title: &str) {
    frame.render_widget(Block::default().style(theme.base()), frame.area());
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_search(frame, app, outer[0], theme, source);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(outer[1]);
    render_items(frame, app, columns[0], theme, list_title);
    render_preview(frame, app, columns[1], theme);
    render_footer(frame, app, outer[2], theme, source);
}

fn render_search(frame: &mut Frame, app: &App, area: Rect, theme: Theme, source: &str) {
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
            .title_style(theme.text().add_modifier(Modifier::BOLD))
            .style(theme.base())
            .title(format!(" tmux-enhance / {source} ")),
    );
    frame.render_widget(query, area);

    let cursor_offset = app.query[..app.cursor_byte].width() as u16;
    let x = (area.x + 11 + cursor_offset).min(area.right().saturating_sub(2));
    frame.set_cursor_position(Position::new(x, area.y + 1));
}

fn render_items(frame: &mut Frame, app: &mut App, area: Rect, theme: Theme, title: &str) {
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
            if let Some(PickerHierarchy::Parent { expanded, .. }) = &item.hierarchy {
                spans.push(Span::styled(
                    if *expanded { "▾ " } else { "▸ " },
                    Style::default().fg(theme.accent),
                ));
            }
            spans.extend(highlighted_spans(&item.display_text, matched, theme));
            let list_item = ListItem::new(Line::from(spans));
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

fn highlighted_spans(text: &str, matched: &[usize], theme: Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut segment = String::new();
    let mut segment_is_match = false;

    for (index, character) in text.chars().enumerate() {
        let is_match = matched.binary_search(&index).is_ok();
        if !segment.is_empty() && is_match != segment_is_match {
            let style = if segment_is_match {
                theme.match_style()
            } else {
                theme.text()
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
            theme.text()
        };
        spans.push(Span::styled(segment, style));
    }
    spans
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

fn render_footer(frame: &mut Frame, app: &App, area: Rect, theme: Theme, source: &str) {
    let actions = match (app.mode, source) {
        (InputMode::Insert, "sessions") => vec![
            ("Esc", "Normal mode"),
            ("↑/↓", "Select"),
            ("Tab", "Toggle session"),
            ("Shift+Tab", "Toggle all sessions"),
            ("Enter", "Open"),
            ("Ctrl-C", "Close"),
        ],
        (InputMode::Normal, "sessions") => vec![
            ("j/k", "Select"),
            ("Tab", "Toggle session"),
            ("Shift+Tab", "Toggle all sessions"),
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
    };
    frame.render_widget(Paragraph::new(legend(actions, theme)), area);
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
