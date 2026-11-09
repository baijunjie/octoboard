//! What the daemon records of the terminals it runs in holders: which holder runs each session's
//! launch and each shell terminal, and the name the user gave a shell terminal. A start that cannot
//! link to a holder reads the first to know exactly which sessions that holder runs, and a shell
//! terminal a holder kept running is put back with the second.

use rusqlite::params;

use super::Result;
use super::Store;

/// What a held terminal was launched for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldKind {
    /// A launch of a session's agent; the owner is the session.
    Session,
    /// A shell terminal; the owner is the terminal, and so is the launch.
    ShellTerminal,
}

impl HeldKind {
    fn text(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::ShellTerminal => "shell_terminal",
        }
    }

    fn from_text(text: &str) -> Option<Self> {
        match text {
            "session" => Some(Self::Session),
            "shell_terminal" => Some(Self::ShellTerminal),
            _ => None,
        }
    }
}

/// Where the latest launch of a session, or a shell terminal, runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldTerminalRecord {
    pub kind: HeldKind,
    /// The session or shell terminal.
    pub owner_id: String,
    pub holder_id: String,
    /// The terminal's id in its holder.
    pub terminal_id: String,
    pub launch_id: String,
}

impl Store {
    /// Records where `record`'s owner runs, in place of where its previous launch ran.
    pub fn record_held_terminal(&self, record: &HeldTerminalRecord) -> Result<()> {
        self.lock().execute(
            "INSERT INTO held_terminals (kind, owner_id, holder_id, terminal_id, launch_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (kind, owner_id) DO UPDATE SET
                 holder_id = ?3, terminal_id = ?4, launch_id = ?5",
            params![
                record.kind.text(),
                record.owner_id,
                record.holder_id,
                record.terminal_id,
                record.launch_id
            ],
        )?;
        Ok(())
    }

    /// Every terminal recorded, whatever has become of it since: a session's row outlives its
    /// launch until another launch replaces it or its holder is forgotten.
    pub fn held_terminals(&self) -> Result<Vec<HeldTerminalRecord>> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT kind, owner_id, holder_id, terminal_id, launch_id FROM held_terminals",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    HeldTerminalRecord {
                        kind: HeldKind::Session,
                        owner_id: row.get(1)?,
                        holder_id: row.get(2)?,
                        terminal_id: row.get(3)?,
                        launch_id: row.get(4)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(kind, record)| {
                Some(HeldTerminalRecord {
                    kind: HeldKind::from_text(&kind)?,
                    ..record
                })
            })
            .collect())
    }

    /// Forgets where the owner `owner_id` of kind `kind` runs.
    #[cfg(test)]
    pub fn forget_held_terminal(&self, kind: HeldKind, owner_id: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM held_terminals WHERE kind = ?1 AND owner_id = ?2",
            params![kind.text(), owner_id],
        )?;
        Ok(())
    }

    /// Forgets every terminal recorded for the holder `holder`, which is gone.
    pub fn forget_held_terminals_of(&self, holder: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM held_terminals WHERE holder_id = ?1",
            params![holder],
        )?;
        Ok(())
    }

    /// Forgets every terminal recorded for a holder that is not among `holders`: the holders
    /// that still run.
    pub fn forget_held_terminals_except(&self, holders: &[String]) -> Result<()> {
        let conn = self.lock();
        let recorded: Vec<String> = conn
            .prepare("SELECT DISTINCT holder_id FROM held_terminals")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for holder in recorded.iter().filter(|holder| !holders.contains(holder)) {
            conn.execute(
                "DELETE FROM held_terminals WHERE holder_id = ?1",
                params![holder],
            )?;
        }
        Ok(())
    }

    /// The name the user gave the shell terminal `id`, if any.
    pub fn shell_terminal_title(&self, id: &str) -> Result<Option<String>> {
        let conn = self.lock();
        let mut statement =
            conn.prepare("SELECT title FROM shell_terminal_titles WHERE id = ?1")?;
        let mut rows = statement.query_map(params![id], |row| row.get::<_, String>(0))?;
        Ok(rows.next().transpose()?)
    }

    /// Records `title` as the name the user gave the shell terminal `id`.
    pub fn set_shell_terminal_title(&self, id: &str, title: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO shell_terminal_titles (id, title) VALUES (?1, ?2)
             ON CONFLICT (id) DO UPDATE SET title = ?2",
            params![id, title],
        )?;
        Ok(())
    }

    /// Forgets the name of the shell terminal `id`, with where it runs.
    pub fn forget_shell_terminal(&self, id: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM shell_terminal_titles WHERE id = ?1",
            params![id],
        )?;
        conn.execute(
            "DELETE FROM held_terminals WHERE kind = ?1 AND owner_id = ?2",
            params![HeldKind::ShellTerminal.text(), id],
        )?;
        Ok(())
    }

    /// The shell terminals the user named, by id.
    pub fn shell_terminal_title_ids(&self) -> Result<Vec<String>> {
        let conn = self.lock();
        let ids = conn
            .prepare("SELECT id FROM shell_terminal_titles")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::{HeldKind, HeldTerminalRecord};
    use crate::store::Store;

    fn store(label: &str) -> (Store, crate::test_support::ScratchDir) {
        let dir = crate::test_support::ScratchDir::new(label);
        let store = Store::open(&dir.join("octoboard.db")).expect("store");
        (store, dir)
    }

    fn record(owner: &str, holder: &str, launch: &str) -> HeldTerminalRecord {
        HeldTerminalRecord {
            kind: HeldKind::Session,
            owner_id: owner.to_string(),
            holder_id: holder.to_string(),
            terminal_id: format!("terminal-{launch}"),
            launch_id: launch.to_string(),
        }
    }

    #[test]
    fn a_later_launch_replaces_where_the_earlier_ran_and_a_gone_holder_is_forgotten() {
        let (store, _dir) = store("held-terminals");
        store
            .record_held_terminal(&record("s1", "h1", "l1"))
            .unwrap();
        store
            .record_held_terminal(&record("s2", "h2", "l2"))
            .unwrap();
        store
            .record_held_terminal(&record("s1", "h2", "l3"))
            .unwrap();
        let mut held = store.held_terminals().unwrap();
        held.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
        assert_eq!(
            held,
            vec![record("s1", "h2", "l3"), record("s2", "h2", "l2")]
        );
        store
            .record_held_terminal(&record("s3", "h1", "l4"))
            .unwrap();
        store
            .forget_held_terminals_except(&["h2".to_string()])
            .unwrap();
        assert_eq!(store.held_terminals().unwrap().len(), 2);
        store.forget_held_terminal(HeldKind::Session, "s2").unwrap();
        assert_eq!(
            store.held_terminals().unwrap(),
            vec![record("s1", "h2", "l3")]
        );
    }

    #[test]
    fn a_shell_terminals_name_is_kept_until_it_is_forgotten() {
        let (store, _dir) = store("shell-titles");
        assert_eq!(store.shell_terminal_title("t1").unwrap(), None);
        store.set_shell_terminal_title("t1", "build").unwrap();
        store.set_shell_terminal_title("t1", "server").unwrap();
        assert_eq!(
            store.shell_terminal_title("t1").unwrap().as_deref(),
            Some("server")
        );
        assert_eq!(store.shell_terminal_title_ids().unwrap(), vec!["t1"]);
        store.forget_shell_terminal("t1").unwrap();
        assert_eq!(store.shell_terminal_title("t1").unwrap(), None);
    }
}
