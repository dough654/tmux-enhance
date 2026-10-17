mod alerts;
mod app;
mod claude;
mod harpoon;
mod notify;
mod opencode;
mod picker;
mod store;
mod theme;
mod tmux;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

const MIN_TMUX_VERSION: (u32, u32) = (3, 7);

#[derive(Debug, Parser)]
#[command(
    name = "tmux-enhance",
    version,
    about = "Focused navigation and agent awareness for tmux"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open an interactive picker.
    Picker {
        #[command(subcommand)]
        source: PickerSource,
    },
    /// Manage persistent Harpoon session slots.
    Harpoon {
        #[command(subcommand)]
        command: HarpoonCommand,
    },
    /// Check runtime dependencies and tmux connectivity.
    Doctor,
    /// Print discovered agent panes and their status.
    Agents,
    /// Jump to the newest agent that needs attention.
    AlertJump,
    #[command(hide = true)]
    AlertClick { pane_id: String },
    #[command(hide = true)]
    AlertOverlay {
        #[arg(long)]
        status: String,
        #[arg(long)]
        location: String,
        #[arg(long)]
        width: u16,
        #[arg(long)]
        duration: u16,
    },
    /// Manage the bundled OpenCode TUI integration.
    #[command(name = "opencode")]
    OpenCode {
        #[command(subcommand)]
        command: OpenCodeCommand,
    },
    /// Manage the Claude Code hook integration.
    #[command(name = "claude-code")]
    ClaudeCode {
        #[command(subcommand)]
        command: ClaudeCodeCommand,
    },
    /// Manage all coding-agent integrations together.
    Integrations {
        #[command(subcommand)]
        command: IntegrationsCommand,
    },
    #[command(hide = true)]
    ClaudeHook,
    #[command(hide = true)]
    OpenCodeReport {
        #[arg(long)]
        pane_id: String,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        status: String,
        #[arg(long, default_value = "")]
        title: String,
    },
    #[command(hide = true)]
    ToastClear { token: String },
    #[command(hide = true)]
    SessionTouch { session_name: String },
}

#[derive(Debug, Subcommand)]
enum PickerSource {
    /// Search every window on the current tmux server.
    Windows,
    /// Search marked Harpoon sessions.
    Harpoon,
    /// Search tmux sessions with multi-window previews.
    Sessions,
    /// Search OpenCode and Claude Code agent panes.
    Agents,
}

#[derive(Debug, Subcommand)]
enum HarpoonCommand {
    /// Add the current session to the first free slot.
    Add,
    /// Jump to a numbered session slot.
    Jump { slot: u8 },
    /// Remove a numbered session slot.
    Remove { slot: u8 },
    /// Remove the current session from its slot.
    RemoveCurrent,
    /// Print the configured session slots.
    List,
}

#[derive(Debug, Subcommand)]
enum OpenCodeCommand {
    /// Register the bundled TUI integration while preserving existing settings.
    Install {
        #[arg(long)]
        plugin_dir: PathBuf,
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        quiet: bool,
    },
    /// Remove the bundled TUI integration from OpenCode settings.
    Remove {
        #[arg(long)]
        plugin_dir: PathBuf,
    },
    /// Check whether the bundled TUI integration is registered.
    Status {
        #[arg(long)]
        plugin_dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ClaudeCodeCommand {
    /// Register lifecycle hooks while preserving existing Claude Code settings.
    Install {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        quiet: bool,
    },
    /// Remove only the tmux-enhance lifecycle hooks.
    Remove,
    /// Check whether every tmux-enhance lifecycle hook is registered.
    Status,
}

#[derive(Debug, Subcommand)]
enum IntegrationsCommand {
    /// Remove tmux-enhance from OpenCode and Claude Code settings.
    Uninstall {
        /// Path to the bundled OpenCode plugin directory.
        #[arg(long)]
        plugin_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Picker {
            source: PickerSource::Windows,
        } => picker::windows::run(),
        Command::Picker {
            source: PickerSource::Harpoon,
        } => harpoon::picker(),
        Command::Picker {
            source: PickerSource::Sessions,
        } => picker::sessions::run(),
        Command::Picker {
            source: PickerSource::Agents,
        } => picker::agents::run(),
        Command::Harpoon {
            command: HarpoonCommand::Add,
        } => harpoon::add(),
        Command::Harpoon {
            command: HarpoonCommand::Jump { slot },
        } => harpoon::jump(slot),
        Command::Harpoon {
            command: HarpoonCommand::Remove { slot },
        } => harpoon::remove(slot),
        Command::Harpoon {
            command: HarpoonCommand::RemoveCurrent,
        } => harpoon::remove_current(),
        Command::Harpoon {
            command: HarpoonCommand::List,
        } => harpoon::list(),
        Command::Doctor => doctor(),
        Command::Agents => list_agents(),
        Command::AlertJump => alerts::jump_latest(),
        Command::AlertClick { pane_id } => alerts::jump_to(&pane_id),
        Command::AlertOverlay {
            status,
            location,
            width,
            duration,
        } => alerts::run_overlay(&status, &location, width, duration),
        Command::OpenCode {
            command:
                OpenCodeCommand::Install {
                    plugin_dir,
                    binary,
                    quiet,
                },
        } => {
            let changed = opencode::install_integration(&plugin_dir, &binary)?;
            if !quiet {
                println!(
                    "OpenCode integration {}",
                    if changed {
                        "installed"
                    } else {
                        "already installed"
                    }
                );
            }
            Ok(())
        }
        Command::OpenCode {
            command: OpenCodeCommand::Remove { plugin_dir },
        } => {
            let changed = opencode::remove_integration(&plugin_dir)?;
            println!(
                "OpenCode integration {}",
                if changed { "removed" } else { "not installed" }
            );
            Ok(())
        }
        Command::OpenCode {
            command: OpenCodeCommand::Status { plugin_dir },
        } => {
            println!(
                "{}",
                if opencode::integration_installed(&plugin_dir)? {
                    "installed"
                } else {
                    "not installed"
                }
            );
            Ok(())
        }
        Command::OpenCodeReport {
            pane_id,
            session_id,
            status,
            title,
        } => {
            if opencode::AgentStatus::from_report(&status).is_none() {
                anyhow::bail!("unsupported OpenCode agent status: {status}");
            }
            alerts::report(&pane_id, "opencode", &session_id, &status, &title)
        }
        Command::ClaudeCode {
            command: ClaudeCodeCommand::Install { binary, quiet },
        } => {
            let changed = claude::install_integration(&binary)?;
            if !quiet {
                println!(
                    "Claude Code integration {}",
                    if changed {
                        "installed"
                    } else {
                        "already installed"
                    }
                );
            }
            Ok(())
        }
        Command::ClaudeCode {
            command: ClaudeCodeCommand::Remove,
        } => {
            let changed = claude::remove_integration()?;
            println!(
                "Claude Code integration {}",
                if changed { "removed" } else { "not installed" }
            );
            Ok(())
        }
        Command::ClaudeCode {
            command: ClaudeCodeCommand::Status,
        } => {
            println!(
                "{}",
                if claude::integration_installed()? {
                    "installed"
                } else {
                    "not installed"
                }
            );
            Ok(())
        }
        Command::Integrations {
            command: IntegrationsCommand::Uninstall { plugin_dir },
        } => uninstall_integrations(&plugin_dir),
        Command::ClaudeHook => claude::handle_hook(),
        Command::ToastClear { token } => notify::clear(&token),
        Command::SessionTouch { session_name } => {
            store::Store::open()?.touch_session(&session_name)
        }
    }
}

fn list_agents() -> Result<()> {
    let tmux = tmux::Tmux::from_env();
    let reports = store::Store::open()?.agent_reports()?;
    for agent in opencode::discover(tmux.windows()?, &reports)? {
        println!(
            "{}\t{}\t{}:{}\t{}\t{}",
            agent.status.label(),
            agent.provider,
            agent.window.session_name,
            agent.window.window_name,
            agent.title,
            agent.session_id.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn doctor() -> Result<()> {
    let tmux = tmux::Tmux::from_env();
    let version = tmux.version()?;
    let windows = tmux.windows()?;
    let reports = store::Store::open()?.agent_reports()?;
    match tmux_version(&version) {
        Some(found) if found >= MIN_TMUX_VERSION => println!("[OK] {version}"),
        Some(_) => println!(
            "[WARN] {version} is too old; tmux-enhance requires tmux {}.{} or newer",
            MIN_TMUX_VERSION.0, MIN_TMUX_VERSION.1
        ),
        None => println!(
            "[WARN] could not parse '{version}'; tmux-enhance requires tmux {}.{} or newer",
            MIN_TMUX_VERSION.0, MIN_TMUX_VERSION.1
        ),
    }
    println!("[OK] discovered {} windows", windows.len());
    if reports.is_empty() {
        println!("[INFO] no direct agent pane reports received yet");
    } else {
        println!(
            "[OK] received direct reports from {} agent panes",
            reports.len()
        );
    }
    println!("[OK] terminal popup UI available");
    Ok(())
}

fn uninstall_integrations(plugin_dir: &std::path::Path) -> Result<()> {
    let opencode_removed = opencode::remove_integration(plugin_dir)?;
    let claude_removed = claude::remove_integration()?;
    println!(
        "OpenCode integration {}",
        if opencode_removed {
            "removed"
        } else {
            "not installed"
        }
    );
    println!(
        "Claude Code integration {}",
        if claude_removed {
            "removed"
        } else {
            "not installed"
        }
    );
    Ok(())
}

fn tmux_version(output: &str) -> Option<(u32, u32)> {
    let version = output.trim().strip_prefix("tmux ")?;
    let (major, minor) = version.split_once('.')?;
    let minor = minor
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_and_letter_suffixed_tmux_versions() {
        assert_eq!(tmux_version("tmux 3.7"), Some((3, 7)));
        assert_eq!(tmux_version("tmux 3.6a"), Some((3, 6)));
        assert_eq!(tmux_version("tmux 4.0\n"), Some((4, 0)));
    }

    #[test]
    fn rejects_unrecognized_tmux_versions() {
        assert_eq!(tmux_version("3.8"), None);
        assert_eq!(tmux_version("tmux next"), None);
    }
}
