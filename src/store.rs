use std::{
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
            );",
        )?;
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
}
