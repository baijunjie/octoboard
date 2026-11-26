//! What one launch of a session carries of Octoboard's integration with its agent, and the answers
//! that follow from it. A provider answers some questions per launch, once, after its plan
//! (`AgentProvider::launch_has_status_integration`, the cancel that drops held messages, the
//! native stash, the tool-listing wait, the extension channel, the further notices); every generic
//! reader of such an answer goes through here rather than asking the provider, so that a launch
//! at the bare tier (`LaunchTier::Bare`), which carries none of the integration, is read as a
//! launch of a provider with none (`docs/plans/terminal-first-degradation.md`, D4) without the
//! provider being asked.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::adapter::{AgentProvider, AgentVersion, FirstHook, LaunchPlan, LaunchSpec, LaunchTier};
use crate::protocol::Notice;
use crate::session::ReportedSignals;

/// What a running session keeps of its launch: the tier it was started at, the version of the CLI
/// it started, and the provider's per-launch answers that the session reads for its whole life.
/// A daemon that adopts the launch rebuilds it from the terminal's meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Integration {
    pub tier: LaunchTier,
    /// The version of the agent's CLI the launch started, where it could be told.
    pub agent_version: Option<AgentVersion>,
    /// Whether the program's reports give the launch its status, as its provider answered for the
    /// launch; `None` where nobody asked (a launch adopted from a build that did not record it, a
    /// stand-in), which takes the provider's answer for all its launches.
    pub status_from_reports: Option<bool>,
    /// Whether a cancel of the launch can drop the messages its agent held, as its provider
    /// answered for the launch; `None` where nobody asked, which takes the provider's reader's
    /// declaration.
    pub cancel_drops: Option<bool>,
    /// When the launch's first hook is due, as its provider answered for the launch
    /// (`AgentProvider::first_hook_due`); `Never` where nobody asked, and for a bare launch, which
    /// is without hooks from its start ([`HookPresence`]).
    pub first_hook: FirstHook,
}

impl Integration {
    /// Whether the launch runs no hook of Octoboard's at all: a bare launch, given none, or an
    /// integrated one whose plan expects none ([`FirstHook::Never`]: a provider's plan that left its
    /// hooks out). Its trust watch then takes its input box, drawn with no trust screen up, for the
    /// start hook it will not run (`crate::session::trust::TrustState::note_start_hook_absent`), so
    /// that whatever else its start signal waits for (a terminal title a configuration may never
    /// draw) cannot hold Octoboard's messages for good.
    pub fn runs_no_hook(&self) -> bool {
        !self.tier.integrated() || self.first_hook == FirstHook::Never
    }

    /// A launch at the enhanced tier whose provider was not asked its per-launch answers, which
    /// then take the provider's answers for all its launches: a stand-in, or a launch adopted from
    /// a build that recorded none of them.
    #[cfg(test)]
    pub fn unasked() -> Self {
        Self {
            tier: LaunchTier::Enhanced,
            agent_version: None,
            status_from_reports: None,
            cancel_drops: None,
            first_hook: FirstHook::Never,
        }
    }

    /// Whether the program's reports (OSC 7501) give the launch its status: always for a bare
    /// launch, whose hooks are not there; otherwise as the provider answered.
    pub fn status_from_reports(&self, provider: &dyn AgentProvider) -> bool {
        if !self.tier.integrated() {
            return true;
        }
        self.status_from_reports
            .unwrap_or_else(|| !provider.has_status_integration())
    }

    /// Whether a cancel of the launch can drop the messages its agent held: never for a bare
    /// launch, which has no reports that tell a message a turn took; otherwise as the provider
    /// answered.
    pub fn cancel_drops(&self, provider: &dyn AgentProvider) -> bool {
        if !self.tier.integrated() {
            return false;
        }
        self.cancel_drops
            .unwrap_or_else(|| provider.screen().cancel_drops_held_messages())
    }

    /// What the program's reports say beside the provider's status integration: nothing where
    /// the reports give the status.
    pub fn reported_signals(&self, provider: &dyn AgentProvider) -> ReportedSignals {
        if self.status_from_reports(provider) {
            ReportedSignals::NONE
        } else {
            provider.reported_signals()
        }
    }
}

/// The provider's answers for one launch as planned, asked once, after the plan: the session's
/// [`Integration`] and what the launch itself acts on.
pub struct LaunchAnswers {
    pub integration: Integration,
    /// Whether a stash key is used: only where the provider says this launch was measured to take
    /// it (`ScreenReader::native_stash`).
    pub native_stash: bool,
    /// How long the launch's first message waits for the session's MCP server to have answered a
    /// listing of its tools (`AgentProvider::awaits_tool_listing`).
    pub tool_listing_wait: Option<Duration>,
    /// Whether the agent runs an extension that opens the session's channel
    /// (`AgentProvider::extension_channel`).
    pub extension_channel: bool,
    /// What else the user is told of the launch (`AgentProvider::further_launch_notices`).
    pub further_notices: Vec<Notice>,
}

impl LaunchAnswers {
    /// The answers for a launch of `spec` at `tier` planned as `plan`: the provider's, for a tier
    /// that carries the integration; for a bare launch those of a provider with none (status from
    /// the program's reports, no cancel that drops messages, no native stash, no tool-listing
    /// wait, no extension channel, nothing further to tell), without asking the provider.
    pub fn of(
        provider: &dyn AgentProvider,
        tier: LaunchTier,
        spec: &LaunchSpec<'_>,
        plan: &LaunchPlan,
    ) -> Self {
        let agent_version = spec.agent_version;
        if !tier.integrated() {
            return Self {
                integration: Integration {
                    tier,
                    agent_version,
                    status_from_reports: Some(true),
                    cancel_drops: Some(false),
                    first_hook: FirstHook::Never,
                },
                native_stash: false,
                tool_listing_wait: None,
                extension_channel: false,
                further_notices: Vec::new(),
            };
        }
        Self {
            integration: Integration {
                tier,
                agent_version,
                status_from_reports: Some(!provider.launch_has_status_integration(spec, plan)),
                cancel_drops: Some(provider.launch_cancel_drops_held_messages(spec, plan)),
                first_hook: provider.first_hook_due(spec, plan),
            },
            native_stash: provider.screen().native_stash(spec, plan),
            tool_listing_wait: provider.awaits_tool_listing(spec, plan),
            extension_channel: provider.extension_channel(spec, plan),
            further_notices: provider.further_launch_notices(spec, plan),
        }
    }
}

/// Where a launch stands with its hooks (`docs/plans/terminal-first-degradation.md`, D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hooks {
    /// None is expected ([`FirstHook::Never`]): nothing of the hooks' absence applies.
    NotExpected,
    /// One is expected and none has come yet, nor has the bound run out.
    Expected,
    /// None came within the bound after one was due, or the launch is bare: every generic reader
    /// of a fact that only holds with hooks takes the no-hooks answer.
    Absent,
    /// A hook has come.
    Present,
}

const NOT_EXPECTED: u8 = 0;
const EXPECTED: u8 = 1;
const ABSENT: u8 = 2;
const PRESENT: u8 = 3;

/// What a running launch keeps of its hooks: whether they are expected, have come, or are taken
/// to be absent, and whether the first is due yet. Entered and left by the watch in
/// `crate::session::hook_absence` and by the hooks themselves; not stored, since a daemon that
/// adopts the launch starts the watch again. Dropped with the session, which ends a wait for the
/// first hook to fall due ([`DueWait`]).
pub struct HookPresence(Arc<Presence>);

struct Presence {
    state: AtomicU8,
    /// The first hook comes with the first turn, so it is due only once a prompt has gone in or
    /// a turn has been seen running ([`HookPresence::note_due`]).
    with_first_turn: AtomicBool,
    due: AtomicBool,
    /// The session holding this is gone.
    closed: AtomicBool,
    changed: tokio::sync::Notify,
}

impl Drop for HookPresence {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::Release);
        self.0.changed.notify_waiters();
    }
}

fn hooks_of(state: u8) -> Hooks {
    match state {
        NOT_EXPECTED => Hooks::NotExpected,
        EXPECTED => Hooks::Expected,
        ABSENT => Hooks::Absent,
        _ => Hooks::Present,
    }
}

impl HookPresence {
    /// The hooks of a launch of `integration`: absent from the start for a bare launch, not
    /// expected where none is, and otherwise expected, due at once for [`FirstHook::AtStart`].
    pub fn of(integration: &Integration) -> Self {
        let (state, with_first_turn) = if !integration.tier.integrated() {
            (ABSENT, false)
        } else {
            match integration.first_hook {
                FirstHook::Never => (NOT_EXPECTED, false),
                FirstHook::AtStart => (EXPECTED, false),
                FirstHook::WithFirstTurn => (EXPECTED, true),
            }
        };
        Self(Arc::new(Presence {
            state: AtomicU8::new(state),
            with_first_turn: AtomicBool::new(with_first_turn),
            due: AtomicBool::new(state == EXPECTED && !with_first_turn),
            closed: AtomicBool::new(false),
            changed: tokio::sync::Notify::new(),
        }))
    }

    /// Whether a first hook is due now, for a test.
    #[cfg(test)]
    pub fn is_due(&self) -> bool {
        self.0.due.load(Ordering::Acquire)
    }

    /// Where the launch stands with its hooks now.
    pub fn state(&self) -> Hooks {
        hooks_of(self.0.state.load(Ordering::Acquire))
    }

    /// Whether the launch is taken to be without hooks ([`Hooks::Absent`]).
    pub fn absent(&self) -> bool {
        self.0.state.load(Ordering::Acquire) == ABSENT
    }

    /// The launch was adopted from a holder the previous daemon left running: whatever start hook
    /// its agent ran came before this daemon, and does not come again, so an expected first hook
    /// is due only with the next turn.
    pub fn note_adopted(&self) {
        if self.0.state.load(Ordering::Acquire) == EXPECTED {
            self.0.with_first_turn.store(true, Ordering::Release);
            self.0.due.store(false, Ordering::Release);
        }
    }

    /// A prompt has gone into the session, or its terminal has shown a turn running: a first hook
    /// that comes with the first turn is due from now on.
    pub fn note_due(&self) {
        if self.0.with_first_turn.load(Ordering::Acquire)
            && self.0.state.load(Ordering::Acquire) == EXPECTED
            && !self.0.due.swap(true, Ordering::AcqRel)
        {
            self.0.changed.notify_waiters();
        }
    }

    /// A wait for the first hook to fall due that does not keep the session alive.
    pub fn due_wait(&self) -> DueWait {
        DueWait(self.0.clone())
    }

    /// The bound ran out with a hook due and none come: the launch is absent from now on. Says
    /// whether it moved, which it does only from the expected state.
    pub fn enter_absent(&self) -> bool {
        self.0
            .state
            .compare_exchange(EXPECTED, ABSENT, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// A hook of the launch has come: present from now on. Says where it stood before.
    pub fn note_hook(&self) -> Hooks {
        let before = hooks_of(self.0.state.swap(PRESENT, Ordering::AcqRel));
        // A wait for the first hook to fall due ends with the hook itself.
        self.0.changed.notify_waiters();
        before
    }
}

/// A wait for a launch's first hook to fall due ([`HookPresence::due_wait`]).
pub struct DueWait(Arc<Presence>);

impl DueWait {
    /// Returns `true` once the first hook is due, and `false` once it no longer can be: a hook has
    /// come, or the session is gone.
    pub async fn until_due(&self) -> bool {
        loop {
            let changed = self.0.changed.notified();
            if self.0.closed.load(Ordering::Acquire)
                || self.0.state.load(Ordering::Acquire) != EXPECTED
            {
                return false;
            }
            if self.0.due.load(Ordering::Acquire) {
                return true;
            }
            changed.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;
    use crate::protocol::AgentId;

    /// A bare launch of any provider takes the answers of a provider with no integration, whatever
    /// the provider would answer for an enhanced launch of the same spec.
    #[test]
    fn a_bare_launch_takes_the_no_integration_answers_for_every_provider() {
        for agent in AgentId::ALL {
            let provider = crate::adapter::provider(agent);
            let fixture = spec_fixture();
            let spec = fixture.spec();
            let plan = LaunchPlan::default();
            let answers = LaunchAnswers::of(provider, LaunchTier::Bare, &spec, &plan);
            assert_eq!(answers.integration.tier, LaunchTier::Bare, "{agent:?}");
            assert!(
                answers.integration.status_from_reports(provider),
                "{agent:?}"
            );
            assert!(!answers.integration.cancel_drops(provider), "{agent:?}");
            assert_eq!(
                answers.integration.reported_signals(provider),
                ReportedSignals::NONE,
                "{agent:?}"
            );
            assert!(!answers.native_stash, "{agent:?}");
            assert_eq!(answers.tool_listing_wait, None, "{agent:?}");
            assert!(!answers.extension_channel, "{agent:?}");
            assert!(answers.further_notices.is_empty(), "{agent:?}");
        }
    }

    /// A bare tier read back from a meta overrides whatever the recorded answers say, so a bare
    /// launch is never read as having hooks.
    #[test]
    fn a_bare_integration_ignores_recorded_answers() {
        let provider = crate::adapter::provider(AgentId::CLAUDE);
        let bare = Integration {
            tier: LaunchTier::Bare,
            agent_version: None,
            status_from_reports: Some(false),
            cancel_drops: Some(true),
            first_hook: FirstHook::AtStart,
        };
        assert!(bare.status_from_reports(provider));
        assert!(!bare.cancel_drops(provider));
    }

    /// An enhanced launch asks the provider: the test-only provider's switches decide.
    #[test]
    fn an_enhanced_launch_asks_the_provider() {
        let _wait =
            crate::test_support::test_provider::declaring_tool_listing_wait(Duration::from_secs(3));
        let _channel = crate::test_support::test_provider::declaring_extension_channel();
        let provider = crate::adapter::provider(AgentId::TEST);
        let fixture = spec_fixture();
        let spec = fixture.spec();
        let plan = LaunchPlan::default();
        let answers = LaunchAnswers::of(provider, LaunchTier::Enhanced, &spec, &plan);
        assert_eq!(answers.tool_listing_wait, Some(Duration::from_secs(3)));
        assert!(answers.extension_channel);
        let bare = LaunchAnswers::of(provider, LaunchTier::Bare, &spec, &plan);
        assert_eq!(bare.tool_listing_wait, None);
        assert!(!bare.extension_channel);
    }
}
