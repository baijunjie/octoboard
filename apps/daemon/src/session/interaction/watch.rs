//! The watch of a session's screen for the prompt its agent shows while the session waits on the
//! user: it publishes what the provider reads ([`super::InteractionReader::read`]), withdraws it
//! when it can no longer be answered, and, once a published prompt is seen gone, has the session's
//! hand come down (`AppState::interaction_left`).

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::{within_limits, InteractionReader, Published, Reading};
use crate::protocol::{InteractionPrompt, PendingInteraction, SessionStatus};
use crate::session::LiveSession;
use crate::state::AppState;

/// How often the screen is read while the watch runs.
pub const POLL: Duration = Duration::from_millis(250);

/// How many readings in a row a prompt has to be seen gone, or not recognised at all, before it is
/// taken to be: one reading can fall on a frame half drawn.
const SEEN_TWICE: u32 = 2;

/// Starts the watch of `live`'s screen unless one runs, for a provider that reads prompts.
pub fn ensure(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let Some(reader) = crate::adapter::provider(live.agent).interactions() else {
        return;
    };
    if live.interaction.watching.swap(true, Ordering::AcqRel) {
        return;
    }
    let (state, live) = (state.clone(), live.clone());
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        live.interaction.watching.store(false, Ordering::Release);
        return;
    };
    runtime.spawn(async move {
        let mut watch = Watch::default();
        loop {
            tokio::time::sleep(POLL).await;
            if watch.look(&state, &live, reader) {
                continue;
            }
            live.interaction.watching.store(false, Ordering::Release);
            // A reason to watch that came in as the flag went down would find it still up and
            // start nothing: look once more, and go on if there is one.
            if !watch.look(&state, &live, reader)
                || live.interaction.watching.swap(true, Ordering::AcqRel)
            {
                break;
            }
        }
    });
}

/// What the watch has seen over its readings.
#[derive(Default)]
pub(super) struct Watch {
    gone: u32,
    unknown: u32,
}

impl Watch {
    /// Reads the screen once and acts on it; says whether to go on watching.
    pub(super) fn look(
        &mut self,
        state: &Arc<AppState>,
        live: &Arc<LiveSession>,
        reader: &dyn InteractionReader,
    ) -> bool {
        // A later process of the session has its own watch.
        let current = state
            .live_session(&live.id)
            .is_some_and(|now| Arc::ptr_eq(&now, live));
        let session = state.session_record(&live.id).ok();
        let (Some(session), true) = (session, current) else {
            withdraw(state, live);
            return false;
        };
        let waiting = state.shown_status(&session) == SessionStatus::WaitingUser;
        let calls = live.interaction.calls();
        let reading = reader.read(&super::screen_of(live, reader), &calls);
        self.gone = if reading == Reading::Gone {
            self.gone + 1
        } else {
            0
        };
        self.unknown = if reading == Reading::Unknown {
            self.unknown + 1
        } else {
            0
        };
        let published = live.interaction.published();
        match reading {
            Reading::Shown { call, prompt } if waiting && declared(live, &prompt) => {
                let Some(call) = calls.get(call) else {
                    return true;
                };
                // Compared as published, its strings cut.
                let candidate = within_limits(PendingInteraction {
                    id: String::new(),
                    call_id: call.id.clone(),
                    prompt,
                });
                // Something typed into the terminal since it was published leaves its answer to
                // the person typing: a prompt still up and unchanged after that is published
                // again under a new id, which an answer from then on names.
                let same = published.as_ref().is_some_and(|published| {
                    published.call == *call
                        && (live.interaction.pressing() || published.writes == live.input_writes())
                        && candidate.as_ref().is_some_and(|candidate| {
                            published.interaction.prompt == candidate.prompt
                        })
                });
                if !same {
                    let published = candidate.map(|mut interaction| {
                        interaction.id = uuid::Uuid::new_v4().to_string();
                        Published {
                            interaction,
                            call: call.clone(),
                            writes: live.input_writes(),
                        }
                    });
                    if live.interaction.publish(published) {
                        state.publish_session_by_id(&live.id);
                    }
                }
            }
            // A prompt still up, being answered or in a state it cannot be answered from as it
            // stands, keeps what was published while the session waits.
            Reading::Up if waiting => {}
            Reading::Gone if self.gone >= SEEN_TWICE => {
                // Whichever of this watch and a press withdraws it lowers the hand, once.
                if let Some(published) = published {
                    if live.interaction.withdraw(&published.interaction.id) {
                        state.publish_session_by_id(&live.id);
                        state.interaction_left(&live.id, published.call.id.as_deref());
                    }
                }
            }
            Reading::Gone => {}
            Reading::Unknown if self.unknown < SEEN_TWICE || live.interaction.pressing() => {}
            _ => withdraw(state, live),
        }
        // A wait with no call of the turn to match a prompt to can publish nothing (a session
        // request's hand, a usage-limit dialog, a prompt before any call was reported); the next
        // hook that raises the hand starts the watch again.
        (waiting && !calls.is_empty())
            || live.interaction.published().is_some()
            || live.interaction.pressing()
    }
}

/// Whether the provider of `live` declares prompts of `prompt`'s kind.
fn declared(live: &LiveSession, prompt: &InteractionPrompt) -> bool {
    let declared = live.agent.descriptor().capabilities.interactions;
    match prompt {
        InteractionPrompt::Permission(_) => declared.permission,
        InteractionPrompt::Question(_) => declared.question,
    }
}

/// Withdraws whatever is published for `live`, and tells the clients if anything was.
fn withdraw(state: &AppState, live: &LiveSession) {
    if live.interaction.publish(None) {
        state.publish_session_by_id(&live.id);
    }
}
