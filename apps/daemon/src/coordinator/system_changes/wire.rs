//! What the daemon's features have in effect on its host outside Octoboard's own directories, and
//! what clearing all of it came to, as the daemon's local routes answer them to `octoboardd
//! uninstall` and to the desktop application's own window. They are no part of the protocol a
//! client connection speaks: no request of a control connection lists or clears.

use serde::Serialize;

use crate::protocol::{AgentId, Code, Params};

/// One modification a feature of the daemon has in effect outside Octoboard's own directories,
/// which turning that feature off undoes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SystemChange {
    /// An account shares its agent's default configuration, or its last change of sharing is
    /// incomplete: entries of the account's directory `config_dir` are links into the default
    /// account's directory.
    AccountSharing {
        /// The account's id.
        account: String,
        name: String,
        agent: AgentId,
        config_dir: String,
    },
}

/// What stays in `dir` after a modification was cleared, because removing it would lose the
/// user's data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemChangeKept {
    pub dir: String,
    /// The entries of `dir` that hold it.
    pub entries: Vec<String>,
}

/// Why a modification was not cleared: an `error` reply's `code`, `params` and `message`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemChangeFailure {
    pub code: Code,
    pub params: Params,
    pub message: String,
}

/// What clearing one modification came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemChangeResult {
    pub change: SystemChange,
    /// Why it was not cleared, in which case it is still in effect; absent when it was cleared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<SystemChangeFailure>,
    /// The user's data that stays where the modification put it; absent when nothing stays, when
    /// it was not cleared, and when what stays could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<SystemChangeKept>,
}

/// One of the daemon's features that could not tell what it has in effect, so that what became
/// of its modifications is not known: some may have been cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemChangeFeatureFailure {
    /// Which feature, by the `kind` of its modifications.
    pub kind: String,
    pub failure: SystemChangeFailure,
}

/// What is in effect, as `POST /system-changes` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemChangesListed {
    pub changes: Vec<SystemChange>,
}

/// What clearing everything came to, as `POST /system-changes/clear` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemChangesCleared {
    /// What became of each modification, those cleared first.
    pub results: Vec<SystemChangeResult>,
    /// The features that could not tell what they have in effect, whose modifications `results`
    /// therefore leaves out; the others were cleared all the same.
    pub failed: Vec<SystemChangeFeatureFailure>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::error_code;

    /// The bodies the routes answer, as `apps/daemon/testdata/system_changes_wire.json` holds
    /// them; the desktop shell's reader (`apps/desktop/src-tauri/src/system_changes/daemon.rs`)
    /// reads the same file, which pins its copy of these shapes to these types.
    #[test]
    fn the_changes_and_their_results_are_spelled_as_the_shared_fixture_holds_them() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../testdata/system_changes_wire.json"))
                .unwrap();
        let change = SystemChange::AccountSharing {
            account: "acct-1".into(),
            name: "Work".into(),
            agent: AgentId::CLAUDE,
            config_dir: "/home/u/.claude-work".into(),
        };
        let failure = |code, params: &[(&str, &str)], message: &str| SystemChangeFailure {
            code,
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            message: message.into(),
        };
        let listed = SystemChangesListed {
            changes: vec![change.clone()],
        };
        assert_eq!(serde_json::to_value(listed).unwrap(), fixture["listed"]);
        let cleared = SystemChangesCleared {
            results: vec![
                SystemChangeResult {
                    change: change.clone(),
                    failure: None,
                    kept: Some(SystemChangeKept {
                        dir: "/home/u/.claude".into(),
                        entries: vec!["projects".into()],
                    }),
                },
                SystemChangeResult {
                    change,
                    failure: Some(failure(
                        error_code::ACCOUNT_IN_USE,
                        &[("name", "Work")],
                        "in use",
                    )),
                    kept: None,
                },
            ],
            failed: vec![SystemChangeFeatureFailure {
                kind: "account_sharing".into(),
                failure: failure(error_code::INTERNAL_ERROR, &[], "unreadable"),
            }],
        };
        assert_eq!(serde_json::to_value(cleared).unwrap(), fixture["cleared"]);
    }
}
