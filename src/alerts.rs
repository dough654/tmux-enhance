use std::{
    env,
    fs::OpenOptions,
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    notify,
    store::{AgentReport, Store},
    theme::Theme,
    tmux::{Tmux, WindowTarget},
};

pub fn report(
    pane_id: &str,
    provider: &str,
    session_id: &str,
    status: &str,
    title: &str,
) -> Result<()> {
    let store = Store::open()?;
    let transition = store.report_agent(pane_id, provider, session_id, status, title)?;
    if transition.changed {
        let _ = handle_transition(&store, &transition.report);
    }
    Ok(())
}

pub fn jump_latest() -> Result<()> {
    let tmux = Tmux::from_env();
    let store = Store::open()?;
    let reports = store.agent_reports()?;
    let windows = tmux.windows()?;

    for alert in store.agent_alerts()? {
        let report_is_current = reports.get(&alert.pane_id).is_some_and(|report| {
            report.provider == alert.provider
                && report.session_id == alert.session_id
                && report.status == alert.status
                && needs_attention(&report.status)
        });
        let window = windows.iter().find(|window| {
            window.pane_id == alert.pane_id && is_agent_window(window, &alert.provider)
        });
        if !report_is_current || window.is_none() {
            store.acknowledge_agent_alert(&alert.pane_id)?;
            continue;
        }

        let window = window.expect("checked above");
        tmux.switch_to_pane(window)?;
        store.touch_pane(&alert.pane_id)?;
        store.acknowledge_agent_alert(&alert.pane_id)?;
        tmux.close_floating_alerts(Some(&alert.pane_id))?;
        return Ok(());
    }

    notify::show("No agents need attention")
}

pub fn jump_to(pane_id: &str) -> Result<()> {
    let tmux = Tmux::from_env();
    let store = Store::open()?;
    let window = tmux
        .windows()?
        .into_iter()
        .find(|window| window.pane_id == pane_id && is_any_agent_window(window));
    let Some(window) = window else {
        store.acknowledge_agent_alert(pane_id)?;
        tmux.close_floating_alerts(Some(pane_id))?;
        return notify::show("Agent is no longer available");
    };

    tmux.switch_to_pane(&window)?;
    store.touch_pane(pane_id)?;
    store.acknowledge_agent_alert(pane_id)?;
    tmux.close_floating_alerts(Some(pane_id))
}

fn is_any_agent_window(window: &WindowTarget) -> bool {
    window.command == "claude"
        || window.command == "opencode"
        || window.pane_title.starts_with("Claude")
        || window.pane_title.starts_with("claude")
        || window.pane_title.starts_with("OC | ")
}

fn handle_transition(store: &Store, report: &AgentReport) -> Result<()> {
    let tmux = Tmux::from_env();
    if !needs_attention(&report.status) {
        tmux.close_floating_alerts(Some(&report.pane_id))?;
    }
    let focused = tmux
        .active_client_panes()
        .unwrap_or_default()
        .iter()
        .any(|pane| pane == &report.pane_id);
    if focused {
        if needs_attention(&report.status) {
            store.acknowledge_agent_alert(&report.pane_id)?;
        }
        tmux.close_floating_alerts(Some(&report.pane_id))?;
        return Ok(());
    }

    let ready_enabled = option_enabled(&tmux.option("@enhance-alert-ready").unwrap_or_default());
    if !(needs_attention(&report.status) || report.status == "ready" && ready_enabled) {
        return Ok(());
    }
    if !option_enabled_with_default(&tmux.option("@enhance-alerts").unwrap_or_default(), true) {
        return Ok(());
    }

    let window = tmux
        .windows()?
        .into_iter()
        .find(|window| window.pane_id == report.pane_id);
    let location = window
        .as_ref()
        .map(|window| format!("{}/{}", window.session_name, window.window_name))
        .unwrap_or_else(|| report.pane_id.clone());
    let provider = provider_label(&report.provider);
    let title = if report.title.trim().is_empty() {
        location.clone()
    } else {
        report.title.trim().to_string()
    };
    if option_enabled_with_default(
        &tmux.option("@enhance-alert-overlay").unwrap_or_default(),
        true,
    ) {
        show_floating_alert(&tmux, report, &location)?;
    }

    if option_enabled(&tmux.option("@enhance-alert-bell").unwrap_or_default()) {
        ring_client_bells(&tmux);
    }
    if option_enabled(&tmux.option("@enhance-alert-native").unwrap_or_default()) {
        show_native_notification(
            &format!("{provider} · {}", report.status.to_uppercase()),
            &format!("{location} · {title}"),
        );
    }
    Ok(())
}

fn is_agent_window(window: &WindowTarget, provider: &str) -> bool {
    match provider {
        "claude" => {
            window.command == "claude"
                || window.pane_title.starts_with("Claude")
                || window.pane_title.starts_with("claude")
        }
        "opencode" => window.command == "opencode" || window.pane_title.starts_with("OC | "),
        _ => false,
    }
}

fn needs_attention(status: &str) -> bool {
    matches!(status, "permission" | "input")
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "claude" => "Claude Code",
        "opencode" => "OpenCode",
        other => other,
    }
}

fn option_enabled(value: &str) -> bool {
    option_enabled_with_default(value, false)
}

fn option_enabled_with_default(value: &str, default: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => true,
        "0" | "off" | "false" | "no" => false,
        _ => default,
    }
}

fn ring_client_bells(tmux: &Tmux) {
    for tty in tmux.client_ttys().unwrap_or_default() {
        if let Ok(mut terminal) = OpenOptions::new().write(true).open(tty) {
            let _ = terminal.write_all(b"\x07");
        }
    }
}

fn show_floating_alert(tmux: &Tmux, report: &AgentReport, location: &str) -> Result<()> {
    tmux.close_floating_alerts(None)?;
    let requested_width = option_number(tmux, "@enhance-alert-width", 50, 32, 100);
    let requested_height = option_number(tmux, "@enhance-alert-height", 6, 6, 20);
    let duration = option_number(tmux, "@enhance-alert-duration", 15, 2, 60);
    let executable = env::current_exe().context("locating tmux-enhance")?;
    let (style, border_style) = Theme::from_tmux(tmux).agent_overlay_styles(&report.status);
    let mut targets = tmux.active_client_panes().unwrap_or_default();
    targets.sort();
    targets.dedup();

    for target in targets {
        let Ok((window_width, window_height)) = tmux.window_dimensions(&target) else {
            continue;
        };
        if window_width < 34 || window_height < 8 {
            continue;
        }
        let width = requested_width.min(window_width.saturating_sub(2));
        let height = requested_height.min(window_height.saturating_sub(2));
        let x = window_width.saturating_sub(width).saturating_sub(1);
        let command = format!(
            "{} alert-overlay --status {} --location {} --width {} --duration {}",
            shell_quote(&executable),
            shell_quote_value(&report.status),
            shell_quote_value(location),
            width,
            duration
        );
        tmux.create_floating_alert(
            &target,
            &report.pane_id,
            width,
            height,
            x,
            1,
            &style,
            &border_style,
            &command,
        )?;
    }
    Ok(())
}

fn option_number(tmux: &Tmux, option: &str, default: u16, minimum: u16, maximum: u16) -> u16 {
    tmux.option(option)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

pub fn run_overlay(status: &str, location: &str, width: u16, duration: u16) -> Result<()> {
    let mut stdout = std::io::stdout();
    for remaining in (1..=duration).rev() {
        stdout.write_all(render_overlay(status, location, width, remaining).as_bytes())?;
        stdout.flush()?;
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

fn render_overlay(status: &str, location: &str, width: u16, remaining: u16) -> String {
    let content_width = usize::from(width.saturating_sub(4));
    let sentence = match status {
        "permission" => format!("Agent in {location} is asking for permission"),
        "input" => format!("Agent in {location} needs your input"),
        "ready" => format!("Agent in {location} has finished processing"),
        _ => format!("Agent in {location} is processing"),
    };
    let sentence = wrap_to_width(&sentence, content_width)
        .into_iter()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n  ");
    let footer = truncate_to_width(
        &format!("Click or prefix + Enter to jump · {remaining}s"),
        content_width,
    );
    format!("\x1b[2J\x1b[H\n  \x1b[1m{sentence}\x1b[0m\n\n  {footer}")
}

fn wrap_to_width(value: &str, maximum_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if candidate.width() <= maximum_width {
            line = candidate;
        } else {
            if !line.is_empty() {
                lines.push(line);
            }
            line = truncate_to_width(word, maximum_width);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn truncate_to_width(value: &str, maximum_width: usize) -> String {
    let value = value
        .replace(['\n', '\r'], " ")
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if value.width() <= maximum_width {
        return value;
    }
    let mut width = 0;
    let mut result = value
        .chars()
        .take_while(|character| {
            let character_width = character.width().unwrap_or_default();
            if width + character_width + 1 > maximum_width {
                return false;
            }
            width += character_width;
            true
        })
        .collect::<String>();
    result.push('…');
    result
}

fn shell_quote(path: &Path) -> String {
    shell_quote_value(&path.to_string_lossy())
}

fn shell_quote_value(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn show_native_notification(summary: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args([
            "--app-name=tmux-enhance",
            "--urgency=critical",
            summary,
            body,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tmux_boolean_options() {
        for enabled in ["on", "true", "1", "YES"] {
            assert!(option_enabled(enabled));
        }
        for disabled in ["off", "false", "0", "no", ""] {
            assert!(!option_enabled(disabled));
        }
    }

    #[test]
    fn identifies_attention_statuses() {
        assert!(needs_attention("permission"));
        assert!(needs_attention("input"));
        assert!(!needs_attention("ready"));
        assert!(!needs_attention("processing"));
    }

    #[test]
    fn renders_a_bounded_overlay() {
        let rendered = render_overlay("input", "project/agent", 48, 8);
        assert!(rendered.contains("Agent in project/agent needs your input"));
        assert!(rendered.contains("Click or prefix + Enter to jump · 8s"));
    }

    #[test]
    fn describes_each_user_facing_status() {
        assert!(
            render_overlay("permission", "work/agent", 58, 8)
                .contains("Agent in work/agent is asking for permission")
        );
        assert!(
            render_overlay("ready", "work/agent", 58, 8)
                .contains("Agent in work/agent has finished processing")
        );
    }

    #[test]
    fn truncates_long_overlay_lines() {
        let truncated = truncate_to_width("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(truncated.width(), 10);
        assert!(truncated.ends_with('…'));
    }
}
