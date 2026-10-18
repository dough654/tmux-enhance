pub mod agents;
pub mod sessions;
pub mod windows;

use anyhow::{Context, Result};

use crate::{PickerSource, theme::Theme, tmux::Tmux};

pub fn popup(source: PickerSource) -> Result<()> {
    let tmux = Tmux::from_env();
    let (style, border_style) = Theme::from_tmux(&tmux).picker_popup_styles();
    let width = tmux.option("@enhance-picker-width")?;
    let height = tmux.option("@enhance-picker-height")?;
    let exe = std::env::current_exe().context("locating picker executable")?;
    let exe = exe
        .to_str()
        .context("picker executable path is not UTF-8")?;
    let source = match source {
        PickerSource::Windows => "windows",
        PickerSource::Harpoon => "harpoon",
        PickerSource::Sessions => "sessions",
        PickerSource::Agents => "agents",
    };
    let command = format!("{} picker {source}", shell_quote(exe));
    tmux.display_picker_popup(
        if width.is_empty() { "90%" } else { &width },
        if height.is_empty() { "80%" } else { &height },
        &style,
        &border_style,
        &command,
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_executable_paths() {
        for path in ["", "/tmp/picker", "/tmp/it's a picker/$HOME;`id`\n"] {
            let output = std::process::Command::new("sh")
                .args(["-c", &format!("printf '%s' {}", shell_quote(path))])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, path.as_bytes());
        }
    }
}
