//! Whether a record not found proves its conversation gone: only where the provider's reader has,
//! in this daemon's life, recognised a record written by the CLI version now installed. A release
//! that renames or moves its records leaves its records unrecognised, so a record not found under
//! it proves nothing, and nothing is refused, forgotten or deleted for it.
//!
//! A witness is noted by the record collector ([`Watch::observe`]) for a running session whose
//! record the same search a resume's judgement makes ([`crate::records::resumability`]) finds, and
//! finds written while that launch runs. It is kept per agent and the version the launch started,
//! never the version installed when it is noted, since a CLI updates itself under running sessions.
//! It is runtime state, never stored: a new daemon has none until its sessions write again.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use crate::adapter::AgentVersion;
use crate::protocol::{AgentId, Session};
use crate::records::locate::{modified, Locator};

/// How often a running session not yet witnessed has its record looked for. The look walks the
/// account's records like a resume's judgement does, so it is not made on every pass.
const LOOK_EVERY: Duration = Duration::from_secs(10);

#[cfg(test)]
thread_local! {
    /// What [`look_every`] answers on this thread in place of [`LOOK_EVERY`]: zero has every
    /// observation look, a long interval has none but the first. Read on the collector's own
    /// thread, which a test calls a pass on directly.
    pub(crate) static LOOK_EVERY_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// How long [`Watch::observe`] leaves between two looks: [`LOOK_EVERY`], or a test's override.
fn look_every() -> Duration {
    #[cfg(test)]
    if let Some(every) = LOOK_EVERY_OVERRIDE.get() {
        return every;
    }
    LOOK_EVERY
}

/// The agent versions whose records the reader was seen to recognise, by agent.
#[derive(Default)]
pub struct Witnesses {
    seen: Mutex<HashSet<(AgentId, AgentVersion)>>,
    /// The version tests say is installed for each agent: no test resolves a real CLI.
    #[cfg(test)]
    installed: Mutex<HashMap<AgentId, AgentVersion>>,
}

impl Witnesses {
    /// Notes that `agent`'s reader recognised a record its CLI at `version` wrote.
    pub(crate) fn note(&self, agent: AgentId, version: AgentVersion) {
        self.seen
            .lock()
            .expect("record witness lock poisoned")
            .insert((agent, version));
    }

    /// Whether a record written by `agent`'s CLI at `version` was recognised.
    pub(crate) fn holds(&self, agent: AgentId, version: AgentVersion) -> bool {
        self.seen
            .lock()
            .expect("record witness lock poisoned")
            .contains(&(agent, version))
    }

    /// The version of `agent`'s CLI a launch on the account at `config_dir` (the default account
    /// when `None`) would start, read as the launch reads it; `None` where it cannot be told,
    /// which leaves the account without a witness. `shell_env` is asked only when needed.
    fn installed_version<'a>(
        &self,
        agent: AgentId,
        config_dir: Option<&str>,
        shell_env: impl FnOnce() -> Option<&'a HashMap<String, String>>,
    ) -> Option<AgentVersion> {
        #[cfg(test)]
        {
            let _ = (config_dir, shell_env);
            self.installed
                .lock()
                .expect("record witness lock poisoned")
                .get(&agent)
                .copied()
        }
        #[cfg(not(test))]
        {
            crate::adapter::installed_version_for(
                agent,
                config_dir.map(std::path::Path::new),
                shell_env()?,
            )
        }
    }

    /// Says `version` is installed for `agent` and witnessed.
    #[cfg(test)]
    pub(crate) fn witness_installed(&self, agent: AgentId, version: AgentVersion) {
        self.install(agent, version);
        self.note(agent, version);
    }

    /// Says `version` is installed for `agent`, witnessed or not.
    #[cfg(test)]
    pub(crate) fn install(&self, agent: AgentId, version: AgentVersion) {
        self.installed
            .lock()
            .expect("record witness lock poisoned")
            .insert(agent, version);
    }
}

/// The versions read as installed for each account, by agent and its pinned directory, read once
/// for as long as a [`Locator`] lives.
pub(crate) type InstalledVersions = HashMap<(AgentId, Option<String>), Option<AgentVersion>>;

/// Whether a record of `session` missing from its account proves the conversation gone: `witnesses`
/// hold the version installed for the session's account. Without `witnesses`, or a version that
/// cannot be read, it never does.
pub(crate) fn witnessed<'a>(
    witnesses: Option<&Witnesses>,
    installed: &mut InstalledVersions,
    session: &Session,
    shell_env: impl FnOnce() -> Option<&'a HashMap<String, String>>,
) -> bool {
    let Some(witnesses) = witnesses else {
        return false;
    };
    let version = *installed
        .entry((session.agent, session.config_dir.clone()))
        .or_insert_with(|| {
            witnesses.installed_version(session.agent, session.config_dir.as_deref(), shell_env)
        });
    version.is_some_and(|version| witnesses.holds(session.agent, version))
}

/// A running session's progress toward a witness, kept by the record collector for each session.
#[derive(Default)]
pub(crate) struct Watch {
    /// The launch the looks below are of; a new launch starts over.
    launch: Option<String>,
    /// When the record was last looked for.
    looked: Option<Instant>,
    /// What the first look of the launch found: the record and when it was last written, or
    /// `None` when there was none yet. Unset before the first look.
    first: Option<Option<(PathBuf, Option<SystemTime>)>>,
}

impl Watch {
    /// Looks, at most every [`LOOK_EVERY`] ([`look_every`]), for the record of `session`, running as `launch_id` at
    /// `version`, the way a resume's judgement looks for it, and notes a witness for its agent and
    /// version once a look finds it written since the launch's first look: a record that first
    /// look did not find, another record, or the same one written again. A record only found as
    /// it stood at the first look could be a release's earlier records, which a resume under it
    /// does not read.
    pub(crate) fn observe(
        &mut self,
        witnesses: &Witnesses,
        locator: &Locator,
        session: &Session,
        launch_id: &str,
        version: AgentVersion,
    ) {
        if witnesses.holds(session.agent, version) {
            return;
        }
        if self.launch.as_deref() != Some(launch_id) {
            *self = Watch {
                launch: Some(launch_id.to_owned()),
                ..Watch::default()
            };
        }
        if self
            .looked
            .is_some_and(|looked| looked.elapsed() < look_every())
        {
            return;
        }
        self.looked = Some(Instant::now());
        let found = crate::records::resumability::found_record(locator, session).map(|path| {
            let written = modified(&path);
            (path, written)
        });
        let Some(first) = &self.first else {
            self.first = Some(found);
            return;
        };
        let written_since = match (first, &found) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some((was, then)), Some((path, now))) => was != path || now > then,
        };
        if written_since {
            witnesses.note(session.agent, version);
        }
    }
}

#[cfg(test)]
mod tests;
