use std::collections::HashMap;

use anyhow::Result;

use crate::{
    app::{PickerAction, PickerEntry, PreviewPane},
    store::Store,
    tmux::{Tmux, WindowTarget},
};

use super::windows;

pub fn run() -> Result<()> {
    let tmux = Tmux::from_env();
    let current_session = tmux.current_session_name()?;
    let store = Store::open()?;
    store.touch_session(&current_session)?;
    let entries = session_entries(tmux.windows()?, &current_session, &store.session_history()?);
    windows::run_entries("sessions", "Sessions", entries)
}

fn session_entries(
    windows: Vec<WindowTarget>,
    current_session: &str,
    history: &HashMap<String, i64>,
) -> Vec<PickerEntry> {
    let mut sessions: HashMap<String, Vec<WindowTarget>> = HashMap::new();
    for window in windows {
        sessions
            .entry(window.session_name.clone())
            .or_default()
            .push(window);
    }

    let mut sessions: Vec<_> = sessions.into_values().collect();
    sessions.sort_by(|left, right| {
        let left_current = left[0].session_name == current_session;
        let right_current = right[0].session_name == current_session;
        left_current
            .cmp(&right_current)
            .then_with(|| {
                history
                    .get(&right[0].session_name)
                    .unwrap_or(&0)
                    .cmp(history.get(&left[0].session_name).unwrap_or(&0))
            })
            .then_with(|| right[0].session_activity.cmp(&left[0].session_activity))
            .then_with(|| left[0].session_name.cmp(&right[0].session_name))
    });

    sessions
        .into_iter()
        .map(|mut session_windows| {
            session_windows.sort_by(|left, right| {
                right
                    .session_active_window
                    .cmp(&left.session_active_window)
                    .then_with(|| right.window_activity.cmp(&left.window_activity))
                    .then_with(|| left.window_index.cmp(&right.window_index))
            });

            let session = &session_windows[0];
            let session_name = session.session_name.clone();
            let window_count = session.session_windows;
            let attached = if session.attached_clients > 0 {
                format!("  {} attached", session.attached_clients)
            } else {
                String::new()
            };
            let window_names = session_windows
                .iter()
                .map(|window| window.window_name.as_str())
                .collect::<Vec<_>>()
                .join("  ");
            let display_text = format!(
                "{}  {} window{}{}  {}",
                session_name,
                window_count,
                if window_count == 1 { "" } else { "s" },
                attached,
                window_names
            );

            let overflow = session_windows.len().saturating_sub(4);
            let preview_panes = session_windows
                .iter()
                .take(4)
                .enumerate()
                .map(|(position, window)| {
                    let mut title = format!("{}:{}", window.window_index, window.window_name);
                    if position == 0 && overflow > 0 {
                        title.push_str(&format!(" · +{overflow} windows"));
                    }
                    PreviewPane {
                        title,
                        pane_id: window.pane_id.clone(),
                        active: window.session_active_window,
                    }
                })
                .collect();

            PickerEntry {
                searchable_text: display_text.clone(),
                display_text,
                preview_title: session_name.clone(),
                preview_panes,
                preview_message: None,
                current: session_name == current_session,
                harpoon_slot: None,
                action: PickerAction::Session { session_name },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        session: &str,
        index: u32,
        session_activity: i64,
        window_activity: i64,
        active: bool,
        window_count: u32,
    ) -> WindowTarget {
        WindowTarget {
            session_id: format!("${session}"),
            session_name: session.into(),
            window_id: format!("@{index}"),
            window_index: index,
            window_name: format!("window-{index}"),
            pane_id: format!("%{index}"),
            command: "shell".into(),
            current_path: "/tmp".into(),
            current: false,
            session_active_window: active,
            session_activity,
            session_windows: window_count,
            attached_clients: 0,
            window_activity,
        }
    }

    #[test]
    fn sorts_sessions_by_activity() {
        let entries = session_entries(
            vec![
                window("older", 0, 10, 10, true, 1),
                window("newer", 0, 20, 20, true, 1),
            ],
            "older",
            &HashMap::new(),
        );
        assert!(entries[0].display_text.starts_with("newer"));
        assert!(entries[1].current);
    }

    #[test]
    fn previews_active_window_then_three_recent_windows() {
        let entries = session_entries(
            (0..5)
                .map(|index| window("project", index, 20, 100 + index as i64, index == 0, 5))
                .collect(),
            "project",
            &HashMap::new(),
        );
        let previews = &entries[0].preview_panes;
        assert_eq!(previews.len(), 4);
        assert!(previews[0].active);
        assert!(previews[0].title.contains("+1 windows"));
        assert_eq!(previews[1].title, "4:window-4");
    }

    #[test]
    fn puts_the_previous_session_before_the_current_session() {
        let history = HashMap::from([("previous".into(), 20), ("current".into(), 30)]);
        let entries = session_entries(
            vec![
                window("other", 0, 100, 100, true, 1),
                window("previous", 0, 10, 10, true, 1),
                window("current", 0, 200, 200, true, 1),
            ],
            "current",
            &history,
        );
        assert!(entries[0].display_text.starts_with("previous"));
        assert!(entries.last().unwrap().current);
    }
}
