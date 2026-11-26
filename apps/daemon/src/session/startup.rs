//! What one launch of a session has shown of an interactive program since it started, which tells
//! an exit at start from any other (`docs/plans/terminal-first-degradation.md`, D6): a hook or a
//! helper call of the launch reached the daemon, the program made a status report, or completed a
//! terminal title. The alternate screen and bracketed paste are no such sign: a CLI can turn both
//! on and exit at start (Codex refusing a permissions profile), or switch the alternate screen on
//! and off as it exits. A key a client typed into it is kept apart, since a person who quits has
//! to type. The evidence also holds, for the early-exit window, what the launch was asked and the
//! program and version it started (`super::relaunch`), for a launch one tier down.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use super::relaunch::{LaunchAsked, LowerTierKey};

/// What a launch's process has shown since it started, and what the launch was asked.
pub struct StartupEvidence {
    launched_at: Instant,
    shown: AtomicBool,
    typed: AtomicBool,
    shown_now: tokio::sync::Notify,
    asked: Mutex<Option<LaunchAsked>>,
    remembered_as: Mutex<Option<LowerTierKey>>,
}

impl Default for StartupEvidence {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupEvidence {
    /// Evidence of a launch starting now, with nothing shown yet.
    pub fn new() -> Self {
        Self {
            launched_at: Instant::now(),
            shown: AtomicBool::new(false),
            typed: AtomicBool::new(false),
            shown_now: tokio::sync::Notify::new(),
            asked: Mutex::new(None),
            remembered_as: Mutex::new(None),
        }
    }

    /// The program showed a sign of an interactive program, or a helper of its launch reached the
    /// daemon.
    pub fn note_shown(&self) {
        if !self.shown.swap(true, Ordering::AcqRel) {
            self.shown_now.notify_waiters();
        }
    }

    /// A client typed a key into the program: not the terminal's own protocol replies.
    pub fn note_typed(&self) {
        self.typed.store(true, Ordering::Release);
    }

    /// Whether the program has shown a sign of an interactive program ([`Self::note_shown`]).
    pub fn has_shown(&self) -> bool {
        self.shown.load(Ordering::Acquire)
    }

    /// Whether a client typed into the program ([`Self::note_typed`]).
    pub fn was_typed(&self) -> bool {
        self.typed.load(Ordering::Acquire)
    }

    /// When the launch's process was started.
    pub fn launched_at(&self) -> Instant {
        self.launched_at
    }

    /// Returns once the program has shown a sign of an interactive program.
    pub async fn until_shown(&self) {
        loop {
            let shown = self.shown_now.notified();
            if self.has_shown() {
                return;
            }
            shown.await;
        }
    }

    /// Keeps what the launch was asked, for a launch one tier down to repeat.
    pub fn keep_asked(&self, asked: LaunchAsked) {
        *self.asked.lock().expect("startup lock poisoned") = Some(asked);
    }

    /// What the launch was asked, where its launcher kept it ([`Self::keep_asked`]).
    pub fn asked(&self) -> Option<LaunchAsked> {
        self.asked.lock().expect("startup lock poisoned").clone()
    }

    /// Notes the program and version the launch started, under which a lower tier that stays up
    /// is remembered (`super::relaunch::LowerTiers`).
    pub fn note_remembered_as(&self, key: LowerTierKey) {
        *self.remembered_as.lock().expect("startup lock poisoned") = Some(key);
    }

    /// The program and version the launch started ([`Self::note_remembered_as`]).
    pub fn remembered_as(&self) -> Option<LowerTierKey> {
        self.remembered_as
            .lock()
            .expect("startup lock poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Something shown is noted once and wakes a waiter; a key typed is kept apart from it.
    #[tokio::test]
    async fn evidence_wakes_a_waiter_and_a_key_is_kept_apart() {
        let evidence = std::sync::Arc::new(StartupEvidence::new());
        evidence.note_typed();
        assert!(evidence.was_typed());
        assert!(!evidence.has_shown());
        let waiting = {
            let evidence = evidence.clone();
            tokio::spawn(async move { evidence.until_shown().await })
        };
        tokio::task::yield_now().await;
        evidence.note_shown();
        tokio::time::timeout(crate::test_support::PATIENCE, waiting)
            .await
            .expect("the waiter was woken")
            .unwrap();
        assert!(evidence.has_shown());
    }
}
