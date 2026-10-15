mod app;
mod picker;
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
    /// Check runtime dependencies and tmux connectivity.
    Doctor,
}

#[derive(Debug, Subcommand)]
enum PickerSource {
    /// Search every window on the current tmux server.
    Windows,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Picker {
            source: PickerSource::Windows,
        } => picker::windows::run(),
        Command::Doctor => doctor(),
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
