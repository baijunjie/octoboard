//! Working out, once per daemon start, which agents are available and what each one's default
//! account resolves to. An agent is available when its binary resolves on the user's login shell
//! `PATH`, and that is the only test: whether its config directory holds a login is not
//! Octoboard's business, since a user may authenticate with an API key from their shell, a
//! credential helper, or an organization's gateway, and in none of those cases does the config
//! directory show it either way. Both are derived from one login-shell snapshot, the same kind a
//! launch takes, and carried as derived state of its own on `AppState` rather than a field of the
//! stored `Settings`, which is written only by the user's own updates (and, once, by the fill of
//! the added agents described below).
//!
//! This creates no account: the default account exists for every agent by construction, and
//! nothing here mints, stores or removes a row — only what each agent's own `AgentAvailability`
//! shows is computed.
//!
//! The list of added agents is decided here too, once, as soon as availability is: it is filled with
//! the agents that are available plus those that have accounts, and never while any agent is still
//! `NotDetermined`, since an installed agent would be left off it. A run whose snapshot does not
//! complete therefore leaves the list undecided (clients see `null`) and tries again at the next
//! start; `add_agent` is refused meanwhile, so nothing takes the list over before it is filled.
//!
//! A snapshot that does not complete — any of the three failure modes `env_shell::snapshot`
//! reports — leaves every agent exactly as it started, `NotDetermined`, rather than failing the
//! daemon's own start or being read as `Unavailable`: the launch's own refusal already covers a
//! binary that turns out not to be there.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::adapter;
use crate::env_shell;
use crate::protocol::{Agent, AgentAvailability, Availability};
use crate::state::AppState;

/// Determines availability on a blocking thread and publishes the result once it lands, without
/// holding up the daemon's own start: `run_daemon` fires this and moves on to serving clients.
pub fn spawn_determine(state: Arc<AppState>) {
    tokio::task::spawn_blocking(move || apply_snapshot(&state, env_shell::snapshot()));
}

/// Applies the outcome of one login-shell snapshot attempt. On success, publishes what it
/// determined and then fills the added agents (see [`fill_added_agents`]); on any of
/// `env_shell::snapshot`'s three ways to fail — a timeout, a non-zero exit, or a clean exit with an
/// incomplete dump — does nothing at all: `AppState`'s agent availability stays exactly as it
/// started (every agent `NotDetermined`) and nothing is broadcast, since nothing changed. Split out from `spawn_determine` so the no-op side of that branch is testable
/// without a real shell.
fn apply_snapshot(state: &AppState, outcome: Result<HashMap<String, String>>) {
    if let Ok(shell_env) = outcome {
        state.set_agent_availability(determine(&shell_env));
        if let Err(err) = fill_added_agents(state) {
            tracing::warn!(%err, "could not fill the added agents");
        }
    }
}

/// Fills the added agents once with the agents that are available plus those that have accounts,
/// in `Agent::ALL` order, and broadcasts the settings when it did. Does nothing while any agent's
/// availability is undetermined or once the list has been decided — a list the user saved empty
/// stays empty. A fresh install has no accounts, so the same rule gives it the available agents.
pub fn fill_added_agents(state: &AppState) -> Result<()> {
    let availability = state.agent_availability();
    if availability
        .iter()
        .any(|entry| entry.availability == Availability::NotDetermined)
    {
        return Ok(());
    }
    if state.store.added_agents()?.is_some() {
        return Ok(());
    }
    let filled = {
        // The read of the accounts and the write are one step: an account created in between would
        // otherwise be missed.
        let _writes = state.lock_account_writes();
        let accounts = state.store.list_accounts()?;
        let agents: Vec<Agent> = availability
            .iter()
            .filter(|entry| {
                entry.availability == Availability::Available
                    || accounts.iter().any(|account| account.agent == entry.agent)
            })
            .map(|entry| entry.agent)
            .collect();
        state.store.set_added_agents_if_undecided(&agents)?
    };
    if filled {
        crate::coordinator::broadcast_settings(state)?;
    }
    Ok(())
}

/// Pure over an already-captured shell snapshot, so the three outcomes — available, unavailable,
/// and each agent's resolved default-account directory — are unit-tested without a real shell or
/// the developer's own `PATH` and `HOME`.
fn determine(shell_env: &HashMap<String, String>) -> Vec<AgentAvailability> {
    Agent::ALL
        .into_iter()
        .map(|agent| {
            let binary = adapter::adapter_for(agent).binary();
            let availability = if env_shell::resolve_binary(binary, shell_env).is_ok() {
                Availability::Available
            } else {
                Availability::Unavailable
            };
            AgentAvailability {
                agent,
                availability,
                default_account_dir: Some(
                    adapter::default_account_dir(agent, shell_env)
                        .to_string_lossy()
                        .into_owned(),
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Event;

    /// A binary on `PATH` is available, and a directory variable it exports wins over the agent's
    /// own default — Claude Code's reading of `CLAUDE_CONFIG_DIR`, generalised across all three.
    #[test]
    fn an_available_agent_shows_its_exported_directory() {
        let dir = crate::test_support::ScratchDir::new("availability-test-bin");
        let claude = dir.join("claude");
        std::fs::write(&claude, "#!/bin/sh\n").expect("fake claude");
        let mut perms = std::fs::metadata(&claude).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
        std::fs::set_permissions(&claude, perms).expect("chmod fake claude");

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), dir.to_string_lossy().into_owned());
        env.insert(
            "CLAUDE_CONFIG_DIR".to_string(),
            "/custom/claude-dir".to_string(),
        );
        // Codex and Grok, which this test asserts nothing about, still have their default
        // directories computed; without a `HOME` entry here that falls back to the real one.
        env.insert("HOME".to_string(), "/snapshot-home".to_string());

        let result = determine(&env);
        let claude_entry = result
            .iter()
            .find(|entry| entry.agent == Agent::Claude)
            .expect("a Claude entry");
        assert_eq!(claude_entry.availability, Availability::Available);
        assert_eq!(
            claude_entry.default_account_dir,
            Some("/custom/claude-dir".to_string())
        );
    }

    /// With no variable exported, the default account falls back to the agent's own usual
    /// default under the snapshot's `HOME` — never the daemon's own.
    #[test]
    fn with_nothing_exported_the_default_falls_back_to_the_agents_own_default() {
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), String::new());
        env.insert("HOME".to_string(), "/snapshot-home".to_string());

        let result = determine(&env);
        let codex_entry = result
            .iter()
            .find(|entry| entry.agent == Agent::Codex)
            .expect("a Codex entry");
        assert_eq!(codex_entry.availability, Availability::Unavailable);
        assert_eq!(
            codex_entry.default_account_dir,
            Some("/snapshot-home/.codex".to_string())
        );
    }

    /// Grok's entry carries its *source* home, never a per-session home: nothing here ever builds
    /// one, since this runs once per daemon start rather than once per launch.
    #[test]
    fn groks_entry_is_its_source_home() {
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), String::new());
        env.insert("GROK_HOME".to_string(), "/snapshot-grok-home".to_string());
        // Claude and Codex, which this test asserts nothing about, still have their default
        // directories computed; without a `HOME` entry here that falls back to the real one.
        env.insert("HOME".to_string(), "/snapshot-home".to_string());

        let result = determine(&env);
        let grok_entry = result
            .iter()
            .find(|entry| entry.agent == Agent::Grok)
            .expect("a Grok entry");
        assert_eq!(
            grok_entry.default_account_dir,
            Some("/snapshot-grok-home".to_string())
        );
    }

    /// A snapshot that does not complete must not be read as every agent being unavailable: it
    /// leaves availability exactly as it started (`NotDetermined`) and publishes nothing, so a
    /// client already connected sees no change at all.
    #[test]
    fn a_snapshot_that_does_not_complete_leaves_availability_not_determined() {
        let (state, _dir) = crate::test_support::app_state("availability-snapshot-failed");
        let mut events = state.subscribe();

        apply_snapshot(&state, Err(anyhow::anyhow!("the snapshot shell timed out")));

        for agent in Agent::ALL {
            assert_eq!(
                state.agent_availability_of(agent).availability,
                Availability::NotDetermined
            );
        }
        assert!(
            events.try_recv().is_err(),
            "a failed snapshot must broadcast nothing, since nothing changed"
        );
    }

    /// Nothing here mints, stores or removes an account: `determine` never touches `AppState` or
    /// its `Store` at all (it is pure over a snapshot), and publishing its result through
    /// `AppState::set_agent_availability` leaves the account table exactly as it was — the one way
    /// this milestone could break every existing user. Seeds one account per agent first, so
    /// "removes nothing" is actually exercised rather than checked against a table that was
    /// already empty.
    #[test]
    fn determining_availability_creates_or_removes_no_account() {
        let (state, _dir) = crate::test_support::app_state("availability-mints-no-account");
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), String::new());
        env.insert(
            "HOME".to_string(),
            "/availability-mints-no-account-home".to_string(),
        );

        let seeded: Vec<crate::protocol::Account> = Agent::ALL
            .into_iter()
            .map(|agent| crate::protocol::Account {
                id: format!("account-{agent:?}"),
                agent,
                name: format!("{agent:?} login"),
                config_dir: format!("/accounts/{agent:?}"),
                approval_reviewer: None,
                approval_mode: None,
            })
            .collect();
        for account in &seeded {
            state.store.insert_account(account).unwrap();
        }

        state.set_agent_availability(determine(&env));

        assert_eq!(state.store.list_accounts().unwrap(), seeded);
        for agent in Agent::ALL {
            assert_eq!(
                state.agent_availability_of(agent).availability,
                Availability::Unavailable
            );
        }
    }

    fn set_availability(state: &AppState, availability: [Availability; 3]) {
        state.set_agent_availability(
            Agent::ALL
                .into_iter()
                .zip(availability)
                .map(|(agent, availability)| AgentAvailability {
                    agent,
                    availability,
                    default_account_dir: None,
                })
                .collect(),
        );
    }

    fn seed_account(state: &AppState, agent: Agent) {
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: format!("account-{agent:?}"),
                agent,
                name: format!("{agent:?} login"),
                config_dir: format!("/accounts/{agent:?}"),
                approval_reviewer: None,
                approval_mode: None,
            })
            .unwrap();
    }

    /// An upgrade fills the list with the available agents plus those that have accounts, in the
    /// agents' own order, and tells clients.
    #[test]
    fn the_upgrade_fill_is_the_available_agents_and_those_with_accounts() {
        let (state, _dir) = crate::test_support::app_state("availability-upgrade-fill");
        seed_account(&state, Agent::Codex);
        let mut events = state.subscribe();
        set_availability(
            &state,
            [
                Availability::Available,
                Availability::Unavailable,
                Availability::Unavailable,
            ],
        );
        assert_eq!(state.store.added_agents().unwrap(), None);

        fill_added_agents(&state).unwrap();

        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Claude, Agent::Codex])
        );
        let mut announced = false;
        while let Ok(event) = events.try_recv() {
            announced |= matches!(
                event,
                Event::SettingsUpdated { settings }
                    if settings.added_agents == Some(vec![Agent::Claude, Agent::Codex])
            );
        }
        assert!(announced, "the fill must broadcast the settings");
    }

    /// A list the user emptied is decided, so a later fill leaves it empty.
    #[test]
    fn a_list_saved_empty_stays_empty() {
        let (state, _dir) = crate::test_support::app_state("availability-empty-stays");
        state.store.set_added_agents_if_undecided(&[]).unwrap();
        seed_account(&state, Agent::Claude);
        set_availability(&state, [Availability::Available; 3]);

        fill_added_agents(&state).unwrap();

        assert_eq!(state.store.added_agents().unwrap(), Some(vec![]));
    }

    /// While any agent is undetermined the fill waits, rather than reading "not yet known" as "not
    /// installed" and leaving an installed agent off the list.
    #[test]
    fn the_fill_waits_for_availability_to_be_determined() {
        let (state, _dir) = crate::test_support::app_state("availability-fill-waits");
        let mut events = state.subscribe();

        fill_added_agents(&state).unwrap();

        assert_eq!(state.store.added_agents().unwrap(), None);
        assert!(
            events.try_recv().is_err(),
            "nothing changed, so nothing is sent"
        );
    }

    /// The fill follows a completed snapshot, and only that: it is the snapshot landing that
    /// settles availability.
    #[test]
    fn a_completed_snapshot_fills_the_added_agents() {
        let (state, _dir) = crate::test_support::app_state("availability-snapshot-fills");
        seed_account(&state, Agent::Grok);
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), String::new());
        env.insert("HOME".to_string(), "/snapshot-home".to_string());

        apply_snapshot(&state, Err(anyhow::anyhow!("timed out")));
        assert_eq!(state.store.added_agents().unwrap(), None);

        apply_snapshot(&state, Ok(env));
        assert_eq!(state.store.added_agents().unwrap(), Some(vec![Agent::Grok]));
    }
}
