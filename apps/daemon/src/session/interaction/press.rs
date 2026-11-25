//! Pressing the user's answer to a session's published prompt into its agent's terminal, key by
//! key. One of the places that type keys of their own into a session (with `crate::session::trust`
//! and `crate::session::interrupt`), held to the same rules: each key is chosen from the screen as
//! it stands ([`super::InteractionReader::next_key`]), written alone and only while, under the
//! input lock, the screen still calls for that key and nothing else has written into the session
//! since the prompt was published, and each is waited on, for a bounded time, to change the screen
//! before the next is chosen. Keys a provider chooses together, where the screen between them
//! tells nothing ([`super::NextKey::PressAll`]), are written as one write under the same rules and
//! waited on as one key. The only text written is the user's own answer, as a bracketed paste
//! the provider puts in a field the prompt opened for it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::{check_answer, Answer, AnswerFields, CallSeen, InteractionReader, Misfit, NextKey};
use super::{screen_of, Published, Reading};
use crate::protocol::{error_code, notice_code, AgentId, CodedError, Notice, ProviderOperation};
use crate::session::wait::wait_for;
use crate::session::LiveSession;
use crate::state::AppState;

/// The most keys one answer may take: past it the provider is taken to be going round in circles.
/// The longest measured answer (several questions, each with a written answer) takes about twenty.
const MOST_KEYS: usize = 64;

/// How long the agent may take to answer one key, and the screen to settle before the next: the
/// agent's own pace sets it, as for the trust confirmation's keys
/// (`crate::session::trust::press::RESPONSE_TIMEOUT`, measured on the slowest host). Each wait
/// ends as soon as what it waits for is seen.
#[cfg_attr(test, allow(dead_code))]
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a reading has to hold to count as the screen settled.
const SETTLE: Duration = Duration::from_millis(150);

/// How many times in a row a look, [`SETTLE`] after the one before, has to agree with it on the
/// reading and the next key to count as the screen settled where it does not stand still beside the
/// prompt.
const AGREEING: u32 = 3;

fn response_timeout() -> Duration {
    #[cfg(test)]
    {
        RESPONSE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    RESPONSE_TIMEOUT
}

#[cfg(test)]
thread_local! {
    pub(super) static RESPONSE_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// Why a press stopped.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Stopped {
    /// The prompt is not the one answered any more, or someone else has it in hand.
    NotWaiting,
    /// The keys could not be pressed, or did not take.
    Failed(String),
}

/// Presses the answer `fields` give to the prompt published for `live` as `id`. Blocks while it
/// presses and watches the screen respond. On success the prompt has been seen gone and the
/// session's hand is down; a failure the user is to hear of is also broadcast as a notice.
pub fn answer(
    state: &Arc<AppState>,
    live: &Arc<LiveSession>,
    id: &str,
    fields: AnswerFields,
) -> Result<()> {
    let agent = live.agent;
    let kinds = agent.descriptor().capabilities.interactions;
    let reader = CodedError::require_offered(
        agent,
        ProviderOperation::Interactions,
        crate::adapter::provider(agent)
            .interactions()
            .filter(|_| kinds.permission || kinds.question),
    )?;
    let Some(published) = live
        .interaction
        .published()
        .filter(|published| published.interaction.id == id)
    else {
        return Err(not_waiting(&live.id));
    };
    let answer = check_answer(&published.interaction.prompt, fields).map_err(misfit)?;
    let Some(_claim) = live.interaction.claim_press() else {
        return Err(not_waiting(&live.id));
    };
    // Nothing of Octoboard's goes in between the keys.
    let _pause = state.pause_outbox(&live.id);
    match press(live, reader, &published, &answer, response_timeout()) {
        Ok(()) => {
            // Whichever of this press and the watch withdraws it lowers the hand, once.
            if live.interaction.withdraw(id) {
                state.publish_session_by_id(&live.id);
                state.interaction_left(&live.id, published.call.id.as_deref());
            }
            Ok(())
        }
        Err(Stopped::NotWaiting) => Err(not_waiting(&live.id)),
        Err(Stopped::Failed(reason)) => {
            tracing::warn!(session = %live.id, %reason, "pressing the answer to a prompt failed");
            let message = answer_failed_message(agent, &reason);
            let pairs = [("agent", agent.label()), ("detail", reason.as_str())];
            state.broadcast(
                Notice::new(
                    notice_code::INTERACTION_ANSWER_FAILED,
                    message.clone(),
                    &pairs,
                )
                .about(&live.id),
            );
            Err(CodedError::raised(
                error_code::INTERACTION_ANSWER_FAILED,
                message,
                &pairs,
            ))
        }
    }
}

/// The keys of `answer`, each given `patience` to take.
pub(super) fn press(
    live: &LiveSession,
    reader: &dyn InteractionReader,
    published: &Published,
    answer: &Answer,
    patience: Duration,
) -> Result<(), Stopped> {
    let mut expected = published.writes;
    if live.input_writes() != expected {
        return Err(Stopped::NotWaiting);
    }
    let call = &published.call;
    let mut pressed = 0;
    loop {
        let key_of = |screen: &str| reader.next_key(screen, call, answer);
        let Some((reading, screen)) = settled(live, reader, &key_of, patience) else {
            return Err(Stopped::Failed("the screen did not settle".to_string()));
        };
        match &reading {
            Reading::Gone if pressed > 0 => return Ok(()),
            Reading::Gone => return Err(Stopped::NotWaiting),
            Reading::Unknown if pressed == 0 => return Err(Stopped::NotWaiting),
            Reading::Unknown => {
                return Err(Stopped::Failed(
                    "the prompt was no longer recognised on the screen".to_string(),
                ))
            }
            // Another call's prompt: before any key, the one answered is not up; after a key, it
            // has been answered and the agent has gone on to the next, which the keys chosen for
            // this one must never reach.
            Reading::Shown { call: index, .. } if !same_call(live, *index, call) => {
                return if pressed > 0 {
                    Ok(())
                } else {
                    Err(Stopped::NotWaiting)
                };
            }
            _ => {}
        }
        let chosen = reader.next_key(&screen, call, answer);
        let keys = match &chosen {
            NextKey::Press(key) => std::slice::from_ref(key),
            NextKey::PressAll(keys) if !keys.is_empty() => keys.as_slice(),
            NextKey::PressAll(_) => {
                return Err(Stopped::Failed("the provider chose no key".to_string()))
            }
            NextKey::Stuck(reason) => return Err(Stopped::Failed(reason.to_string())),
        };
        // Keys chosen together count one each, and are not written at all when they would take
        // the answer past the most it may take.
        if pressed + keys.len() > MOST_KEYS {
            return Err(Stopped::Failed(format!(
                "the prompt was still up after {pressed} keys, and the next would pass {MOST_KEYS}"
            )));
        }
        let still = |now: &str| {
            reader.next_key(now, call, answer) == chosen
                && match reader.read(now, &live.interaction.calls()) {
                    Reading::Gone | Reading::Unknown => false,
                    Reading::Shown { call: index, .. } => same_call(live, index, call),
                    Reading::Up => true,
                }
        };
        let gate = || still(&screen_of(live, reader));
        match live.write_keys_if(expected, gate, keys) {
            Ok(true) => {
                expected += 1;
                pressed += keys.len();
            }
            // Someone typed into the terminal: the prompt is theirs.
            Ok(false) if live.input_writes() != expected => return Err(Stopped::NotWaiting),
            Ok(false) => {
                return Err(Stopped::Failed(
                    "the screen changed before the key was sent".to_string(),
                ))
            }
            Err(err) => {
                return Err(Stopped::Failed(format!(
                    "writing to the terminal failed: {err}"
                )))
            }
        }
        if !wait_for(patience, || screen_of(live, reader) != screen) {
            return Err(Stopped::Failed(
                "the prompt did not respond to a key".to_string(),
            ));
        }
    }
}

/// The screen once two looks [`SETTLE`] apart agree on its reading and on the key `key_of` would
/// press on it, and that reading, or `None` when they have not within `patience`. What they say is
/// compared, not the screens: once a prompt is answered the agent animates what it is doing, which
/// would keep a screen from ever reading the same twice, while the prompt's own rows, which the
/// reading and the key come from, stay still for as long as it is up.
fn settled(
    live: &LiveSession,
    reader: &dyn InteractionReader,
    key_of: &dyn Fn(&str) -> NextKey,
    patience: Duration,
) -> Option<(Reading, String)> {
    let read = || {
        let screen = screen_of(live, reader);
        let reading = reader.read(&screen, &live.interaction.calls());
        let key = key_of(&screen);
        (reading, screen, key)
    };
    let mut last = read();
    let mut agreeing = 0;
    let mut found = None;
    wait_for(patience, || {
        std::thread::sleep(SETTLE);
        let now = read();
        agreeing = if now.0 == last.0 && now.2 == last.2 {
            agreeing + 1
        } else {
            0
        };
        // While the prompt is up its rows stand still, so two looks off frames half drawn, which
        // could agree while neither is whole, are told apart by the screen itself standing still;
        // where something beside the prompt keeps changing, looks that agree [`AGREEING`] times in
        // a row are taken instead.
        let still = now.0 == Reading::Gone || now.1 == last.1 || agreeing >= AGREEING;
        if agreeing > 0 && still {
            found = Some((now.0, now.1));
            true
        } else {
            last = now;
            false
        }
    });
    found
}

/// Whether `calls()[index]` of `live` is `call`.
fn same_call(live: &LiveSession, index: usize, call: &CallSeen) -> bool {
    live.interaction.calls().get(index) == Some(call)
}

/// The refusal of an answer to a prompt that is not up.
pub(crate) fn not_waiting(session: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::INTERACTION_NOT_WAITING,
        "this prompt is no longer waiting for an answer",
        &[("session", session)],
    )
}

fn answer_failed_message(agent: AgentId, reason: &str) -> String {
    format!(
        "{} could not press the answer into {} ({reason}). Answer it in the terminal.",
        crate::APP_NAME,
        agent.label()
    )
}

/// The refusal of an answer that does not fit its prompt.
fn misfit(misfit: Misfit) -> anyhow::Error {
    let code = if misfit.missing {
        error_code::FIELD_REQUIRED
    } else {
        error_code::UNREADABLE_REQUEST
    };
    CodedError::raised(code, misfit.reason, &[("field", misfit.field)])
}
