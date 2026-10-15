use std::{
    env, fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::alerts;

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PermissionRequest",
    "PermissionDenied",
    "Notification",
    "PostToolUse",
    "PostToolUseFailure",
    "PostToolBatch",
    "Stop",
    "StopFailure",
    "Elicitation",
    "ElicitationResult",
    "SessionEnd",
];

pub fn handle_hook() -> Result<()> {
    let pane_id = match env::var("TMUX_PANE") {
        Ok(pane_id) if !pane_id.is_empty() => pane_id,
        _ => return Ok(()),
    };
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input).context("parsing Claude Code hook input")?;
    if input.get("agent_id").is_some_and(|value| !value.is_null()) {
        return Ok(());
    }
    let Some(session_id) = input.get("session_id").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(status) = hook_status(&input) else {
        return Ok(());
    };
    let title = hook_title(&input);
    alerts::report(&pane_id, "claude", session_id, status, &title)
}

pub fn install_integration(binary: &Path) -> Result<bool> {
    install_integration_at(&settings_path()?, binary)
}

pub fn remove_integration() -> Result<bool> {
    remove_integration_at(&settings_path()?)
}

pub fn integration_installed() -> Result<bool> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(false);
    }
    let settings = read_settings(&path)?;
    Ok(HOOK_EVENTS
        .iter()
        .all(|event| event_contains_handler(&settings, event)))
}

fn hook_status(input: &Value) -> Option<&'static str> {
    match input.get("hook_event_name")?.as_str()? {
        "SessionStart" | "Stop" | "StopFailure" | "SessionEnd" => Some("ready"),
        "UserPromptSubmit" | "PermissionDenied" | "PostToolUse" | "PostToolUseFailure"
        | "PostToolBatch" | "ElicitationResult" => Some("processing"),
        "PermissionRequest"
            if input.get("tool_name").and_then(Value::as_str) == Some("AskUserQuestion") =>
        {
            Some("input")
        }
        "PermissionRequest" => Some("permission"),
        "Elicitation" => Some("input"),
        "Notification" => match input.get("notification_type")?.as_str()? {
            // PermissionRequest carries the tool name and is therefore more precise. In
            // particular, Claude also describes AskUserQuestion menus as permission prompts.
            "permission_prompt" => None,
            "agent_needs_input" | "elicitation_dialog" | "elicitation_url_dialog" => Some("input"),
            "idle_prompt" | "agent_completed" => Some("ready"),
            "elicitation_response" | "elicitation_complete" => Some("processing"),
            _ => None,
        },
        _ => None,
    }
}

fn hook_title(input: &Value) -> String {
    let Some(prompt) = input.get("prompt").and_then(Value::as_str) else {
        return String::new();
    };
    let title = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = title.chars();
    let shortened = characters.by_ref().take(80).collect::<String>();
    if characters.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

fn settings_path() -> Result<PathBuf> {
    if let Some(directory) = env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(Path::new(&directory).join("settings.json"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(Path::new(&home).join(".claude/settings.json"))
}

fn install_integration_at(settings_path: &Path, binary: &Path) -> Result<bool> {
    let binary = binary
        .canonicalize()
        .with_context(|| format!("finding tmux-enhance binary at {}", binary.display()))?;
    let mut settings = if settings_path.exists() {
        read_settings(settings_path)?
    } else {
        json!({})
    };
    let object = settings
        .as_object_mut()
        .context("Claude Code settings.json must contain a JSON object")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("Claude Code settings.json 'hooks' must be an object")?;
    let handler = json!({
        "type": "command",
        "command": binary.to_string_lossy(),
        "args": ["claude-hook"],
        "timeout": 5
    });
    let mut changed = false;

    for event in HOOK_EVENTS {
        let groups = hooks
            .entry((*event).to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .with_context(|| format!("Claude Code hook '{event}' must be an array"))?;
        let existing = groups.iter_mut().find_map(|group| {
            group
                .get_mut("hooks")?
                .as_array_mut()?
                .iter_mut()
                .find(|candidate| is_tmux_enhance_handler(candidate))
        });
        if let Some(existing) = existing {
            if *existing != handler {
                *existing = handler.clone();
                changed = true;
            }
        } else {
            groups.push(json!({ "hooks": [handler.clone()] }));
            changed = true;
        }
    }
    if changed {
        write_settings(settings_path, &settings)?;
    }
    Ok(changed)
}

fn remove_integration_at(settings_path: &Path) -> Result<bool> {
    if !settings_path.exists() {
        return Ok(false);
    }
    let mut settings = read_settings(settings_path)?;
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for event in HOOK_EVENTS {
        let Some(groups) = hooks.get_mut(*event).and_then(Value::as_array_mut) else {
            continue;
        };
        for group in groups.iter_mut() {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                continue;
            };
            let previous_len = handlers.len();
            handlers.retain(|handler| !is_tmux_enhance_handler(handler));
            changed |= handlers.len() != previous_len;
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|handlers| !handlers.is_empty())
        });
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    if changed {
        write_settings(settings_path, &settings)?;
    }
    Ok(changed)
}

fn event_contains_handler(settings: &Value, event: &str) -> bool {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|handlers| handlers.iter().any(is_tmux_enhance_handler))
            })
        })
}

fn is_tmux_enhance_handler(handler: &Value) -> bool {
    handler
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|arguments| {
            arguments.len() == 1 && arguments[0].as_str() == Some("claude-hook")
        })
}

fn read_settings(path: &Path) -> Result<Value> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("parsing {}", path.display()))
}

fn write_settings(path: &Path, settings: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut contents = serde_json::to_string_pretty(settings)?;
    contents.push('\n');
    let temporary = path.with_extension("json.tmux-enhance.tmp");
    fs::write(&temporary, contents).with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("updating {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn maps_claude_events_to_agent_statuses() {
        assert_eq!(
            hook_status(&json!({ "hook_event_name": "UserPromptSubmit" })),
            Some("processing")
        );
        assert_eq!(
            hook_status(&json!({
                "hook_event_name": "PermissionRequest",
                "tool_name": "Bash"
            })),
            Some("permission")
        );
        assert_eq!(
            hook_status(&json!({
                "hook_event_name": "PermissionRequest",
                "tool_name": "AskUserQuestion"
            })),
            Some("input")
        );
        assert_eq!(
            hook_status(&json!({
                "hook_event_name": "Notification",
                "notification_type": "permission_prompt"
            })),
            None
        );
        assert_eq!(
            hook_status(&json!({
                "hook_event_name": "Notification",
                "notification_type": "agent_needs_input"
            })),
            Some("input")
        );
        assert_eq!(
            hook_status(&json!({ "hook_event_name": "Stop" })),
            Some("ready")
        );
    }

    #[test]
    fn uses_a_compact_user_prompt_as_the_session_title() {
        assert_eq!(
            hook_title(&json!({
                "hook_event_name": "UserPromptSubmit",
                "prompt": "Fix the tests\nthen run Clippy"
            })),
            "Fix the tests then run Clippy"
        );
        assert!(hook_title(&json!({ "prompt": "x".repeat(100) })).ends_with('…'));
    }

    #[test]
    fn installs_and_removes_hooks_without_replacing_existing_hooks() {
        let root = temporary_directory();
        let settings_path = root.join("settings.json");
        let binary = root.join("tmux-enhance");
        fs::write(&binary, "binary").unwrap();
        fs::write(
            &settings_path,
            r#"{"model":"opus","hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"existing"}]}]}}"#,
        )
        .unwrap();

        assert!(install_integration_at(&settings_path, &binary).unwrap());
        assert!(!install_integration_at(&settings_path, &binary).unwrap());
        let installed = read_settings(&settings_path).unwrap();
        assert_eq!(installed["model"], "opus");
        assert_eq!(
            installed["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "existing"
        );
        assert!(
            HOOK_EVENTS
                .iter()
                .all(|event| event_contains_handler(&installed, event))
        );

        assert!(remove_integration_at(&settings_path).unwrap());
        let removed = read_settings(&settings_path).unwrap();
        assert_eq!(
            removed["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "existing"
        );
        assert!(
            HOOK_EVENTS
                .iter()
                .all(|event| !event_contains_handler(&removed, event))
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = env::temp_dir().join(format!(
            "tmux-enhance-claude-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
