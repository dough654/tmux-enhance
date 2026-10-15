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
    app::{App, InputMode},
    theme::Theme,
    tmux::Tmux,
};

const PREVIEW_REFRESH: Duration = Duration::from_millis(500);

pub fn run() -> Result<()> {
    let tmux = Tmux::from_env();
    let theme = Theme::from_tmux(&tmux);
    let mut app = App::new(tmux.windows()?);
    refresh_preview(&tmux, &mut app, 80);

    let mut terminal = setup_terminal()?;
    let _cleanup = TerminalCleanup;
    let result = run_loop(&mut terminal, &tmux, &mut app, theme);
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tmux: &Tmux,
    app: &mut App,
    theme: Theme,
) -> Result<()> {
    let mut last_preview = Instant::now();
    let mut cursor_mode = None;

    loop {
        if cursor_mode != Some(app.mode) {
            set_cursor_style(app.mode)?;
            cursor_mode = Some(app.mode);
        }
        terminal.draw(|frame| render(frame, app, theme))?;
        if app.should_quit {
            return Ok(());
        }
        if app.should_open {
            if let Some(item) = app.selected_item() {
                tmux.switch_to_window(item)?;
            }
            return Ok(());
        }

        if event::poll(Duration::from_millis(75))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key);
        }

        if app.preview_dirty || last_preview.elapsed() >= PREVIEW_REFRESH {
            let preview_height = terminal.size()?.height.saturating_sub(5);
            refresh_preview(tmux, app, preview_height);
            last_preview = Instant::now();
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode {
        InputMode::Insert => match key.code {
            KeyCode::Esc => {
                app.enter_normal_mode();
                app.pending_g = false;
            }
            KeyCode::Enter => app.should_open = true,
            KeyCode::Backspace => app.backspace_query(),
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
            app.pending_g = false;
            match key.code {
                KeyCode::Char('g') if completing_gg => app.move_first(),
                KeyCode::Char('g') => app.pending_g = true,
                KeyCode::Char('j') | KeyCode::Down => app.move_next(),
                KeyCode::Char('k') | KeyCode::Up => app.move_previous(),
                KeyCode::Char('h') | KeyCode::Left => app.move_cursor_left(),
                KeyCode::Char('l') | KeyCode::Right => app.move_cursor_right(),
                KeyCode::Char('G') => app.move_last(),
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
    let Some(pane_id) = app.selected_item().map(|item| item.pane_id.clone()) else {
        app.preview = Default::default();
        app.preview_dirty = false;
        return;
    };
    match tmux.capture_preview(&pane_id, height) {
        Ok(preview) => match preview.into_text() {
            Ok(preview) => {
                app.preview = preview;
                app.error = None;
            }
            Err(error) => app.error = Some(format!("could not parse pane colors: {error}")),
        },
        Err(error) => app.error = Some(error.to_string()),
    }
    app.preview_dirty = false;
}

fn render(frame: &mut Frame, app: &mut App, theme: Theme) {
    frame.render_widget(Block::default().style(theme.base()), frame.area());
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_search(frame, app, outer[0], theme);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(outer[1]);
    render_windows(frame, app, columns[0], theme);
    render_preview(frame, app, columns[1], theme);
    render_footer(frame, app, outer[2], theme);
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
            .title_style(theme.text().add_modifier(Modifier::BOLD))
            .style(theme.base())
            .title(" tmux-enhance / windows "),
    );
    frame.render_widget(query, area);

    let cursor_offset = app.query[..app.cursor_byte].width() as u16;
    let x = (area.x + 11 + cursor_offset).min(area.right().saturating_sub(2));
    frame.set_cursor_position(Position::new(x, area.y + 1));
}

fn render_windows(frame: &mut Frame, app: &mut App, area: Rect, theme: Theme) {
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
            spans.extend(highlighted_spans(&item.searchable_text(), matched, theme));
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
                .title(format!(" Windows ({}) ", app.filtered.len())),
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
    let title = app
        .selected_item()
        .map(|item| format!(" {} / {} ", item.session_name, item.window_name))
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

fn render_footer(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let help = match app.mode {
        InputMode::Insert => " type to filter  esc normal  ↑/↓ select  enter open  ctrl-c close ",
        InputMode::Normal => " j/k select  i search  enter open  q close ",
    };
    frame.render_widget(
        Paragraph::new(help).style(
            theme
                .text()
                .bg(theme.background)
                .add_modifier(Modifier::DIM),
        ),
        area,
    );
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
