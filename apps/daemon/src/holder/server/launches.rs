//! The keys the helpers of a launch call the holder with. The holder issues one per launch before
//! the program starts (its command line and environment name it), binds it to the terminal, the
//! session and the launch's MCP token, and forgets it when the terminal is released or its start
//! fails. A key outlives the daemon with its launch, so the daemon that adopts the launch finds it
//! still bound.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::request_guard::same_secret;

/// How long a key issued for a program that never started is kept: the daemon revokes the key of a
/// start it gives up on, so this only covers a daemon that went away between the two.
const UNSPAWNED_KEY_TTL: Duration = Duration::from_secs(10 * 60);

/// What a key stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Launch {
    pub(super) terminal: String,
    pub(super) session: String,
    /// The only MCP token this launch's key may be used with, when it was issued for one.
    pub(super) mcp_token: Option<String>,
}

struct Entry {
    key: String,
    launch: Launch,
    issued: Instant,
    started: bool,
}

#[derive(Default)]
pub(super) struct Launches {
    /// By terminal id.
    entries: Mutex<HashMap<String, Entry>>,
}

impl Launches {
    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// A new key for the launch of `terminal`, replacing any it had, bound to `mcp_token` when
    /// there is one.
    pub(super) fn issue(
        &self,
        terminal: &str,
        session: &str,
        mcp_token: Option<&str>,
    ) -> std::io::Result<String> {
        let key = crate::request_guard::generate_key()?;
        let mut entries = self.entries();
        entries.retain(|_, entry| entry.started || entry.issued.elapsed() < UNSPAWNED_KEY_TTL);
        entries.insert(
            terminal.to_string(),
            Entry {
                key: key.clone(),
                launch: Launch {
                    terminal: terminal.to_string(),
                    session: session.to_string(),
                    mcp_token: mcp_token.map(str::to_string),
                },
                issued: Instant::now(),
                started: false,
            },
        );
        Ok(key)
    }

    /// The program of `terminal` is running, so its key lasts until the terminal is released.
    pub(super) fn started(&self, terminal: &str) {
        if let Some(entry) = self.entries().get_mut(terminal) {
            entry.started = true;
        }
    }

    pub(super) fn revoke(&self, terminal: &str) {
        self.entries().remove(terminal);
    }

    /// The launch a supplied key belongs to. Every key is compared, so the time it takes does not
    /// say how near a guess came.
    pub(super) fn find(&self, supplied: &str) -> Option<Launch> {
        let entries = self.entries();
        let mut found = None;
        for entry in entries.values() {
            if same_secret(supplied, &entry.key) {
                found = Some(entry.launch.clone());
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_finds_its_launch_until_it_is_revoked() {
        let launches = Launches::default();
        let key = launches.issue("t1", "s1", None).unwrap();
        let other = launches.issue("t2", "s2", None).unwrap();
        assert_ne!(key, other);
        assert_eq!(
            launches.find(&key),
            Some(Launch {
                terminal: "t1".into(),
                session: "s1".into(),
                mcp_token: None,
            })
        );
        assert_eq!(launches.find("nope"), None);
        launches.revoke("t1");
        assert_eq!(launches.find(&key), None);
        assert!(launches.find(&other).is_some());
    }

    #[test]
    fn a_second_key_for_a_terminal_replaces_the_first() {
        let launches = Launches::default();
        let first = launches.issue("t", "s", None).unwrap();
        let second = launches.issue("t", "s", None).unwrap();
        assert_eq!(launches.find(&first), None);
        assert!(launches.find(&second).is_some());
    }

    #[test]
    fn a_key_of_a_start_that_never_came_expires_but_a_running_terminals_does_not() {
        let launches = Launches::default();
        let waiting = launches.issue("waiting", "s", None).unwrap();
        let running = launches.issue("running", "s", None).unwrap();
        launches.started("running");
        for entry in launches.entries().values_mut() {
            entry.issued = Instant::now()
                .checked_sub(UNSPAWNED_KEY_TTL)
                .expect("the machine has been up longer than the key lasts");
        }
        launches.issue("new", "s", None).unwrap();
        assert_eq!(launches.find(&waiting), None);
        assert!(launches.find(&running).is_some());
    }
}
