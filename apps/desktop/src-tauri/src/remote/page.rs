//! What the Remote devices page of the main window is asked to show: a question (a password, a
//! host key to confirm) or a failure of a connection made away from the page, and the request to
//! show the page itself. A connection made quietly from a menu is the shell's; when it ends in a
//! question or a failure, the outcome is queued here, the main window is brought forward, and its
//! page takes the queue once it listens (`remote_connect_pending_take`). So does a menu's Remote
//! Devices item (`remote_page_requested_take`). What is asked is read by a command, and not only
//! an event with a payload, because the page may not be listening yet at the moment of the
//! request: the window may be starting, or its page loading. A connection the page itself made,
//! whose page was loaded again before it ended, has its question or failure handed over again
//! (`should_hand_back`), since the page that asked is gone.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use super::outcome::ConnectOutcome;
use super::RemoteState;
use crate::daemon_window::is_main;
use crate::MAIN_WINDOW_LABEL;

/// The event the page is told of a queued outcome with; it takes them with
/// `remote_connect_pending_take`.
pub const PENDING_EVENT: &str = "remote-connect-pending";

/// The event the page is told of a request to show it with; it takes the request with
/// `remote_page_requested_take`.
pub const REQUESTED_EVENT: &str = "remote-page-requested";

/// An outcome for the saved remote daemon `id`, for the Remote devices page to show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pending {
    pub id: String,
    pub outcome: ConnectOutcome,
}

#[derive(Default)]
pub struct PageState {
    pending: Mutex<Vec<Pending>>,
    /// Whether the page has been asked for and has not taken the request yet.
    requested: AtomicBool,
    /// How many times the main window's page has started loading, which tells a connection its
    /// page asked for that the page that asked is gone.
    loads: AtomicU64,
}

impl PageState {
    fn queue(&self) -> std::sync::MutexGuard<'_, Vec<Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queues `outcome` for the entry `id`, in place of one already queued for it.
    fn push(&self, id: String, outcome: ConnectOutcome) {
        let mut queue = self.queue();
        queue.retain(|pending| pending.id != id);
        queue.push(Pending { id, outcome });
    }

    /// Records that the page was asked for.
    fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }

    /// What is queued, which it empties.
    fn take(&self) -> Vec<Pending> {
        std::mem::take(&mut *self.queue())
    }

    /// What is queued, which it empties, for the window labelled `label`: nothing for any window
    /// but the main one.
    pub fn take_pending_for(&self, label: &str) -> Vec<Pending> {
        if !is_main(label) {
            return Vec::new();
        }
        self.take()
    }

    /// Whether the page was asked for, which it settles, for the window labelled `label`: false for
    /// any window but the main one.
    pub fn take_requested_for(&self, label: &str) -> bool {
        is_main(label) && self.requested.swap(false, Ordering::SeqCst)
    }
}

/// Whether a connection made from the main window's page, which started when the page had been
/// loaded `loads_before` times and ended when it had been loaded `loads_after`, ended with an
/// `outcome` to hand to the page again: the page that asked is gone, and the outcome has to be
/// shown.
pub fn should_hand_back(loads_before: u64, loads_after: u64, outcome: &ConnectOutcome) -> bool {
    loads_before != loads_after && needs_showing(outcome)
}

/// Whether `outcome` is a question or a failure, which only the Remote devices page shows: not a
/// daemon shown in its window or the browser.
pub fn needs_showing(outcome: &ConnectOutcome) -> bool {
    !matches!(
        outcome,
        ConnectOutcome::Opened | ConnectOutcome::OpenedInBrowser
    )
}

/// Has the main window brought forward (on the main thread, as `show_main_window` queues it) and
/// tells its page `event`. A page still loading misses the event and takes what was asked for
/// once it listens.
fn show_and_tell(app: &AppHandle, event: &'static str) {
    super::actions::show_main_window(app);
    let _ = app.emit_to(MAIN_WINDOW_LABEL, event, ());
}

/// Queues `outcome` for the entry `id`, brings the main window forward, and tells its page.
pub fn hand_over(app: &AppHandle, id: String, outcome: ConnectOutcome) {
    app.state::<RemoteState>().page.push(id, outcome);
    show_and_tell(app, PENDING_EVENT);
}

/// Asks for the Remote devices page, in the main window, which is brought forward; the page takes
/// the request once it listens (`remote_page_requested_take`).
pub fn request_page(app: &AppHandle) {
    app.state::<RemoteState>().page.request();
    show_and_tell(app, REQUESTED_EVENT);
}

/// Whether the page was asked for, which it settles; false for any window but the main one.
#[tauri::command]
pub fn remote_page_requested_take(app: AppHandle, webview_window: WebviewWindow) -> bool {
    app.state::<RemoteState>()
        .page
        .take_requested_for(webview_window.label())
}

/// What is queued, which it empties; nothing for any window but the main one.
#[tauri::command]
pub fn remote_connect_pending_take(app: AppHandle, webview_window: WebviewWindow) -> Vec<Pending> {
    app.state::<RemoteState>()
        .page
        .take_pending_for(webview_window.label())
}

/// The main window's page started loading: what the page before it had under way is gone with it.
/// What is queued stays, for the new page to take.
pub fn page_load_started(app: &AppHandle) {
    app.state::<RemoteState>()
        .page
        .loads
        .fetch_add(1, Ordering::SeqCst);
}

/// How many times the main window's page has started loading.
pub fn loads(app: &AppHandle) -> u64 {
    app.state::<RemoteState>().page.loads.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(detail: &str) -> ConnectOutcome {
        ConnectOutcome::Failed {
            detail: detail.into(),
        }
    }

    #[test]
    fn only_a_question_or_a_failure_is_left_for_the_page() {
        use super::super::outcome::{Action, SecretKindName};
        // A daemon shown in its window or the browser is nothing to show.
        for outcome in [ConnectOutcome::Opened, ConnectOutcome::OpenedInBrowser] {
            assert!(!needs_showing(&outcome), "{outcome:?}");
        }
        // Every question and every failure is.
        for outcome in [
            ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Password,
            },
            ConnectOutcome::HostKeyUnknown {
                fingerprint: "SHA256:x".into(),
            },
            ConnectOutcome::HostKeyMismatch {
                expected: "a".into(),
                actual: "b".into(),
            },
            ConnectOutcome::Unreachable { detail: "x".into() },
            ConnectOutcome::AuthFailed { detail: "x".into() },
            ConnectOutcome::DeviceKeyRefused,
            ConnectOutcome::KeyUnreadable { detail: "x".into() },
            ConnectOutcome::OctoboarddNotFound { tried: vec![] },
            ConnectOutcome::NoDaemon,
            ConnectOutcome::ApplicationNotRunning { detail: "x".into() },
            ConnectOutcome::NoOwnerEndpoint { detail: "x".into() },
            ConnectOutcome::EndpointUnreachable { detail: "x".into() },
            ConnectOutcome::GenerationMismatch {
                remote_generation: 1,
                local_generation: 2,
                remote_version: "1".into(),
                local_version: "2".into(),
            },
            ConnectOutcome::ActionFailed {
                action: Action::Start,
                detail: "x".into(),
            },
            failed("x"),
        ] {
            assert!(needs_showing(&outcome), "{outcome:?}");
        }
    }

    #[test]
    fn a_second_outcome_for_an_entry_replaces_the_first_and_taking_empties_the_queue() {
        let state = PageState::default();
        state.push("a".into(), failed("one"));
        state.push("b".into(), failed("two"));
        state.push("a".into(), failed("three"));
        let mut taken = state.take();
        // Which is shown first is not the queue's to promise; each entry has its latest alone.
        taken.sort_by(|left, right| left.id.cmp(&right.id));
        assert_eq!(
            taken,
            [
                Pending {
                    id: "a".into(),
                    outcome: failed("three")
                },
                Pending {
                    id: "b".into(),
                    outcome: failed("two")
                }
            ]
        );
        assert!(state.take().is_empty());
    }

    #[test]
    fn what_is_queued_after_a_take_is_taken_by_the_next_one_alone() {
        let state = PageState::default();
        state.push("a".into(), failed("one"));
        assert_eq!(state.take().len(), 1);
        state.push("b".into(), failed("two"));
        assert_eq!(
            state.take(),
            [Pending {
                id: "b".into(),
                outcome: failed("two")
            }]
        );
    }

    #[test]
    fn a_pending_outcome_reaches_the_page_as_its_entry_and_the_outcome() {
        let pending = Pending {
            id: "a".into(),
            outcome: ConnectOutcome::SecretNeeded {
                kind: super::super::outcome::SecretKindName::Password,
            },
        };
        assert_eq!(
            serde_json::to_value(pending).unwrap(),
            serde_json::json!({
                "id": "a",
                "outcome": { "outcome": "secret_needed", "kind": "password" }
            })
        );
    }

    #[test]
    fn only_the_main_window_takes_what_is_queued_and_a_take_empties_it() {
        let state = PageState::default();
        state.push("a".into(), failed("one"));
        // A window that is not the main one takes nothing from it.
        assert!(state.take_pending_for("daemon-1").is_empty());
        assert!(state.take_pending_for("flyout").is_empty());
        assert_eq!(
            state.take_pending_for(MAIN_WINDOW_LABEL),
            [Pending {
                id: "a".into(),
                outcome: failed("one")
            }]
        );
        assert!(state.take_pending_for(MAIN_WINDOW_LABEL).is_empty());
    }

    #[test]
    fn only_the_main_window_takes_the_request_for_the_page_and_it_is_taken_once() {
        let state = PageState::default();
        assert!(!state.take_requested_for(MAIN_WINDOW_LABEL));
        state.request();
        assert!(!state.take_requested_for("daemon-1"));
        assert!(!state.take_requested_for("flyout"));
        assert!(state.take_requested_for(MAIN_WINDOW_LABEL));
        assert!(!state.take_requested_for(MAIN_WINDOW_LABEL));
    }

    #[test]
    fn an_outcome_is_handed_back_only_when_the_page_started_loading_again_and_it_needs_showing() {
        assert!(should_hand_back(0, 1, &failed("x")));
        assert!(should_hand_back(
            3,
            5,
            &ConnectOutcome::SecretNeeded {
                kind: super::super::outcome::SecretKindName::Password
            }
        ));
        // The page that asked is still there.
        assert!(!should_hand_back(2, 2, &failed("x")));
        // Nothing to show: the daemon's window or the browser is open.
        assert!(!should_hand_back(0, 1, &ConnectOutcome::Opened));
        assert!(!should_hand_back(0, 1, &ConnectOutcome::OpenedInBrowser));
        assert!(!should_hand_back(1, 1, &ConnectOutcome::Opened));
    }
}
