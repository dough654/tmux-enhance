use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::{
    app::{PickerAction, PickerEntry, PreviewPane},
    notify,
    picker::windows,
    store::Store,
    tmux::{Tmux, WindowTarget},
};

pub fn add() -> Result<()> {
    let tmux = Tmux::from_env();
    let session_name = tmux.current_session_name()?;
    let result = Store::open()?.add_session(&session_name)?;
    let message = if result.added {
        format!("Harpoon [{slot}] added {session_name}", slot = result.slot)
    } else {
        format!(
            "Harpoon [{slot}] already marks {session_name}",
            slot = result.slot
        )
    };
    notify::show(&message)
}

pub fn jump(slot: u8) -> Result<()> {
    validate_slot(slot)?;
    let tmux = Tmux::from_env();
    let Some(mark) = Store::open()?.get(slot)? else {
        let message = format!("Harpoon [{slot}] is empty");
        notify::show(&message)?;
        return Ok(());
    };
    if !tmux.session_exists(&mark.session_name)? {
        let message = format!(
            "Harpoon [{slot}] session '{}' is unavailable",
            mark.session_name
        );
        notify::show(&message)?;
        return Ok(());
    }
    tmux.switch_to_session(&mark.session_name)
}

pub fn remove(slot: u8) -> Result<()> {
    validate_slot(slot)?;
    let removed = Store::open()?.remove(slot)?;
    let message = if removed {
        format!("Harpoon [{slot}] removed")
    } else {
        format!("Harpoon [{slot}] is already empty")
    };
    notify::show(&message)
}

pub fn remove_current() -> Result<()> {
    let tmux = Tmux::from_env();
    let session_name = tmux.current_session_name()?;
    let removed_slot = Store::open()?.remove_session(&session_name)?;
    let message = if let Some(slot) = removed_slot {
        format!("Harpoon [{slot}] removed {session_name}")
    } else {
        format!("Harpoon does not mark {session_name}")
    };
    notify::show(&message)
}

pub fn list() -> Result<()> {
    for mark in Store::open()?.list()? {
        println!("{}\t{}", mark.slot, mark.session_name);
    }
    Ok(())
}

pub fn picker() -> Result<()> {
    let tmux = Tmux::from_env();
    let current_session = tmux.current_session_name()?;
    let active_windows: HashMap<String, WindowTarget> = tmux
        .windows()?
        .into_iter()
        .filter(|window| window.session_active_window)
        .map(|window| (window.session_name.clone(), window))
        .collect();

    let entries = Store::open()?
        .list()?
        .into_iter()
        .map(|mark| {
            if let Some(window) = active_windows.get(&mark.session_name) {
                let display_text = format!(
                    "{}  {}  {}  {}  {}",
                    mark.slot,
                    mark.session_name,
                    window.window_name,
                    window.command,
                    window.current_path
                );
                PickerEntry {
                    searchable_text: display_text.clone(),
                    display_text,
                    secondary_text: None,
                    badge: None,
                    preview_title: format!(
                        "{} · {} / {}",
                        mark.slot, mark.session_name, window.window_name
                    ),
                    preview_panes: vec![PreviewPane {
                        title: format!("{} / {}", mark.session_name, window.window_name),
                        pane_id: window.pane_id.clone(),
                        active: true,
                    }],
                    preview_message: None,
                    current: mark.session_name == current_session,
                    harpoon_slot: Some(mark.slot),
                    hierarchy: None,
                    visible: true,
                    action: PickerAction::Session {
                        session_name: mark.session_name,
                    },
                }
            } else {
                let message = format!(
                    "Session '{}' is not running. Remove it with: tmux-enhance harpoon remove {}",
                    mark.session_name, mark.slot
                );
                let display_text = format!("{}  {}  [missing]", mark.slot, mark.session_name);
                PickerEntry {
                    searchable_text: display_text.clone(),
                    display_text,
                    secondary_text: None,
                    badge: None,
                    preview_title: format!("{} · {} · unavailable", mark.slot, mark.session_name),
                    preview_panes: Vec::new(),
                    preview_message: Some(message.clone()),
                    current: false,
                    harpoon_slot: Some(mark.slot),
                    hierarchy: None,
                    visible: true,
                    action: PickerAction::Unavailable { message },
                }
            }
        })
        .collect();

    windows::run_entries("harpoon", "Sessions", entries)
}

fn validate_slot(slot: u8) -> Result<()> {
    if !(1..=9).contains(&slot) {
        bail!("Harpoon slot must be between 1 and 9");
    }
    Ok(())
}
