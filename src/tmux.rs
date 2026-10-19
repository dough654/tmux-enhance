use std::{collections::HashMap, process::Command};

use anyhow::{Context, Result, bail};

const FIELD_SEPARATOR: char = '\u{1f}';
const RECORD_SEPARATOR: char = '\u{1e}';

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

#[derive(Debug, PartialEq, Eq)]
struct ClientContext {
    name: String,
    session_name: String,
    pane_id: String,
}

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
        Ok(top_lines(&output, lines))
    }

    pub fn option(&self, name: &str) -> Result<String> {
        self.output(["show-options", "-gqv", name])
    }

    pub fn options(&self, names: &[&str]) -> Result<HashMap<String, String>> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }
        // Explicit markers preserve raw multiline values without requiring show-options -F.
        let mut args = Vec::new();
        for name in names {
            if !args.is_empty() {
                args.push(";".to_string());
            }
            args.extend([
                "display-message".to_string(),
                "-p".to_string(),
                format!("{RECORD_SEPARATOR}{name}{FIELD_SEPARATOR}"),
                ";".to_string(),
                "show-options".to_string(),
                "-gqv".to_string(),
                name.to_string(),
            ]);
        }
        Ok(parse_options(&self.output(args)?))
    }

    pub fn option_raw(&self, name: &str) -> Result<String> {
        self.output_preserving(["show-options", "-gqv", name])
    }

    pub fn current_session_name(&self) -> Result<String> {
        if let Some(context) = self.current_client_context()? {
            return Ok(context.session_name);
        }
        self.output(["display-message", "-p", "#{session_name}"])
    }

    pub fn current_pane_id(&self) -> Result<String> {
        if let Some(context) = self.current_client_context()? {
            return Ok(context.pane_id);
        }
        self.output(["display-message", "-p", "#{pane_id}"])
    }

    pub fn active_client_panes(&self) -> Result<Vec<String>> {
        Ok(self
            .output(["list-clients", "-F", "#{pane_id}"])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn client_ttys(&self) -> Result<Vec<String>> {
        Ok(self
            .output(["list-clients", "-F", "#{client_tty}"])?
            .lines()
            .map(str::to_string)
            .collect())
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

    pub fn display_picker_popup(
        &self,
        width: &str,
        height: &str,
        style: &str,
        border_style: &str,
        command: &str,
    ) -> Result<()> {
        let context = self
            .current_client_context()?
            .ok_or_else(|| anyhow::anyhow!("tmux did not identify the originating client"))?;
        let client = context.name;
        let client_env = format!("TMUX_ENHANCE_CLIENT={client}");
        self.run([
            "display-popup",
            "-EE",
            "-c",
            &client,
            "-t",
            &context.pane_id,
            "-e",
            &client_env,
            "-w",
            width,
            "-h",
            height,
            "-s",
            style,
            "-S",
            border_style,
            "-d",
            "#{pane_current_path}",
            command,
        ])
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

    pub fn switch_to_pane(&self, window: &WindowTarget) -> Result<()> {
        let client = self.current_client()?;
        self.run([
            "switch-client",
            "-c",
            &client,
            "-t",
            &window.session_id,
            ";",
            "select-window",
            "-t",
            &window.window_id,
            ";",
            "select-pane",
            "-t",
            &window.pane_id,
        ])
    }

    pub fn window_dimensions(&self, pane_id: &str) -> Result<(u16, u16)> {
        let separator = FIELD_SEPARATOR.to_string();
        let format = ["#{window_width}", "#{window_height}"].join(&separator);
        let output = self.output(["display-message", "-p", "-t", pane_id, &format])?;
        let Some((width, height)) = output.split_once(FIELD_SEPARATOR) else {
            bail!("unexpected tmux window dimensions");
        };
        Ok((
            width.parse().context("parsing tmux window width")?,
            height.parse().context("parsing tmux window height")?,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_floating_alert(
        &self,
        target_pane: &str,
        source_pane: &str,
        width: u16,
        height: u16,
        x: u16,
        y: u16,
        style: &str,
        border_style: &str,
        command: &str,
    ) -> Result<()> {
        let width = width.to_string();
        let height = height.to_string();
        let x = x.to_string();
        let y = y.to_string();
        let pane_id = self.output([
            "new-pane",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            target_pane,
            "-x",
            &width,
            "-y",
            &height,
            "-X",
            &x,
            "-Y",
            &y,
            "-s",
            style,
            "-S",
            border_style,
            "-R",
            border_style,
            command,
        ])?;
        self.run([
            "set-option",
            "-pt",
            &pane_id,
            "@enhance-alert-source",
            source_pane,
        ])?;
        self.run(["select-pane", "-t", &pane_id, "-T", "tmux-enhance alert"])
    }

    pub fn close_floating_alerts(&self, source_pane: Option<&str>) -> Result<()> {
        let separator = FIELD_SEPARATOR.to_string();
        let format = ["#{pane_id}", "#{@enhance-alert-source}"].join(&separator);
        let output = self.output(["list-panes", "-a", "-F", &format])?;
        for line in output.lines() {
            let Some((pane_id, source)) = line.split_once(FIELD_SEPARATOR) else {
                continue;
            };
            if !source.is_empty() && source_pane.is_none_or(|expected| expected == source) {
                let _ = self.run(["kill-pane", "-t", pane_id]);
            }
        }
        Ok(())
    }

    fn current_client(&self) -> Result<String> {
        self.current_client_context()?
            .map(|context| context.name)
            .ok_or_else(|| anyhow::anyhow!("tmux did not identify the originating client"))
    }

    fn current_client_context(&self) -> Result<Option<ClientContext>> {
        // Background launchers must retain the client that invoked the binding.
        let client_name = match std::env::var("TMUX_ENHANCE_CLIENT") {
            Ok(name) if !name.is_empty() => name,
            _ => self.output(["display-message", "-p", "#{client_name}"])?,
        };
        if client_name.is_empty() {
            return Ok(None);
        }

        let separator = FIELD_SEPARATOR.to_string();
        let format = ["#{client_name}", "#{client_session}", "#{pane_id}"].join(&separator);
        let clients = self.output(["list-clients", "-F", &format])?;
        Ok(parse_client_contexts(&clients)
            .into_iter()
            .find(|context| context.name == client_name))
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

fn parse_options(output: &str) -> HashMap<String, String> {
    output
        .split(RECORD_SEPARATOR)
        .filter_map(|record| {
            let (name, value) = record
                .trim_start_matches('\n')
                .split_once(FIELD_SEPARATOR)?;
            let value = value.strip_prefix('\n').unwrap_or(value).trim_end();
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

fn top_lines(output: &str, lines: u16) -> String {
    output
        .lines()
        .take(usize::from(lines))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_client_contexts(output: &str) -> Vec<ClientContext> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(FIELD_SEPARATOR);
            let context = ClientContext {
                name: fields.next()?.to_string(),
                session_name: fields.next()?.to_string(),
                pane_id: fields.next()?.to_string(),
            };
            fields.next().is_none().then_some(context)
        })
        .collect()
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
    fn parses_batched_options_without_unquoting_or_expanding_values() {
        let output = concat!(
            "\x1estatus-style\x1f\nbg=#282c34,fg=colour231\n",
            "\x1estatus-left\x1f\n\n  #[fg=#50fa7b] \"quoted\" \\ #{session_name}\nnext line\n",
            "\x1e@empty\x1f\n\n",
            "\x1e@enhance-color-accent\x1f\n#61afef\n",
            "\x1e@enhance-color-background\x1f\n",
        );
        let options = parse_options(output);
        assert_eq!(options.len(), 5);
        assert_eq!(options["status-style"], "bg=#282c34,fg=colour231");
        assert_eq!(
            options["status-left"],
            "\n  #[fg=#50fa7b] \"quoted\" \\ #{session_name}\nnext line"
        );
        assert_eq!(options["@empty"], "");
        assert_eq!(options["@enhance-color-accent"], "#61afef");
        assert_eq!(options["@enhance-color-background"], "");
        assert!(parse_options("").is_empty());
    }

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

    #[test]
    fn previews_the_top_of_the_visible_pane() {
        let output = "editor header\nimportant content\nblank lower area\n";
        assert_eq!(top_lines(output, 2), "editor header\nimportant content");
    }

    #[test]
    fn parses_client_session_and_active_pane_context() {
        let separator = FIELD_SEPARATOR;
        let output = format!(
            "/dev/pts/4{separator}acme-store{separator}%12\n/dev/pts/7{separator}orders-api{separator}%19"
        );
        assert_eq!(
            parse_client_contexts(&output),
            vec![
                ClientContext {
                    name: "/dev/pts/4".into(),
                    session_name: "acme-store".into(),
                    pane_id: "%12".into(),
                },
                ClientContext {
                    name: "/dev/pts/7".into(),
                    session_name: "orders-api".into(),
                    pane_id: "%19".into(),
                },
            ]
        );
    }
}
