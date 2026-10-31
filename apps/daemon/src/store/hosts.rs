//! The hosts table.

use anyhow::Result;
use rusqlite::params;

use super::rows::{enum_from_row, enum_to_text};
use super::{Store, LOCAL_HOST_ID};
use crate::protocol::{Host, HostKind};

impl Store {
    pub(super) fn ensure_local_host(&self) -> Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO hosts (id, name, kind, ssh_config) VALUES (?1, ?2, ?3, NULL)",
            params![
                LOCAL_HOST_ID,
                "This machine",
                enum_to_text(&HostKind::Local)
            ],
        )?;
        Ok(())
    }

    pub fn list_hosts(&self) -> Result<Vec<Host>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, name, kind, ssh_config FROM hosts ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Host {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: enum_from_row(row, 2)?,
                    ssh_config: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
