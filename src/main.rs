mod app;
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
    /// Print discovered OpenCode agent panes and their status.
    Agents,
    /// Manage the bundled OpenCode TUI integration.
    #[command(name = "opencode")]
    OpenCode {
        #[command(subcommand)]
        command: OpenCodeCommand,
    },
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
    /// Search OpenCode agent panes.
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
            store::Store::open()?.report_agent(&pane_id, &session_id, &status, &title)
        }
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
            "{}\t{}:{}\t{}\t{}",
            agent.status.label(),
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
    println!("[OK] {version}");
    println!("[OK] discovered {} windows", windows.len());
    if reports.is_empty() {
        println!("[INFO] no direct OpenCode pane reports received yet");
    } else {
        println!(
            "[OK] received direct reports from {} OpenCode panes",
            reports.len()
        );
    }
    println!("[OK] terminal popup UI available");
    Ok(())
}
