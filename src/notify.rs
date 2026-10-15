use std::{env, path::Path, time::Duration};

use anyhow::{Context, Result};

use crate::{theme::Theme, tmux::Tmux};

const ORIGINAL_OPTION: &str = "@enhance-toast-original";
const TOKEN_OPTION: &str = "@enhance-toast-token";

pub fn show(message: &str) -> Result<()> {
    let tmux = Tmux::from_env();
    if tmux.option(TOKEN_OPTION)?.is_empty() {
        let original = tmux.option("status-right")?;
        tmux.set_option(ORIGINAL_OPTION, &original)?;
    }

    let token = format!("{}-{}", std::process::id(), timestamp());
    let theme = Theme::from_tmux(&tmux);
    let toast = format!(
        "#[{}] {} #[default]",
        theme.toast_style(),
        escape_format(message)
    );
    tmux.set_option(TOKEN_OPTION, &token)?;
    tmux.set_option("status-right", &toast)?;

    let duration = tmux
        .option("@enhance-toast-duration")?
        .parse::<u64>()
        .unwrap_or(4)
        .clamp(1, 30);
    let executable = env::current_exe().context("locating tmux-enhance")?;
    let command = format!(
        "sleep {duration}; {} toast-clear {}",
        shell_quote(&executable),
        shell_quote_value(&token)
    );
    tmux.run_shell_background(&command)
}

pub fn clear(token: &str) -> Result<()> {
    let tmux = Tmux::from_env();
    if tmux.option(TOKEN_OPTION)? != token {
        return Ok(());
    }
    let original = tmux.option(ORIGINAL_OPTION)?;
    tmux.set_option("status-right", &original)?;
    tmux.unset_option(TOKEN_OPTION)?;
    tmux.unset_option(ORIGINAL_OPTION)
}

fn timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

fn escape_format(value: &str) -> String {
    value
        .replace('#', "##")
        .replace(['\n', '\r'], " ")
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn shell_quote(path: &Path) -> String {
    shell_quote_value(&path.to_string_lossy())
}

fn shell_quote_value(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_tmux_formats_and_control_characters() {
        assert_eq!(escape_format("done #1\nnext"), "done ##1 next");
    }

    #[test]
    fn quotes_shell_values() {
        assert_eq!(shell_quote_value("it's ready"), "'it'\\''s ready'");
    }
}
