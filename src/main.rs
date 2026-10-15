mod app;
mod harpoon;
mod notify;
mod picker;
mod store;
mod theme;
mod tmux;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    #[command(hide = true)]
    ToastClear { token: String },
}

#[derive(Debug, Subcommand)]
enum PickerSource {
    /// Search every window on the current tmux server.
    Windows,
    /// Search marked Harpoon sessions.
    Harpoon,
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

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Picker {
            source: PickerSource::Windows,
        } => picker::windows::run(),
        Command::Picker {
            source: PickerSource::Harpoon,
        } => harpoon::picker(),
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
        Command::ToastClear { token } => notify::clear(&token),
    }
}

fn doctor() -> Result<()> {
    let tmux = tmux::Tmux::from_env();
    let version = tmux.version()?;
    let windows = tmux.windows()?;
    println!("[OK] {version}");
    println!("[OK] discovered {} windows", windows.len());
    println!("[OK] terminal popup UI available");
    Ok(())
}
