//! One lock per agent over changing and repairing the sharing of its accounts, and over an
//! account switch's copy into an account, and over an account removal's last judgement and the
//! move of its directory: every account of an agent writes into the same default directory, and
//! none of these may work on a directory another is changing.

use tokio::sync::{Mutex, MutexGuard};

use crate::protocol::Agent;

#[derive(Default)]
pub struct Locks {
    claude: Mutex<()>,
    codex: Mutex<()>,
    grok: Mutex<()>,
}

impl Locks {
    pub async fn lock(&self, agent: Agent) -> MutexGuard<'_, ()> {
        match agent {
            Agent::Claude => self.claude.lock().await,
            Agent::Codex => self.codex.lock().await,
            Agent::Grok => self.grok.lock().await,
        }
    }

    /// The agent's lock if no one holds it now.
    pub fn try_lock(&self, agent: Agent) -> Option<MutexGuard<'_, ()>> {
        match agent {
            Agent::Claude => self.claude.try_lock().ok(),
            Agent::Codex => self.codex.try_lock().ok(),
            Agent::Grok => self.grok.try_lock().ok(),
        }
    }
}
