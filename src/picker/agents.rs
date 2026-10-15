use anyhow::Result;
use std::collections::HashMap;

use crate::{
    app::{PickerAction, PickerBadge, PickerBadgeTone, PickerEntry, PreviewPane},
    opencode::{self, AgentStatus},
    store::Store,
    tmux::Tmux,
};

use super::windows;

pub fn run() -> Result<()> {
    let tmux = Tmux::from_env();
    let current_pane = tmux.current_pane_id()?;
    let store = Store::open()?;
    store.touch_pane(&current_pane)?;
    let history = store.pane_history()?;
    let entries = build_entries(&tmux, &current_pane, &history)?;
    let mut refresh = || build_entries(&tmux, &current_pane, &history);

    windows::run_entries_live("agents", "Agents", entries, &mut refresh)
}

fn build_entries(
    tmux: &Tmux,
    current_pane: &str,
    history: &HashMap<String, i64>,
) -> Result<Vec<PickerEntry>> {
    let reports = Store::open()?.agent_reports()?;
    let mut agents = opencode::discover(tmux.windows()?, &reports)?;
    agents.sort_by(|left, right| {
        let left_current = left.window.pane_id == *current_pane;
        let right_current = right.window.pane_id == *current_pane;
        left_current
            .cmp(&right_current)
            .then_with(|| {
                history
                    .get(&right.window.pane_id)
                    .unwrap_or(&0)
                    .cmp(history.get(&left.window.pane_id).unwrap_or(&0))
            })
            .then_with(|| right.updated.unwrap_or(0).cmp(&left.updated.unwrap_or(0)))
            .then_with(|| status_order(left.status).cmp(&status_order(right.status)))
            .then_with(|| left.window.session_name.cmp(&right.window.session_name))
            .then_with(|| left.window.window_index.cmp(&right.window.window_index))
    });

    Ok(agents
        .into_iter()
        .map(|agent| {
            let pane_id = agent.window.pane_id.clone();
            let agent_name = agent
                .agent
                .as_deref()
                .filter(|name| *name != "build")
                .map(|name| format!(" · {name}"))
                .unwrap_or_default();
            let display_text = format!(
                "{} › {}",
                agent.window.session_name, agent.window.window_name
            );
            let secondary_text = format!("{}{}", agent.title, agent_name);
            let searchable_text = format!(
                "{}\n{} {}",
                display_text,
                secondary_text,
                agent.status.label()
            );
            PickerEntry {
                searchable_text,
                display_text,
                secondary_text: Some(secondary_text),
                badge: Some(PickerBadge {
                    label: agent.status.label().to_uppercase(),
                    tone: badge_tone(agent.status),
                }),
                preview_title: format!(
                    "{} / {} · {}",
                    agent.window.session_name,
                    agent.window.window_name,
                    agent.status.label()
                ),
                preview_panes: vec![PreviewPane {
                    title: format!(
                        "{} / {} · {}",
                        agent.window.session_name,
                        agent.window.window_name,
                        agent.status.label()
                    ),
                    pane_id: pane_id.clone(),
                    active: true,
                }],
                preview_message: None,
                current: pane_id == *current_pane,
                harpoon_slot: None,
                hierarchy: None,
                visible: true,
                action: PickerAction::Window {
                    session_id: agent.window.session_id,
                    window_index: agent.window.window_index,
                },
            }
        })
        .collect())
}

fn status_order(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Permission => 0,
        AgentStatus::Input => 1,
        AgentStatus::Waiting => 2,
        AgentStatus::Processing => 3,
        AgentStatus::Ready => 4,
        AgentStatus::Idle => 5,
    }
}

fn badge_tone(status: AgentStatus) -> PickerBadgeTone {
    match status {
        AgentStatus::Processing => PickerBadgeTone::Processing,
        AgentStatus::Permission | AgentStatus::Waiting => PickerBadgeTone::Permission,
        AgentStatus::Input => PickerBadgeTone::Input,
        AgentStatus::Ready => PickerBadgeTone::Ready,
        AgentStatus::Idle => PickerBadgeTone::Muted,
    }
}
