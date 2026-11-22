//! The clean-up `uninstall` makes first: what the daemon's features have in effect outside
//! Octoboard's own directories, and clearing all of it (`crate::coordinator::system_changes`).
//! It is no command of its own, and no client connection has a request for it and no session a
//! tool.
//!
//! Clearing is the running daemon's work: turning an account's sharing off ends and resumes the
//! account's idle sessions and is refused while one of them works, which only the daemon that runs
//! them can tell and do. So it is asked of the running daemon, over loopback with the launch key
//! from its owner-only record, as `stop` asks its check (`POST /system-changes`,
//! `POST /system-changes/clear`). With no daemon running, what is in effect is read from the store
//! under the daemon's lock, and nothing here clears: clearing in the daemon's place would not see
//! the sessions a planned stop left running in their holders. `uninstall` starts the daemon first.

use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::coordinator::system_changes;
use crate::owner_record::Owner;
use crate::protocol::AgentId;
use crate::{instance_lock, loopback, paths};

use super::background::RESTART_WAIT;
use super::succession::{find_daemon, find_daemon_unchanged, Found};

/// How long the daemon is given to say what is in effect: it reads its store.
const LIST_WAIT: Duration = Duration::from_secs(15);

/// How long the daemon is given to clear everything: each account's idle sessions are ended and
/// resumed around its change, one account after another.
const CLEAR_WAIT: Duration = Duration::from_secs(30 * 60);

/// What clearing leaves, said before the confirmation and after the result.
pub(super) const KEPT: &str = "The conversation history that sharing moved into a default \
account's directory stays there.";

/// What stays where the clean-up is skipped or leaves something in effect.
pub(super) const LEFT: &str = "the links in each such account's directory that lead into its \
agent's default account's directory, and beside them the backups of the account's own \
configuration that sharing set aside (`<entry>.bak-<stamp>`)";

/// The daemon `owner` names, asked over loopback with its launch key.
struct Asked {
    owner: Owner,
    key: String,
}

impl Asked {
    fn post(&self, path: &str, wait: Duration) -> Result<Value> {
        let answer =
            loopback::post(self.owner.port, &self.key, path, b"{}", wait).map_err(|err| {
                match loopback::error_status(&err) {
                    Some(404) => anyhow::anyhow!(
                        "the running daemon (pid {}) is an earlier build, which cannot be asked \
                         for the clean-up; restart it into this build with `octoboardd restart`",
                        self.owner.identity.pid.0
                    ),
                    _ => anyhow::Error::new(err).context(format!(
                        "asking pid {} on port {} ({path})",
                        self.owner.identity.pid.0, self.owner.port
                    )),
                }
            })?;
        serde_json::from_slice(&answer).context("the daemon's answer could not be read")
    }
}

/// The daemon that runs against this data directory, to ask, or `None` when none runs. An error
/// where one runs that cannot be asked: it has not recorded itself, or its key is not recorded.
/// With `unchanged`, found without creating `daemon.lock` where it is not there.
fn running(unchanged: bool) -> Result<Option<Asked>> {
    let found = match unchanged {
        true => find_daemon_unchanged(RESTART_WAIT, false)?,
        false => find_daemon(RESTART_WAIT, false)?,
    };
    match found {
        Found::Running { owner, .. } => match owner.recorded_key() {
            Some(key) => Ok(Some(Asked { owner, key })),
            None => anyhow::bail!(
                "{} is running (pid {}), but its key is not recorded, so it cannot be asked",
                crate::APP_NAME,
                owner.identity.pid.0
            ),
        },
        Found::Unrecorded => anyhow::bail!(
            "{} is running, but its address is not known (it may still be starting or stopping)",
            crate::APP_NAME
        ),
        Found::NotRunning { .. } => Ok(None),
    }
}

/// What is in effect now, each a `SystemChange` as the daemon answers it: asked of the running
/// daemon, else read from the store, which with `unchanged` is read as it is ([`recorded`]).
pub(super) fn in_effect(unchanged: bool) -> Result<Vec<Value>> {
    let listed = match running(unchanged)? {
        Some(daemon) => daemon.post("/system-changes", LIST_WAIT)?,
        None => recorded(&paths::db_path(), unchanged)?,
    };
    Ok(listed["changes"].as_array().cloned().unwrap_or_default())
}

/// Has the running daemon clear everything in effect, and answers what became of each item
/// (`SystemChangesCleared`). An error when no daemon runs.
pub(super) fn clear() -> Result<Value> {
    match running(false)? {
        Some(daemon) => daemon.post("/system-changes/clear", CLEAR_WAIT),
        None => anyhow::bail!("{} is not running", crate::APP_NAME),
    }
}

/// What the store at `path` records as in effect while no daemon runs, read under the daemon's
/// lock so that none starts under the read. A user for whom no daemon has ever run has no store,
/// and none is created: nothing is in effect. With `unchanged` nothing of the data directory
/// changes, as a dry run promises: the lock, which the read would create, is not taken, and the
/// store is read as it is, which fails for one a daemon's start would migrate first.
fn recorded(path: &std::path::Path, unchanged: bool) -> Result<Value> {
    let changes = match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        Ok(_) if unchanged => {
            let store = crate::store::Store::open_unchanged(path).context(
                "read without changing it, as `--dry-run` does (`octoboardd uninstall` without \
                 `--dry-run` brings the store up to date first, as a daemon's start does)",
            )?;
            system_changes::list(&store)?
        }
        Ok(_) => {
            let Some(_lock) = instance_lock::hold()? else {
                anyhow::bail!(
                    "a daemon started meanwhile against {}",
                    paths::data_dir().display()
                );
            };
            let store = crate::store::Store::open(path)?;
            system_changes::list(&store)?
        }
    };
    Ok(serde_json::to_value(system_changes::SystemChangesListed {
        changes,
    })?)
}

/// The product name of the agent the wire names `id`, or `id` itself for one this build does not
/// know.
fn agent_label(id: &str) -> &str {
    AgentId::from_wire_name(id).map_or(id, |agent| agent.descriptor().label)
}

/// One modification, named for the user: `still` words it as in effect, and otherwise as cleared.
fn change_text(change: &Value, still: bool) -> String {
    match change["kind"].as_str() {
        Some("account_sharing") => {
            let agent = agent_label(change["agent"].as_str().unwrap_or(""));
            let name = change["name"].as_str().unwrap_or("");
            let dir = change["config_dir"].as_str().unwrap_or("");
            let verb = if still { "shares" } else { "no longer shares" };
            format!("{agent} account \"{name}\" {verb} the default configuration ({dir})")
        }
        // A kind this build has no words for, as a later daemon's would be.
        kind => kind.unwrap_or("an item of a kind not named").to_owned(),
    }
}

/// The lines naming what is in effect, one per item.
pub(super) fn list_lines(changes: &[Value]) -> Vec<String> {
    changes
        .iter()
        .map(|change| change_text(change, true))
        .collect()
}

/// How many items are still in effect after `cleared`, the features that could not tell counted
/// as one each.
pub(super) fn left_in_effect(cleared: &Value) -> usize {
    let not_cleared = cleared["results"].as_array().map_or(0, |results| {
        results
            .iter()
            .filter(|result| !result["failure"].is_null())
            .count()
    });
    not_cleared + cleared["failed"].as_array().map_or(0, Vec::len)
}

/// What clearing came to: each item cleared with what of it is kept, each item left with the
/// daemon's own reason, the features that could not tell, then the count.
pub(super) fn result_text(cleared: &Value) -> String {
    let mut lines = Vec::new();
    let mut done = 0;
    for result in cleared["results"].as_array().into_iter().flatten() {
        let failure = &result["failure"];
        if failure.is_null() {
            done += 1;
            lines.push(format!(
                "cleared: {}",
                change_text(&result["change"], false)
            ));
            let kept = &result["kept"];
            let entries: Vec<&str> = kept["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if let (Some(dir), false) = (kept["dir"].as_str(), entries.is_empty()) {
                lines.push(format!(
                    "  kept: its conversation history stays in {dir} ({})",
                    entries.join(", ")
                ));
            }
        } else {
            lines.push(format!(
                "not cleared: {}: {}",
                change_text(&result["change"], true),
                failure["message"].as_str().unwrap_or("no reason given")
            ));
        }
    }
    for failed in cleared["failed"].as_array().into_iter().flatten() {
        lines.push(format!(
            "not known: what became of `{}` could not be told, and some of it may have been \
             cleared: {}",
            failed["kind"].as_str().unwrap_or(""),
            failed["failure"]["message"]
                .as_str()
                .unwrap_or("no reason given")
        ));
    }
    let left = left_in_effect(cleared);
    lines.push(if left == 0 {
        format!("{done} cleared; nothing is left in effect.")
    } else {
        format!("{done} cleared, {left} still in effect.")
    });
    lines.join("\n")
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;

    pub(in crate::cli) fn sharing(name: &str) -> Value {
        json!({
            "kind": "account_sharing", "account": format!("id-{name}"), "name": name,
            "agent": "claude", "config_dir": format!("/home/u/.claude-{name}"),
        })
    }

    /// Everything cleared: one account, whose history stays.
    pub(in crate::cli) fn all_cleared() -> Value {
        json!({
            "results": [{
                "change": sharing("work"),
                "kept": { "dir": "/home/u/.claude", "entries": ["projects", "todos"] },
            }],
            "failed": [],
        })
    }

    /// One account cleared and one left, in use.
    pub(in crate::cli) fn one_left() -> Value {
        json!({
            "results": [
                { "change": sharing("side") },
                { "change": sharing("work"), "failure": {
                    "code": "account_in_use", "params": { "name": "work" },
                    "message": "account `work` cannot change its sharing of the default \
                                configuration while one of its sessions is running or starting",
                } },
            ],
            "failed": [],
        })
    }

    /// Each item is named with its agent's product name, the account's name and its directory.
    #[test]
    fn each_item_in_effect_is_named_with_its_agent_account_and_directory() {
        assert_eq!(
            list_lines(&[sharing("work"), json!({ "kind": "later_feature" })]),
            [
                "Claude Code account \"work\" shares the default configuration \
                 (/home/u/.claude-work)",
                "later_feature"
            ]
        );
        assert!(list_lines(&[]).is_empty());
    }

    /// A cleared item says what of it is kept; nothing is left in effect.
    #[test]
    fn a_cleared_item_says_what_is_kept() {
        let text = result_text(&all_cleared());
        assert!(
            text.contains(
                "cleared: Claude Code account \"work\" no longer shares the default configuration"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "kept: its conversation history stays in /home/u/.claude (projects, todos)"
            ),
            "{text}"
        );
        assert!(
            text.ends_with("1 cleared; nothing is left in effect."),
            "{text}"
        );
        assert_eq!(left_in_effect(&all_cleared()), 0);
    }

    /// An account in use is reported as left, with the daemon's own wording, and counted.
    #[test]
    fn an_account_in_use_is_reported_as_left_with_the_daemons_reason() {
        let text = result_text(&one_left());
        assert!(
            text.contains("cleared: Claude Code account \"side\" no longer shares"),
            "{text}"
        );
        assert!(
            text.contains(
                "not cleared: Claude Code account \"work\" shares the default configuration \
                 (/home/u/.claude-work): account `work` cannot change its sharing"
            ),
            "{text}"
        );
        assert!(text.ends_with("1 cleared, 1 still in effect."), "{text}");
        assert_eq!(left_in_effect(&one_left()), 1);
    }

    /// A feature that could not tell what it has counts as left, and is said.
    #[test]
    fn a_feature_that_could_not_tell_counts_as_left() {
        let cleared = json!({
            "results": [],
            "failed": [{ "kind": "account_sharing", "failure": {
                "code": "internal_error", "params": {}, "message": "unreadable",
            } }],
        });
        assert_eq!(left_in_effect(&cleared), 1);
        let text = result_text(&cleared);
        assert!(
            text.contains("`account_sharing`") && text.contains("unreadable"),
            "{text}"
        );
    }

    /// With no daemon running, a read that changes nothing reads the store as it is: no lock
    /// file is made beside it, a store a daemon's start would migrate is said to be unreadable
    /// that way and stays at its version, and where there is no store nothing is in effect and
    /// none is made.
    #[test]
    fn a_read_that_changes_nothing_takes_no_lock_and_migrates_no_store() {
        use crate::store::Store;

        let dir = crate::test_support::ScratchDir::new("cleanup-unchanged");
        let path = dir.join("octoboard.db");
        let none = recorded(&path, true).unwrap();
        assert_eq!(none["changes"], json!([]));
        assert!(!path.exists());

        drop(Store::open(&path).unwrap());
        assert_eq!(recorded(&path, true).unwrap()["changes"], json!([]));
        let earlier = Store::SCHEMA_VERSION - 1;
        rusqlite::Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", earlier)
            .unwrap();
        let err = format!("{:#}", recorded(&path, true).unwrap_err());
        assert!(
            err.contains("read without changing it, as `--dry-run` does")
                && err.contains(&format!("it is at version {earlier}")),
            "{err}"
        );
        assert_eq!(Store::peek_schema_version(&path), Some(earlier));
        let left: Vec<_> = std::fs::read_dir(&*dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, ["octoboard.db"], "nothing was made beside the store");
    }

    /// The running daemon is asked on its two local routes with the launch key, and a daemon
    /// from before the routes existed is named as an earlier build.
    #[test]
    fn the_running_daemon_is_asked_on_its_local_routes_with_the_launch_key() {
        use crate::cli::background::tests::{answer_once, owner_on, KEY};

        let ok = |body: &str| {
            format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        };
        for (path, wait, body) in [
            ("/system-changes", LIST_WAIT, r#"{"changes":[]}"#),
            (
                "/system-changes/clear",
                CLEAR_WAIT,
                r#"{"results":[],"failed":[]}"#,
            ),
        ] {
            let (port, served) = answer_once(ok(body));
            let asked = Asked {
                owner: owner_on(port),
                key: KEY.to_owned(),
            };
            assert_eq!(
                asked.post(path, wait).unwrap(),
                serde_json::from_str::<Value>(body).unwrap()
            );
            assert!(
                served
                    .join()
                    .unwrap()
                    .starts_with(&format!("POST {path}?key={KEY} ")),
                "{path}"
            );
        }
        let (port, _served) =
            answer_once("HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
        let asked = Asked {
            owner: owner_on(port),
            key: KEY.to_owned(),
        };
        let err = format!(
            "{:#}",
            asked.post("/system-changes", LIST_WAIT).unwrap_err()
        );
        assert!(
            err.contains("earlier build") && err.contains("octoboardd restart"),
            "{err}"
        );
    }
}
