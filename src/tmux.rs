use std::process::Command;

use anyhow::{Context, Result, bail};

const FIELD_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTarget {
    pub session_id: String,
    pub session_name: String,
    pub window_id: String,
    pub window_index: u32,
    pub window_name: String,
    pub pane_id: String,
    pub pane_title: String,
    pub command: String,
    pub current_path: String,
    pub current: bool,
    pub session_active_window: bool,
    pub session_activity: i64,
    pub session_windows: u32,
    pub attached_clients: u32,
    pub window_activity: i64,
}

impl WindowTarget {
    pub fn searchable_text(&self) -> String {
        format!(
            "{}:{}  {}  {}  {}",
            self.session_name, self.window_index, self.window_name, self.command, self.current_path
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tmux;

impl Tmux {
    pub fn from_env() -> Self {
        Self
    }

    pub fn version(&self) -> Result<String> {
        self.output(["-V"])
    }

    pub fn windows(&self) -> Result<Vec<WindowTarget>> {
        let separator = FIELD_SEPARATOR.to_string();
        let format = [
            "#{session_id}",
            "#{session_name}",
            "#{window_id}",
            "#{window_index}",
            "#{window_name}",
            "#{pane_id}",
            "#{pane_title}",
            "#{pane_current_command}",
            "#{pane_current_path}",
            "#{&&:#{session_attached},#{window_active}}",
            "#{window_active}",
            "#{session_activity}",
            "#{session_windows}",
            "#{session_attached}",
            "#{window_activity}",
        ]
        .join(&separator);

        let output = self.output(["list-panes", "-a", "-f", "#{pane_active}", "-F", &format])?;
        output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(parse_window)
            .collect()
    }

    pub fn capture_preview(&self, pane_id: &str, lines: u16) -> Result<String> {
        let output = self.output_preserving(["capture-pane", "-e", "-p", "-t", pane_id])?;
        let line_count = output.lines().count();
        let skip = line_count.saturating_sub(lines as usize);
        Ok(output.lines().skip(skip).collect::<Vec<_>>().join("\n"))
    }

    pub fn option(&self, name: &str) -> Result<String> {
        self.output(["show-options", "-gqv", name])
    }

    pub fn current_session_name(&self) -> Result<String> {
        self.output(["display-message", "-p", "#{session_name}"])
    }

    pub fn current_pane_id(&self) -> Result<String> {
        self.output(["display-message", "-p", "#{pane_id}"])
    }

    pub fn switch_to_session(&self, session_name: &str) -> Result<()> {
        let client = self.current_client()?;
        self.run(["switch-client", "-c", &client, "-t", session_name])
    }

    pub fn set_option(&self, name: &str, value: &str) -> Result<()> {
        self.run(["set-option", "-g", name, value])
    }

    pub fn unset_option(&self, name: &str) -> Result<()> {
        self.run(["set-option", "-gu", name])
    }

    pub fn run_shell_background(&self, command: &str) -> Result<()> {
        self.run(["run-shell", "-b", command])
    }

    pub fn session_exists(&self, session_name: &str) -> Result<bool> {
        let status = Command::new("tmux")
            .args(["has-session", "-t", session_name])
            .status()
            .context("running tmux")?;
        Ok(status.success())
    }

    pub fn switch_to_window(&self, session_id: &str, window_index: u32) -> Result<()> {
        let client = self.current_client()?;

        let qualified_window = format!("{session_id}:{window_index}");
        self.run([
            "select-window",
            "-t",
            &qualified_window,
            ";",
            "switch-client",
            "-c",
            &client,
            "-t",
            session_id,
        ])
    }

    fn current_client(&self) -> Result<String> {
        let client = self.output(["display-message", "-p", "#{client_name}"])?;
        if client.is_empty() {
            bail!("tmux did not identify the originating client");
        }
        Ok(client)
    }

    fn output<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .context("running tmux")?;
        if !output.status.success() {
            bail!("tmux: {}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string())
    }

    fn run<I, S>(&self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let status = Command::new("tmux")
            .args(args)
            .status()
            .context("running tmux")?;
        if !status.success() {
            bail!("tmux command exited with {status}");
        }
        Ok(())
    }

    fn output_preserving<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .context("running tmux")?;
        if !output.status.success() {
            bail!("tmux: {}", String::from_utf8_lossy(&output.stderr).trim());
        }
        let output = String::from_utf8_lossy(&output.stdout);
        Ok(output.strip_suffix('\n').unwrap_or(&output).to_string())
    }
}

fn parse_window(line: &str) -> Result<WindowTarget> {
    let fields: Vec<&str> = line.split(FIELD_SEPARATOR).collect();
    if fields.len() != 15 {
        bail!("unexpected tmux window record with {} fields", fields.len());
    }
    Ok(WindowTarget {
        session_id: fields[0].to_string(),
        session_name: fields[1].to_string(),
        window_id: fields[2].to_string(),
        window_index: fields[3].parse().context("parsing tmux window index")?,
        window_name: fields[4].to_string(),
        pane_id: fields[5].to_string(),
        pane_title: fields[6].to_string(),
        command: fields[7].to_string(),
        current_path: fields[8].to_string(),
        current: fields[9] == "1",
        session_active_window: fields[10] == "1",
        session_activity: fields[11].parse().context("parsing session activity")?,
        session_windows: fields[12].parse().context("parsing session window count")?,
        attached_clients: fields[13]
            .parse()
            .context("parsing attached client count")?,
        window_activity: fields[14].parse().context("parsing window activity")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_delimiter_safe_window_record() {
        let separator = FIELD_SEPARATOR.to_string();
        let line = [
            "$1",
            "project one",
            "@4",
            "2",
            "agent window",
            "%8",
            "OC | Review",
            "opencode",
            "/home/example/project one",
            "1",
            "1",
            "1700000000",
            "4",
            "2",
            "1700000001",
        ]
        .join(&separator);

        let parsed = parse_window(&line).unwrap();
        assert_eq!(parsed.session_name, "project one");
        assert_eq!(parsed.window_name, "agent window");
        assert!(parsed.current);
        assert!(parsed.session_active_window);
        assert_eq!(parsed.session_windows, 4);
        assert_eq!(parsed.attached_clients, 2);
    }
}
