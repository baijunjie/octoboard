//! What a launch one tier down after an early exit repeats and remembers
//! (`docs/plans/terminal-first-degradation.md`, D6): what the exited launch was asked, the
//! relaunch the exit watcher hands to the operation following the launch, and the lower tiers that
//! stayed up, remembered per agent, program and version. What tells an exit at start from any
//! other is `super::startup`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::adapter::{AgentVersion, EarlyExit, LaunchTier};
use crate::protocol::{AgentId, Notice, SessionStatus, TerminalAppearance};

/// What a launch was asked beside what the session's record gives every launch of it: the task,
/// the conversation to resume, the terminal's appearance, the shell environment the caller
/// resolved something from, the notices the caller already knew of, and the tier and early exit
/// the launch follows. A launch one tier down repeats it.
#[derive(Debug, Clone)]
pub struct LaunchAsked {
    pub task: Option<String>,
    pub resume_agent_session_id: Option<String>,
    pub terminal_appearance: Option<TerminalAppearance>,
    pub shell_env: Option<crate::env_shell::LaunchEnv>,
    pub notices: Vec<Notice>,
    /// The highest tier the launch could use.
    pub highest_tier: LaunchTier,
    /// The early exit of the higher tier this launch follows, if it does.
    pub early_exit: Option<EarlyExit>,
    /// The conversation id the launch recorded for its session, if it recorded one.
    pub recorded_conversation: Option<String>,
}

impl LaunchAsked {
    /// A first launch at the enhanced tier of `task` or `resume_agent_session_id`.
    pub fn new(
        task: Option<String>,
        resume_agent_session_id: Option<String>,
        terminal_appearance: Option<TerminalAppearance>,
        shell_env: Option<crate::env_shell::LaunchEnv>,
        notices: Vec<Notice>,
    ) -> Self {
        Self {
            task,
            resume_agent_session_id,
            terminal_appearance,
            shell_env,
            notices,
            highest_tier: LaunchTier::Enhanced,
            early_exit: None,
            recorded_conversation: None,
        }
    }
}

/// A launch one tier down that the exit watcher hands to the operation holding the session's
/// claim and watching its launch come up, which relaunches under that claim.
#[derive(Debug, Clone)]
pub struct EarlyExitRelaunch {
    /// What the exited launch was asked.
    pub asked: LaunchAsked,
    /// How its process exited.
    pub exit: EarlyExit,
    /// The highest tier the relaunch may use.
    pub tier: LaunchTier,
    /// The status the session's record had before it was set back to starting.
    pub previous_status: SessionStatus,
    /// The record as the exited process ran under it, for what it left of a sharing account's
    /// registries to be put back should the relaunch fail
    /// (`crate::accounts::shared_config::keep_registries_after_exit`).
    pub ran_as: Option<crate::protocol::Session>,
}

/// What a lower tier is remembered under: the agent, the program its launch started and the
/// version that program reported.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LowerTierKey {
    pub agent: AgentId,
    pub program: PathBuf,
    pub version: Option<AgentVersion>,
}

/// How long a lower tier that stayed up is remembered: later launches of the same program and
/// version start there in the meantime, and still tell the user.
pub const LOWER_TIER_KEPT: Duration = Duration::from_secs(10 * 60);

/// A lower tier recalled for a launch ([`LowerTiers::recalled`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecalledTier {
    pub tier: LaunchTier,
    /// The early exit of the tier above it, which a reduced plan is given again.
    pub exit: EarlyExit,
}

/// The lower tiers that stayed up after an early exit, each for [`LOWER_TIER_KEPT`]. Held on
/// `AppState`, not stored: a daemon that starts again measures again.
#[derive(Default)]
pub struct LowerTiers {
    held: Mutex<HashMap<LowerTierKey, (Instant, RecalledTier)>>,
}

impl LowerTiers {
    /// Remembers that a launch at `tier` stayed up for `key` after the tier above it exited as
    /// `exit`.
    pub fn remember(&self, key: LowerTierKey, tier: LaunchTier, exit: EarlyExit) {
        self.remember_at(key, tier, exit, Instant::now());
    }

    fn remember_at(&self, key: LowerTierKey, tier: LaunchTier, exit: EarlyExit, at: Instant) {
        let mut held = self.held.lock().expect("lower tiers lock poisoned");
        held.retain(|_, (since, _)| at.saturating_duration_since(*since) < LOWER_TIER_KEPT);
        held.insert(key, (at, RecalledTier { tier, exit }));
    }

    /// The lower tier remembered for `key`, while it is kept.
    pub fn recalled(&self, key: &LowerTierKey) -> Option<RecalledTier> {
        self.recalled_at(key, Instant::now())
    }

    fn recalled_at(&self, key: &LowerTierKey, at: Instant) -> Option<RecalledTier> {
        let held = self.held.lock().expect("lower tiers lock poisoned");
        held.get(key)
            .filter(|(since, _)| at.saturating_duration_since(*since) < LOWER_TIER_KEPT)
            .map(|(_, recalled)| recalled.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(version: Option<AgentVersion>) -> LowerTierKey {
        LowerTierKey {
            agent: AgentId::TEST,
            program: PathBuf::from("/bin/agent"),
            version,
        }
    }

    fn exit() -> EarlyExit {
        EarlyExit {
            output: "unknown option".to_string(),
            status: None,
        }
    }

    /// A lower tier is recalled for the same agent, program and version for its whole period,
    /// and no longer after it; another version recalls nothing.
    #[test]
    fn a_lower_tier_is_recalled_for_its_program_and_version_while_it_is_kept() {
        let tiers = LowerTiers::default();
        let at = Instant::now();
        tiers.remember_at(key(Some((1, 2, 3))), LaunchTier::Bare, exit(), at);
        let recalled = tiers.recalled_at(&key(Some((1, 2, 3))), at + LOWER_TIER_KEPT / 2);
        assert_eq!(
            recalled,
            Some(RecalledTier {
                tier: LaunchTier::Bare,
                exit: exit()
            })
        );
        assert_eq!(tiers.recalled_at(&key(Some((1, 2, 4))), at), None);
        assert_eq!(tiers.recalled_at(&key(None), at), None);
        assert_eq!(
            tiers.recalled_at(&key(Some((1, 2, 3))), at + LOWER_TIER_KEPT),
            None
        );
    }
}
