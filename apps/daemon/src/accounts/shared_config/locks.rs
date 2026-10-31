//! One lock per agent over changing and repairing the sharing of its accounts, and over an
//! account switch's copy into an account, and over an account removal's last judgement and the
//! move of its directory: every account of an agent writes into the same default directory, and
//! none of these may work on a directory another is changing.

use tokio::sync::{Mutex, MutexGuard};

use crate::protocol::{Agent, PerAgent};

#[derive(Default)]
pub struct Locks(PerAgent<Mutex<()>>);

impl Locks {
    pub async fn lock(&self, agent: Agent) -> MutexGuard<'_, ()> {
        self.0.get(agent).lock().await
    }

    /// The agent's lock if no one holds it now.
    pub fn try_lock(&self, agent: Agent) -> Option<MutexGuard<'_, ()>> {
        self.0.get(agent).try_lock().ok()
    }
}
