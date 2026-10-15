use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarpoonSlot {
    pub slot: u8,
    pub session_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddResult {
    pub slot: u8,
    pub added: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentReport {
    pub pane_id: String,
    pub provider: String,
    pub session_id: String,
    pub status: String,
    pub title: String,
    pub reported_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTransition {
    pub report: AgentReport,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAlert {
    pub pane_id: String,
    pub provider: String,
    pub session_id: String,
    pub status: String,
    pub title: String,
    pub created_at: i64,
}

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open() -> Result<Self> {
        let path = database_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let connection =
            Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS harpoon_slots (
                slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 9),
                session_name TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_history (
                session_name TEXT PRIMARY KEY,
                last_accessed INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pane_history (
                pane_id TEXT PRIMARY KEY,
                last_accessed INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_reports (
                pane_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL DEFAULT 'opencode',
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                title TEXT NOT NULL,
                reported_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_alerts (
                pane_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );",
        )?;
        let has_provider = {
            let mut statement = connection.prepare("PRAGMA table_info(agent_reports)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            columns
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|column| column == "provider")
        };
        if !has_provider {
            connection.execute(
                "ALTER TABLE agent_reports ADD COLUMN provider TEXT NOT NULL DEFAULT 'opencode'",
                [],
            )?;
        }
        Ok(Self { connection })
    }

    pub fn add_session(&self, session_name: &str) -> Result<AddResult> {
        if let Some(slot) = self
            .connection
            .query_row(
                "SELECT slot FROM harpoon_slots WHERE session_name = ?1",
                [session_name],
                |row| row.get::<_, u8>(0),
            )
            .optional()?
        {
            self.connection.execute(
                "UPDATE harpoon_slots SET updated_at = ?1 WHERE slot = ?2",
                params![timestamp(), slot],
            )?;
            return Ok(AddResult { slot, added: false });
        }

        let used = self.list()?;
        let Some(slot) = (1..=9).find(|slot| !used.iter().any(|item| item.slot == *slot)) else {
            bail!("all nine Harpoon slots are occupied");
        };
        let now = timestamp();
        self.connection.execute(
            "INSERT INTO harpoon_slots (slot, session_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![slot, session_name, now],
        )?;
        Ok(AddResult { slot, added: true })
    }

    pub fn get(&self, slot: u8) -> Result<Option<HarpoonSlot>> {
        self.connection
            .query_row(
                "SELECT slot, session_name FROM harpoon_slots WHERE slot = ?1",
                [slot],
                |row| {
                    Ok(HarpoonSlot {
                        slot: row.get(0)?,
                        session_name: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn remove(&self, slot: u8) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM harpoon_slots WHERE slot = ?1", [slot])?
            > 0)
    }

    pub fn remove_session(&self, session_name: &str) -> Result<Option<u8>> {
        let slot = self
            .connection
            .query_row(
                "SELECT slot FROM harpoon_slots WHERE session_name = ?1",
                [session_name],
                |row| row.get::<_, u8>(0),
            )
            .optional()?;
        if let Some(slot) = slot {
            self.remove(slot)?;
        }
        Ok(slot)
    }

    pub fn list(&self) -> Result<Vec<HarpoonSlot>> {
        let mut statement = self
            .connection
            .prepare("SELECT slot, session_name FROM harpoon_slots ORDER BY slot")?;
        let rows = statement.query_map([], |row| {
            Ok(HarpoonSlot {
                slot: row.get(0)?,
                session_name: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn touch_session(&self, session_name: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO session_history (session_name, last_accessed)
             VALUES (?1, ?2)
             ON CONFLICT(session_name) DO UPDATE SET last_accessed = excluded.last_accessed",
            params![session_name, timestamp_millis()],
        )?;
        Ok(())
    }

    pub fn session_history(&self) -> Result<HashMap<String, i64>> {
        let mut statement = self
            .connection
            .prepare("SELECT session_name, last_accessed FROM session_history")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(Into::into)
    }

    pub fn touch_pane(&self, pane_id: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO pane_history (pane_id, last_accessed)
             VALUES (?1, ?2)
             ON CONFLICT(pane_id) DO UPDATE SET last_accessed = excluded.last_accessed",
            params![pane_id, timestamp_millis()],
        )?;
        Ok(())
    }

    pub fn pane_history(&self) -> Result<HashMap<String, i64>> {
        let mut statement = self
            .connection
            .prepare("SELECT pane_id, last_accessed FROM pane_history")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(Into::into)
    }

    pub fn report_agent(
        &self,
        pane_id: &str,
        provider: &str,
        session_id: &str,
        status: &str,
        title: &str,
    ) -> Result<AgentTransition> {
        let previous = self
            .connection
            .query_row(
                "SELECT provider, session_id, status FROM agent_reports WHERE pane_id = ?1",
                [pane_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let changed = previous.as_ref().is_none_or(
            |(previous_provider, previous_session, previous_status)| {
                previous_provider != provider
                    || previous_session != session_id
                    || previous_status != status
            },
        );
        let now = timestamp_millis();
        self.connection.execute(
            "INSERT INTO agent_reports (pane_id, provider, session_id, status, title, reported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(pane_id) DO UPDATE SET
                provider = excluded.provider,
                session_id = excluded.session_id,
                status = excluded.status,
                title = CASE
                    WHEN excluded.title = '' THEN agent_reports.title
                    ELSE excluded.title
                END,
                reported_at = excluded.reported_at",
            params![pane_id, provider, session_id, status, title, now],
        )?;
        let report = self.connection.query_row(
            "SELECT pane_id, provider, session_id, status, title, reported_at
             FROM agent_reports WHERE pane_id = ?1",
            [pane_id],
            |row| {
                Ok(AgentReport {
                    pane_id: row.get(0)?,
                    provider: row.get(1)?,
                    session_id: row.get(2)?,
                    status: row.get(3)?,
                    title: row.get(4)?,
                    reported_at: row.get(5)?,
                })
            },
        )?;

        if matches!(status, "permission" | "input") {
            if changed {
                self.connection.execute(
                    "INSERT INTO agent_alerts
                        (pane_id, provider, session_id, status, title, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(pane_id) DO UPDATE SET
                        provider = excluded.provider,
                        session_id = excluded.session_id,
                        status = excluded.status,
                        title = excluded.title,
                        created_at = excluded.created_at",
                    params![pane_id, provider, session_id, status, report.title, now],
                )?;
            } else {
                self.connection.execute(
                    "UPDATE agent_alerts SET title = ?1 WHERE pane_id = ?2",
                    params![report.title, pane_id],
                )?;
            }
        } else {
            self.acknowledge_agent_alert(pane_id)?;
        }

        Ok(AgentTransition { report, changed })
    }

    pub fn agent_reports(&self) -> Result<HashMap<String, AgentReport>> {
        let mut statement = self.connection.prepare(
            "SELECT pane_id, provider, session_id, status, title, reported_at FROM agent_reports",
        )?;
        let rows = statement.query_map([], |row| {
            let report = AgentReport {
                pane_id: row.get(0)?,
                provider: row.get(1)?,
                session_id: row.get(2)?,
                status: row.get(3)?,
                title: row.get(4)?,
                reported_at: row.get(5)?,
            };
            Ok((report.pane_id.clone(), report))
        })?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(Into::into)
    }

    pub fn agent_alerts(&self) -> Result<Vec<AgentAlert>> {
        let mut statement = self.connection.prepare(
            "SELECT pane_id, provider, session_id, status, title, created_at
             FROM agent_alerts ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AgentAlert {
                pane_id: row.get(0)?,
                provider: row.get(1)?,
                session_id: row.get(2)?,
                status: row.get(3)?,
                title: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn acknowledge_agent_alert(&self, pane_id: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM agent_alerts WHERE pane_id = ?1", [pane_id])?
            > 0)
    }
}

fn database_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("TMUX_ENHANCE_DB") {
        return Ok(PathBuf::from(path));
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        return Ok(Path::new(&data_home).join("tmux-enhance/state.db"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(Path::new(&home).join(".local/share/tmux-enhance/state.db"))
}

fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::from_connection(Connection::open_in_memory().unwrap()).unwrap()
    }

    #[test]
    fn adds_sessions_without_duplicates() {
        let store = store();
        assert_eq!(
            store.add_session("one").unwrap(),
            AddResult {
                slot: 1,
                added: true
            }
        );
        assert_eq!(
            store.add_session("one").unwrap(),
            AddResult {
                slot: 1,
                added: false
            }
        );
        assert_eq!(store.add_session("two").unwrap().slot, 2);
    }

    #[test]
    fn removes_and_reuses_slots() {
        let store = store();
        store.add_session("one").unwrap();
        store.add_session("two").unwrap();
        assert!(store.remove(1).unwrap());
        assert_eq!(store.add_session("three").unwrap().slot, 1);
    }

    #[test]
    fn removes_a_session_by_name() {
        let store = store();
        store.add_session("one").unwrap();
        assert_eq!(store.remove_session("one").unwrap(), Some(1));
        assert_eq!(store.remove_session("one").unwrap(), None);
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn records_session_access() {
        let store = store();
        store.touch_session("one").unwrap();
        assert!(store.session_history().unwrap().contains_key("one"));
    }

    #[test]
    fn records_pane_access() {
        let store = store();
        store.touch_pane("%12").unwrap();
        assert!(store.pane_history().unwrap().contains_key("%12"));
    }

    #[test]
    fn records_authoritative_agent_reports() {
        let store = store();
        let transition = store
            .report_agent("%12", "opencode", "ses_123", "permission", "Review")
            .unwrap();
        assert!(transition.changed);
        let reports = store.agent_reports().unwrap();
        let report = reports.get("%12").unwrap();
        assert_eq!(report.provider, "opencode");
        assert_eq!(report.session_id, "ses_123");
        assert_eq!(report.status, "permission");
        assert_eq!(report.title, "Review");
    }

    #[test]
    fn retains_an_agent_title_when_a_lifecycle_report_omits_it() {
        let store = store();
        store
            .report_agent("%12", "claude", "session-123", "processing", "Fix tests")
            .unwrap();
        store
            .report_agent("%12", "claude", "session-123", "ready", "")
            .unwrap();

        let reports = store.agent_reports().unwrap();
        let report = reports.get("%12").unwrap();
        assert_eq!(report.status, "ready");
        assert_eq!(report.title, "Fix tests");
    }

    #[test]
    fn queues_attention_transitions_once_and_resolves_them() {
        let store = store();
        let first = store
            .report_agent("%12", "claude", "session-123", "input", "Choose")
            .unwrap();
        assert!(first.changed);
        assert_eq!(store.agent_alerts().unwrap().len(), 1);
        let created_at = store.agent_alerts().unwrap()[0].created_at;

        let repeated = store
            .report_agent("%12", "claude", "session-123", "input", "Choose")
            .unwrap();
        assert!(!repeated.changed);
        assert_eq!(store.agent_alerts().unwrap()[0].created_at, created_at);

        store
            .report_agent("%12", "claude", "session-123", "processing", "")
            .unwrap();
        assert!(store.agent_alerts().unwrap().is_empty());
    }
}
