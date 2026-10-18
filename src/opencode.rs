use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{store::AgentReport, tmux::WindowTarget};

const REPORT_TTL: Duration = Duration::from_secs(45);

pub fn check_compatibility() -> Result<String> {
    let output = Command::new("opencode")
        .arg("--version")
        .output()
        .context("checking OpenCode version; the integration requires OpenCode 2.0 or newer")?;
    if !output.status.success() {
        bail!("could not check OpenCode version; the integration requires OpenCode 2.0 or newer");
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    check_version(&version)?;
    Ok(version)
}

fn check_version(version: &str) -> Result<()> {
    let number = version.strip_prefix("opencode ").unwrap_or(version);
    let number = number.strip_prefix('v').unwrap_or(number);
    match number
        .split_once('.')
        .and_then(|(major, _)| major.parse::<u32>().ok())
    {
        Some(major) if major >= 2 => Ok(()),
        Some(_) => bail!(
            "OpenCode {version} is not supported; the integration requires OpenCode 2.0 or newer. Upgrade OpenCode to use agent discovery and notifications"
        ),
        None => bail!(
            "could not parse OpenCode version '{version}'; the integration requires OpenCode 2.0 or newer"
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Waiting,
    Ready,
    Input,
    Permission,
    Processing,
}

impl AgentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Waiting => "waiting",
            Self::Ready => "ready",
            Self::Input => "input",
            Self::Permission => "permission",
            Self::Processing => "processing",
        }
    }

    pub fn from_report(value: &str) -> Option<Self> {
        match value {
            "ready" => Some(Self::Ready),
            "input" => Some(Self::Input),
            "permission" => Some(Self::Permission),
            "processing" => Some(Self::Processing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentPane {
    pub window: WindowTarget,
    pub provider: &'static str,
    pub session_id: Option<String>,
    pub title: String,
    pub agent: Option<String>,
    pub status: AgentStatus,
    pub updated: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenCodeSession {
    id: String,
    title: String,
    #[serde(default)]
    agent: Option<String>,
    time: SessionTime,
    location: SessionLocation,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionTime {
    updated: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionLocation {
    directory: String,
}

#[derive(Debug, Deserialize)]
struct ActiveStatus {
    #[serde(rename = "type")]
    kind: String,
}

pub fn discover(
    windows: Vec<WindowTarget>,
    reports: &HashMap<String, AgentReport>,
) -> Result<Vec<AgentPane>> {
    let has_opencode = windows
        .iter()
        .any(|window| is_opencode_window(window, reports));
    let mut sessions: Vec<OpenCodeSession> = if has_opencode {
        api("/api/session")?
    } else {
        Vec::new()
    };
    let active: HashMap<String, ActiveStatus> = if has_opencode {
        api("/api/session/active")?
    } else {
        HashMap::new()
    };
    sessions.sort_by_key(|session| std::cmp::Reverse(session.time.updated));

    Ok(windows
        .into_iter()
        .filter_map(|window| {
            if is_claude_window(&window) {
                let report = reports
                    .get(&window.pane_id)
                    .filter(|report| report.provider == "claude");
                let title = report
                    .map(|report| report.title.trim())
                    .filter(|title| !title.is_empty())
                    .unwrap_or(window.pane_title.trim())
                    .to_string();
                return Some(AgentPane {
                    provider: "Claude Code",
                    session_id: report.map(|report| report.session_id.clone()),
                    status: report
                        .and_then(|report| AgentStatus::from_report(&report.status))
                        .unwrap_or(AgentStatus::Idle),
                    agent: None,
                    updated: report.map(|report| report.reported_at),
                    title,
                    window,
                });
            }
            if !is_opencode_window(&window, reports) {
                return None;
            }
            let pane_title = window
                .pane_title
                .strip_prefix("OC | ")
                .unwrap_or(&window.pane_title)
                .trim();
            let report = reports
                .get(&window.pane_id)
                .filter(|report| report.provider == "opencode" && report_is_fresh(report));
            let session = report
                .and_then(|report| {
                    sessions
                        .iter()
                        .find(|session| session.id == report.session_id)
                })
                .or_else(|| best_session(pane_title, &window.current_path, &sessions));
            let session_id = session.map(|session| session.id.clone());
            let status = report
                .and_then(|report| AgentStatus::from_report(&report.status))
                .unwrap_or_else(|| match session_id.as_ref().and_then(|id| active.get(id)) {
                    Some(status) if status.kind != "idle" => AgentStatus::Processing,
                    _ if session.is_some() => AgentStatus::Waiting,
                    _ => AgentStatus::Idle,
                });
            Some(AgentPane {
                provider: "OpenCode",
                title: session
                    .map(|session| session.title.clone())
                    .filter(|title| !title.is_empty())
                    .or_else(|| report.map(|report| report.title.clone()))
                    .unwrap_or_else(|| pane_title.to_string()),
                agent: session.and_then(|session| session.agent.clone()),
                updated: session.map(|session| session.time.updated),
                window,
                session_id,
                status,
            })
        })
        .collect())
}

fn is_opencode_window(window: &WindowTarget, reports: &HashMap<String, AgentReport>) -> bool {
    window.command == "opencode"
        || window.pane_title.starts_with("OC | ")
        || reports
            .get(&window.pane_id)
            .is_some_and(|report| report.provider == "opencode" && report_is_fresh(report))
}

fn is_claude_window(window: &WindowTarget) -> bool {
    window.command == "claude"
        || window.pane_title.starts_with("Claude")
        || window.pane_title.starts_with("claude")
}

fn report_is_fresh(report: &AgentReport) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    now.saturating_sub(report.reported_at) <= REPORT_TTL.as_millis() as i64
}

fn best_session<'a>(
    title: &str,
    pane_path: &str,
    sessions: &'a [OpenCodeSession],
) -> Option<&'a OpenCodeSession> {
    sessions
        .iter()
        .filter(|session| session.title == title)
        .max_by_key(|session| {
            let session_path = Path::new(&session.location.directory);
            let pane_path = Path::new(pane_path);
            (
                u8::from(session_path == pane_path)
                    + u8::from(
                        session_path.starts_with(pane_path) || pane_path.starts_with(session_path),
                    ),
                session.time.updated,
            )
        })
}

fn api<T>(path: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let output = Command::new("opencode")
        .args(["api", "get", path])
        .output()
        .context("running opencode api")?;
    if !output.status.success() {
        check_compatibility()?;
        bail!(
            "opencode api: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let response: ApiResponse<T> = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("parsing OpenCode response for {path}"))?;
    Ok(response.data)
}

pub fn install_integration(plugin_dir: &Path, binary: &Path) -> Result<bool> {
    let config_path = cli_config_path()?;
    install_integration_at(&config_path, plugin_dir, binary)
}

pub fn remove_integration(plugin_dir: &Path) -> Result<bool> {
    let config_path = cli_config_path()?;
    remove_integration_at(&config_path, plugin_dir)
}

pub fn integration_installed(plugin_dir: &Path) -> Result<bool> {
    let config_path = cli_config_path()?;
    if !config_path.exists() {
        return Ok(false);
    }
    let plugin_dir = plugin_dir
        .canonicalize()
        .unwrap_or_else(|_| plugin_dir.to_path_buf());
    let config = read_cli_config(&config_path)?;
    Ok(config
        .get("plugins")
        .and_then(Value::as_array)
        .is_some_and(|plugins| {
            plugins
                .iter()
                .any(|entry| package_path(entry) == Some(plugin_dir.as_path()))
        }))
}

fn cli_config_path() -> Result<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .context("neither XDG_CONFIG_HOME nor HOME is set")?;
    Ok(config_home.join("opencode/cli.json"))
}

fn install_integration_at(config_path: &Path, plugin_dir: &Path, binary: &Path) -> Result<bool> {
    let plugin_dir = plugin_dir
        .canonicalize()
        .with_context(|| format!("finding OpenCode plugin at {}", plugin_dir.display()))?;
    let binary = binary
        .canonicalize()
        .with_context(|| format!("finding tmux-enhance binary at {}", binary.display()))?;
    let mut config = if config_path.exists() {
        read_cli_config(config_path)?
    } else {
        json!({ "$schema": "https://opencode.ai/v2/cli.json" })
    };
    let object = config
        .as_object_mut()
        .context("OpenCode cli.json must contain a JSON object")?;
    let plugins = object
        .entry("plugins")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("OpenCode cli.json 'plugins' must be an array")?;

    let plugin_string = plugin_dir.to_string_lossy().into_owned();
    let binary_string = binary.to_string_lossy().into_owned();
    if let Some(entry) = plugins
        .iter_mut()
        .find(|entry| package_path(entry) == Some(plugin_dir.as_path()))
    {
        let desired = plugin_entry(&plugin_string, &binary_string, entry);
        if *entry == desired {
            return Ok(false);
        }
        *entry = desired;
    } else {
        plugins.push(plugin_entry(&plugin_string, &binary_string, &Value::Null));
    }
    write_cli_config(config_path, &config)?;
    Ok(true)
}

fn remove_integration_at(config_path: &Path, plugin_dir: &Path) -> Result<bool> {
    if !config_path.exists() {
        return Ok(false);
    }
    let plugin_dir = plugin_dir
        .canonicalize()
        .unwrap_or_else(|_| plugin_dir.to_path_buf());
    let mut config = read_cli_config(config_path)?;
    let Some(plugins) = config.get_mut("plugins").and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let previous_len = plugins.len();
    plugins.retain(|entry| package_path(entry) != Some(plugin_dir.as_path()));
    if plugins.len() == previous_len {
        return Ok(false);
    }
    write_cli_config(config_path, &config)?;
    Ok(true)
}

fn plugin_entry(package: &str, binary: &str, previous: &Value) -> Value {
    let mut entry = previous.as_object().cloned().unwrap_or_default();
    entry.insert("package".into(), Value::String(package.into()));
    let mut options = entry
        .remove("options")
        .and_then(|options| options.as_object().cloned())
        .unwrap_or_default();
    options.insert("binary".into(), Value::String(binary.into()));
    entry.insert("options".into(), Value::Object(options));
    Value::Object(entry)
}

fn package_path(entry: &Value) -> Option<&Path> {
    entry
        .as_str()
        .or_else(|| entry.get("package").and_then(Value::as_str))
        .map(Path::new)
}

fn read_cli_config(path: &Path) -> Result<Value> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("parsing {}", path.display()))
}

fn write_cli_config(path: &Path, config: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut contents = serde_json::to_string_pretty(config)?;
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
    fn requires_opencode_two_or_newer() {
        for version in ["2.0.0", "2.0.0-beta.1", "v2.1.0", "opencode 2.0.0", "3.0.0"] {
            assert!(check_version(version).is_ok(), "{version}");
        }
        let error = check_version("1.18.3").unwrap_err().to_string();
        assert!(error.contains("1.18.3 is not supported"));
        assert!(error.contains("requires OpenCode 2.0 or newer"));
        for version in ["", "unknown", "2"] {
            assert!(check_version(version).is_err(), "{version}");
        }
    }

    fn session(id: &str, title: &str, directory: &str, updated: i64) -> OpenCodeSession {
        OpenCodeSession {
            id: id.into(),
            title: title.into(),
            agent: Some("build".into()),
            time: SessionTime { updated },
            location: SessionLocation {
                directory: directory.into(),
            },
        }
    }

    #[test]
    fn matches_titles_and_prefers_the_pane_path() {
        let sessions = vec![
            session("newer", "Review", "/other", 20),
            session("project", "Review", "/project", 10),
        ];
        let matched = best_session("Review", "/project/src", &sessions).unwrap();
        assert_eq!(matched.id, "project");
    }

    #[test]
    fn installs_without_replacing_other_cli_settings() {
        let root = temporary_directory();
        let config = root.join("cli.json");
        let plugin = root.join("plugin");
        let binary = root.join("tmux-enhance");
        fs::create_dir(&plugin).unwrap();
        fs::write(&binary, "binary").unwrap();
        fs::write(
            &config,
            r#"{"theme":{"name":"test"},"plugins":["existing"]}"#,
        )
        .unwrap();

        assert!(install_integration_at(&config, &plugin, &binary).unwrap());
        assert!(!install_integration_at(&config, &plugin, &binary).unwrap());
        let installed = read_cli_config(&config).unwrap();
        assert_eq!(installed["theme"]["name"], "test");
        assert_eq!(installed["plugins"][0], "existing");
        assert_eq!(installed["plugins"].as_array().unwrap().len(), 2);
        assert!(remove_integration_at(&config, &plugin).unwrap());
        assert_eq!(read_cli_config(&config).unwrap()["plugins"][0], "existing");
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = env::temp_dir().join(format!(
            "tmux-enhance-opencode-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
