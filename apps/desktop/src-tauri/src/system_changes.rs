//! The clean-up before the application is uninstalled, as the application's local window does it:
//! what the shell's features have in effect on this computer and on remote servers outside the
//! application's own directories, with what this computer's own daemon's features have in effect
//! ([`daemon`]), and clearing all of it at once. The UI's Settings lists it and offers the one
//! action, in the local daemon's window alone. The daemon's part is asked of the application's own
//! daemon on its local routes, never through a window's connection: the clean-up is done on the
//! machine itself. The Windows uninstaller has the application do the same without a window, before
//! it removes anything ([`headless`]).
//!
//! Every write of the shell's outside its own directories that is a registered exception (P9 in
//! `docs/conventions/principles.md`) and has something to undo is an [`Exception`] in
//! [`REGISTERED`], with its two answers: what of it is in effect now, and clear all of it. The
//! module of the feature holds both, and clearing goes through the one function the feature's own
//! switch goes through. Nothing here clears a single item: one is undone at its feature's switch.
//!
//! The Windows taskbar's jump list is not an item: it is part of the application's own presence,
//! written whenever the menus are built, with no switch. Clearing removes it all the same
//! (`menu::delete_jump_list`, which does its work where the menus do theirs), so that none stays once
//! the application is gone.

pub mod daemon;
pub mod headless;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use serde::Serialize;
use tauri::AppHandle;

use crate::command_line;
use crate::diagnostics::diagnostic;
use crate::launch_at_login;
use crate::remote::{self, KeyRemoval};

/// One modification a feature of the shell, or of this computer's own daemon, has in effect outside
/// the application's own directories, which turning that feature off undoes.
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    /// This device's key for the saved remote daemon `id`. With `on_server`, it is installed: a
    /// line in the authorized keys of `user` on `host`, and its private half in the system's
    /// credential store. Without, only a private half is left in the credential store, from a key
    /// turned off earlier whose private half the store did not remove then.
    DeviceKey {
        id: String,
        name: String,
        host: String,
        user: String,
        on_server: bool,
    },
    /// The system holds the application's login entry. With `turned_off`, the user turned it off in
    /// the system's own list, where it stays: the switch shows it off, and the entry is still there.
    LaunchAtLogin { turned_off: bool },
    /// The command-line tool's link is at `path`.
    CommandLine { path: String },
    /// The daemon's: the account `account` (its id) of `agent` shares that agent's default
    /// configuration, or its last change of sharing is incomplete, entries of its directory being
    /// links into the default account's directory.
    AccountSharing {
        account: String,
        name: String,
        agent: String,
    },
}

/// Why the daemon did not clear one of its items: its refusal as its `error` reply carries it,
/// for the UI to word by the code.
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct DaemonFailure {
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub message: String,
}

/// What clearing one modification came to.
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
#[serde(
    tag = "result",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Outcome {
    /// Nothing of it is left.
    Cleared,
    /// A device key of which a part stays: its line on the host, its private key in the credential
    /// store, or both, as `removal` names them.
    DeviceKeyLeft { removal: KeyRemoval },
    /// The login entry is still there; `detail` is the system's own text.
    LaunchAtLoginLeft { detail: String },
    /// The command-line link is still there, as `error` says (`command_line::Error`).
    CommandLineLeft { error: command_line::Error },
    /// The account still shares, as the daemon's refusal `failure` says (an account in use, say).
    AccountSharingLeft { failure: DaemonFailure },
}

/// A modification and what clearing it came to.
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct Cleared {
    pub change: Change,
    pub outcome: Outcome,
}

/// One of the shell's features that could not tell what it has in effect, or the daemon's part as a
/// whole (`daemon::KIND`), which could not be asked or read: nothing of it is listed, and
/// after a clearing what became of its modifications is not known (some may have been cleared).
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct Failed {
    /// Which feature, by the `kind` of its modifications; `daemon` for the daemon's part.
    pub kind: &'static str,
    /// The system's or the shell's own text.
    pub detail: String,
}

/// What is in effect: the items of every feature that could tell, and the features that could not.
#[derive(Serialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct Listed {
    pub changes: Vec<Change>,
    pub failed: Vec<Failed>,
}

/// What clearing everything came to: each item that was in effect or could not be cleared, and the
/// features that failed as a whole, whose items `results` therefore leaves out.
#[derive(Serialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct ClearedAll {
    pub results: Vec<Cleared>,
    pub failed: Vec<Failed>,
}

/// What an exception answers, once it has: its items, or why it could not tell what it has.
pub type Answer<'a, T> = Pin<Box<dyn Future<Output = Result<Vec<T>, String>> + Send + 'a>>;

/// A registered exception of the shell's that has something to undo. `C` is what it reaches the
/// application through: the application's handle, but for the tests of this module.
pub trait Exception<C = AppHandle>: Send + Sync {
    /// The `kind` of its modifications, which also names it in a failure and a diagnostic; the same
    /// word the daemon's registration uses for its own (`coordinator::system_changes` there).
    fn kind(&self) -> &'static str;
    /// What of it is in effect now, one item per thing its feature's switch would undo.
    fn in_effect<'a>(&'a self, app: &'a C) -> Answer<'a, Change>;
    /// Clears all of it and turns its feature off, as the feature's own switch does, one result per
    /// item that was in effect or could not be cleared: an item that cannot be cleared is answered
    /// with what stays and does not stop the others.
    fn clear_all<'a>(&'a self, app: &'a C) -> Answer<'a, Cleared>;
}

/// Every registered exception of the shell's that has something to undo, in the order the clean-up
/// lists them.
pub static REGISTERED: &[&dyn Exception] = &[
    &remote::DeviceKeys,
    &launch_at_login::LoginEntry,
    &command_line::Link,
];

/// What the shell's features, and this computer's own daemon's, have in effect outside the
/// application's own directories, and the features that could not tell.
#[tauri::command]
pub async fn system_changes_list(app: AppHandle) -> Listed {
    list_with(REGISTERED, &app, &daemon::Local(&app)).await
}

/// Clears everything `system_changes_list` names and turns the features behind it off, and removes
/// the taskbar's jump list: for the moment before the application is uninstalled, and asked for by
/// the user alone. Answers what became of each item, and the features that could not tell what
/// they have. May wait for the system's administrator prompt, and for remote servers to answer.
#[tauri::command]
pub async fn system_changes_clear(app: AppHandle) -> ClearedAll {
    let cleared = clear_with(REGISTERED, &app, &daemon::Local(&app)).await;
    remove_jump_list(&app);
    cleared
}

/// Removes the taskbar's jump list for the clean-up. Called from a worker thread: the removal does
/// its work on the main thread itself and waits for it.
fn remove_jump_list(app: &AppHandle) {
    if let Err(err) = crate::menu::delete_jump_list(app) {
        // Not an item the user was shown, so only logged. The uninstaller removes the list itself only where its
        // clean-up did not answer 0 or 1 (`OCTOBOARD_REMOVE_FEATURE_ENTRIES` in `windows/installer-hooks.nsh`);
        // where it did, a list this could not remove stays.
        diagnostic!("removing the jump list for the clean-up failed: {err}");
    }
}

/// [`list_of`] the shell's exceptions, then the daemon's items, in the one list; the daemon's part
/// failing is answered beside the shell's own.
async fn list_with<C: Sync>(
    exceptions: &[&dyn Exception<C>],
    app: &C,
    own_daemon: &dyn daemon::Daemon,
) -> Listed {
    let mut all = list_of(exceptions, app).await;
    let (changes, failed) = daemon::list(own_daemon).await;
    all.changes.extend(changes);
    all.failed.extend(failed);
    all
}

/// [`clear_of`] the shell's exceptions, then the daemon's items, in the one result: neither stops
/// the other, and the daemon's part failing is answered beside the shell's own results.
async fn clear_with<C: Sync>(
    exceptions: &[&dyn Exception<C>],
    app: &C,
    own_daemon: &dyn daemon::Daemon,
) -> ClearedAll {
    let mut all = clear_of(exceptions, app).await;
    let (results, failed) = daemon::clear(own_daemon).await;
    for failure in &failed {
        diagnostic!(
            "clearing the daemon's part for the clean-up failed: {}",
            failure.detail
        );
    }
    all.results.extend(results);
    all.failed.extend(failed);
    all
}

/// What every exception has in effect, in their order; one that cannot tell is answered beside the
/// items of the others.
async fn list_of<C: Sync>(exceptions: &[&dyn Exception<C>], app: &C) -> Listed {
    let mut all = Listed::default();
    for exception in exceptions {
        match exception.in_effect(app).await {
            Ok(changes) => all.changes.extend(changes),
            Err(detail) => all.failed.push(Failed {
                kind: exception.kind(),
                detail,
            }),
        }
    }
    all
}

/// Clears every exception, each after the other, whatever became of the ones before it. An
/// exception that could not tell what it has is answered beside the items of the others.
async fn clear_of<C: Sync>(exceptions: &[&dyn Exception<C>], app: &C) -> ClearedAll {
    let mut all = ClearedAll::default();
    for exception in exceptions {
        match exception.clear_all(app).await {
            Ok(cleared) => all.results.extend(cleared),
            Err(detail) => {
                let kind = exception.kind();
                diagnostic!("clearing {kind} for the clean-up failed: {detail}");
                all.failed.push(Failed { kind, detail });
            }
        }
    }
    all
}

/// Runs `work` on the main thread and answers what it returned, for the system calls the shell's
/// own commands make there.
pub async fn on_main_thread<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (answer, answered) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = answer.send(work());
    })
    .map_err(|err| err.to_string())?;
    answered.await.map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// An exception that answers what it was given, counting its clearings.
    pub(super) struct StandIn {
        changes: Result<Vec<Change>, String>,
        cleared: Result<Vec<Cleared>, String>,
        clearings: AtomicUsize,
    }

    impl Exception<()> for StandIn {
        fn kind(&self) -> &'static str {
            "stand_in"
        }

        fn in_effect<'a>(&'a self, _: &'a ()) -> Answer<'a, Change> {
            Box::pin(async move { self.changes.clone() })
        }

        fn clear_all<'a>(&'a self, _: &'a ()) -> Answer<'a, Cleared> {
            Box::pin(async move {
                self.clearings.fetch_add(1, Ordering::SeqCst);
                self.cleared.clone()
            })
        }
    }

    pub(super) const LOGIN: Change = Change::LaunchAtLogin { turned_off: false };

    pub(super) fn link(path: &str) -> Change {
        Change::CommandLine { path: path.into() }
    }

    pub(super) fn stand_in(
        changes: Result<Vec<Change>, String>,
        cleared: Result<Vec<Cleared>, String>,
    ) -> StandIn {
        StandIn {
            changes,
            cleared,
            clearings: AtomicUsize::new(0),
        }
    }

    fn clearings(exceptions: &[&StandIn]) -> Vec<usize> {
        exceptions
            .iter()
            .map(|exception| exception.clearings.load(Ordering::SeqCst))
            .collect()
    }

    #[tokio::test]
    async fn the_list_holds_every_exception_s_items_in_the_order_they_are_registered() {
        let first = stand_in(Ok(vec![LOGIN]), Ok(vec![]));
        let none = stand_in(Ok(vec![]), Ok(vec![]));
        let last = stand_in(Ok(vec![link("/a"), link("/b")]), Ok(vec![]));
        let listed = list_of(&[&first, &none, &last], &()).await;
        assert_eq!(listed.changes, [LOGIN, link("/a"), link("/b")]);
        assert!(listed.failed.is_empty());
        assert_eq!(list_of::<()>(&[], &()).await, Listed::default());
        // Reading what is in effect clears nothing.
        assert_eq!(clearings(&[&first, &none, &last]), [0, 0, 0]);
    }

    #[tokio::test]
    async fn an_exception_that_cannot_tell_what_it_has_is_listed_beside_the_items_of_the_others() {
        let first = stand_in(Ok(vec![LOGIN]), Ok(vec![]));
        let broken = stand_in(Err("unreadable".into()), Ok(vec![]));
        let last = stand_in(Ok(vec![link("/a")]), Ok(vec![]));
        let listed = list_of(&[&first, &broken, &last], &()).await;
        assert_eq!(listed.changes, [LOGIN, link("/a")]);
        assert_eq!(
            listed.failed,
            [Failed {
                kind: "stand_in",
                detail: "unreadable".into()
            }]
        );
    }

    #[tokio::test]
    async fn an_item_that_stays_does_not_stop_the_ones_after_it() {
        let stays = Cleared {
            change: LOGIN,
            outcome: Outcome::LaunchAtLoginLeft {
                detail: "refused".into(),
            },
        };
        let gone = Cleared {
            change: link("/a"),
            outcome: Outcome::Cleared,
        };
        let first = stand_in(Ok(vec![]), Ok(vec![stays.clone()]));
        let second = stand_in(Ok(vec![]), Ok(vec![gone.clone()]));
        let all = clear_of(&[&first, &second], &()).await;
        assert_eq!(all.results, [stays, gone]);
        assert!(all.failed.is_empty());
        assert_eq!(clearings(&[&first, &second]), [1, 1]);
    }

    #[tokio::test]
    async fn an_exception_that_fails_as_a_whole_is_answered_beside_the_items_of_the_others() {
        let gone = Cleared {
            change: link("/a"),
            outcome: Outcome::Cleared,
        };
        let first = stand_in(Ok(vec![]), Ok(vec![gone.clone()]));
        let broken = stand_in(Ok(vec![]), Err("unreadable".into()));
        let last = stand_in(Ok(vec![]), Ok(vec![gone.clone()]));
        let all = clear_of(&[&first, &broken, &last], &()).await;
        assert_eq!(clearings(&[&first, &broken, &last]), [1, 1, 1]);
        assert_eq!(all.results, [gone.clone(), gone]);
        assert_eq!(
            all.failed,
            [Failed {
                kind: "stand_in",
                detail: "unreadable".into()
            }]
        );
        assert_eq!(
            serde_json::to_value(&all.failed).unwrap(),
            serde_json::json!([{ "kind": "stand_in", "detail": "unreadable" }])
        );
    }

    #[tokio::test]
    async fn the_daemons_items_are_listed_in_the_same_list_after_the_shells() {
        use super::daemon::tests::{sharing, StandIn as Daemon, SHARING};

        let keys = stand_in(Ok(vec![LOGIN]), Ok(vec![]));
        let links = stand_in(Ok(vec![link("/a")]), Ok(vec![]));
        let own_daemon = Daemon::answering(&format!(r#"{{"changes":[{SHARING}]}}"#));
        let listed = list_with(&[&keys, &links], &(), &own_daemon).await;
        // Grouped by kind, the sharing accounts last.
        assert_eq!(listed.changes, [LOGIN, link("/a"), sharing()]);
        assert!(listed.failed.is_empty());
        assert_eq!(own_daemon.routes(), ["/system-changes"]);
        assert_eq!(clearings(&[&keys, &links]), [0, 0]);
    }

    #[tokio::test]
    async fn the_daemons_part_failing_is_listed_beside_the_shells_own_items() {
        use super::daemon::tests::StandIn as Daemon;

        let shell = stand_in(Ok(vec![LOGIN]), Ok(vec![]));
        let broken = stand_in(Err("unreadable".into()), Ok(vec![]));
        let own_daemon = Daemon::failing("this computer's daemon is not running");
        let listed = list_with(&[&shell, &broken], &(), &own_daemon).await;
        assert_eq!(listed.changes, [LOGIN]);
        assert_eq!(
            listed.failed,
            [
                Failed {
                    kind: "stand_in",
                    detail: "unreadable".into()
                },
                Failed {
                    kind: "daemon",
                    detail: "this computer's daemon is not running".into()
                }
            ]
        );
    }

    #[tokio::test]
    async fn clearing_clears_the_shells_items_and_the_daemons_in_the_one_result() {
        use super::daemon::tests::{sharing, StandIn as Daemon, SHARING};

        let gone = Cleared {
            change: link("/a"),
            outcome: Outcome::Cleared,
        };
        let shell = stand_in(Ok(vec![]), Ok(vec![gone.clone()]));
        let body = format!(
            r#"{{"results":[{{"change":{SHARING}}},{{"change":{SHARING},"failure":{{"code":"account_in_use","params":{{}},"message":"in use"}}}}],"failed":[]}}"#
        );
        let own_daemon = Daemon::answering(&body);
        let all = clear_with(&[&shell], &(), &own_daemon).await;
        assert_eq!(clearings(&[&shell]), [1]);
        assert_eq!(own_daemon.routes(), ["/system-changes/clear"]);
        assert!(all.failed.is_empty(), "{:?}", all.failed);
        assert_eq!(
            all.results,
            [
                gone,
                Cleared {
                    change: sharing(),
                    outcome: Outcome::Cleared
                },
                Cleared {
                    change: sharing(),
                    outcome: Outcome::AccountSharingLeft {
                        failure: DaemonFailure {
                            code: "account_in_use".into(),
                            params: BTreeMap::new(),
                            message: "in use".into(),
                        }
                    }
                }
            ]
        );
    }

    #[tokio::test]
    async fn the_daemons_part_failing_does_not_stop_the_shells_clearing_and_is_answered_beside_it()
    {
        use super::daemon::tests::StandIn as Daemon;

        let gone = Cleared {
            change: link("/a"),
            outcome: Outcome::Cleared,
        };
        let shell = stand_in(Ok(vec![]), Ok(vec![gone.clone()]));
        let own_daemon = Daemon::failing("this computer's daemon is not running");
        let all = clear_with(&[&shell], &(), &own_daemon).await;
        assert_eq!(clearings(&[&shell]), [1]);
        assert_eq!(all.results, [gone]);
        assert_eq!(
            serde_json::to_value(&all.failed).unwrap(),
            serde_json::json!([{
                "kind": "daemon", "detail": "this computer's daemon is not running",
            }])
        );
    }

    #[test]
    fn the_daemons_items_and_their_refusal_are_worded_for_the_ui() {
        use super::daemon::tests::sharing;

        assert_eq!(
            serde_json::to_value(sharing()).unwrap(),
            serde_json::json!({
                "kind": "account_sharing", "account": "acct-1", "name": "Work",
                "agent": "claude",
            })
        );
        assert_eq!(
            serde_json::to_value(Cleared {
                change: sharing(),
                outcome: Outcome::AccountSharingLeft {
                    failure: DaemonFailure {
                        code: "account_in_use".into(),
                        params: BTreeMap::from([("name".to_owned(), "Work".to_owned())]),
                        message: "in use".into(),
                    },
                },
            })
            .unwrap()["outcome"],
            serde_json::json!({
                "result": "account_sharing_left",
                "failure": {
                    "code": "account_in_use", "params": { "name": "Work" }, "message": "in use",
                },
            })
        );
    }

    /// The daemon's part is not one of the shell's registered exceptions: it is the daemon's own
    /// registration that names it.
    #[test]
    fn every_registered_exception_is_named_by_the_kind_of_its_items() {
        assert_eq!(
            REGISTERED
                .iter()
                .map(|exception| exception.kind())
                .collect::<Vec<_>>(),
            ["device_key", "launch_at_login", "command_line"]
        );
    }

    #[test]
    fn the_items_and_their_outcomes_are_worded_for_the_ui() {
        let key = Change::DeviceKey {
            id: "e".into(),
            name: "Build server".into(),
            host: "build.example".into(),
            user: "dev".into(),
            on_server: true,
        };
        assert_eq!(
            serde_json::to_value(&key).unwrap(),
            serde_json::json!({
                "kind": "device_key", "id": "e", "name": "Build server",
                "host": "build.example", "user": "dev", "onServer": true,
            })
        );
        assert_eq!(
            serde_json::to_value(Change::LaunchAtLogin { turned_off: true }).unwrap(),
            serde_json::json!({ "kind": "launch_at_login", "turnedOff": true })
        );
        assert_eq!(
            serde_json::to_value(Listed {
                changes: vec![LOGIN],
                failed: vec![Failed {
                    kind: "command_line",
                    detail: "unreadable".into()
                }],
            })
            .unwrap(),
            serde_json::json!({
                "changes": [{ "kind": "launch_at_login", "turnedOff": false }],
                "failed": [{ "kind": "command_line", "detail": "unreadable" }],
            })
        );
        assert_eq!(
            serde_json::to_value(Cleared {
                change: link("/usr/local/bin/octoboardd"),
                outcome: Outcome::Cleared,
            })
            .unwrap(),
            serde_json::json!({
                "change": { "kind": "command_line", "path": "/usr/local/bin/octoboardd" },
                "outcome": { "result": "cleared" },
            })
        );
        let left = serde_json::to_value(Outcome::DeviceKeyLeft {
            removal: KeyRemoval::nothing(),
        })
        .unwrap();
        assert_eq!(left["result"], "device_key_left");
        assert_eq!(left["removal"]["line"]["result"], "not_installed");
        assert_eq!(
            serde_json::to_value(Outcome::CommandLineLeft {
                error: command_line::Error::new(
                    command_line::ErrorCode::Refused,
                    Some("/usr/local/bin/octoboardd".into()),
                    "cancelled",
                ),
            })
            .unwrap(),
            serde_json::json!({
                "result": "command_line_left",
                "error": {
                    "code": "refused", "path": "/usr/local/bin/octoboardd", "detail": "cancelled",
                },
            })
        );
    }

    #[test]
    fn only_the_local_window_may_list_and_clear() {
        assert!(include_str!("../capabilities/default.json").contains("\"allow-system-changes\""));
        for capability in [
            include_str!("../capabilities/remote.json"),
            include_str!("../capabilities/remote-window.json"),
            include_str!("../capabilities/flyout.json"),
        ] {
            assert!(!capability.contains("allow-system-changes"));
        }
        let permission = include_str!("../permissions/system-changes.toml");
        for command in ["system_changes_list", "system_changes_clear"] {
            assert!(permission.contains(&format!("\"{command}\"")), "{command}");
        }
    }
}
