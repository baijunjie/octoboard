//! The git workflow settings as they go on the wire: where a project session is asked to develop,
//! how it delivers finished work, and the revert gate, application-wide and as a project's
//! override.

use serde::{Deserialize, Serialize};

use super::fields::present_option;

/// How finished work leaves its branch.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDelivery {
    /// The agent asks the user each time.
    Ask,
    /// The branch is merged into its target branch locally.
    Merge,
    /// The branch is pushed and a pull request opened.
    PullRequest,
}

/// Where a project session is asked to develop.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDevelopment {
    /// Directly on the current branch; the session is given no rule.
    InPlace,
    /// On a branch of its own, in the project directory.
    Branch,
    /// On a branch of its own, in a worktree of its own.
    Worktree,
}

/// The wire names of the items, the same in every shape below.
pub const GIT_WORKFLOW_ITEMS: [&str; 5] = [
    "development",
    "squash_before_delivery",
    "rebase_before_merge",
    "delivery",
    "revert_gate",
];

/// The values the git workflow settings take, application-wide or resolved for a project.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorkflowSettings {
    pub development: GitDevelopment,
    /// Whether a branch's commits are squashed into one commit for the feature it delivers, a
    /// separate commit kept only for an unrelated bug fix, before delivering finished work by
    /// merge, push or pull request. Moot while `development` is `InPlace`.
    pub squash_before_delivery: bool,
    /// Whether a branch is rebased onto its target branch before delivery (merge, push or pull
    /// request). The wire name is kept for compatibility. Moot while `development` is `InPlace`.
    pub rebase_before_merge: bool,
    /// Moot while `development` is `InPlace`.
    pub delivery: GitDelivery,
    /// Whether a branch update that quietly undoes changes already on the target branch is
    /// blocked. Independent of `development`.
    pub revert_gate: bool,
}

impl Default for GitWorkflowSettings {
    /// The values of a fresh install.
    fn default() -> Self {
        Self {
            development: GitDevelopment::Worktree,
            squash_before_delivery: false,
            rebase_before_merge: true,
            delivery: GitDelivery::Ask,
            revert_gate: false,
        }
    }
}

/// A project's own choices; an item left `None` follows the application-wide value.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorkflowOverride {
    pub development: Option<GitDevelopment>,
    pub squash_before_delivery: Option<bool>,
    pub rebase_before_merge: Option<bool>,
    pub delivery: Option<GitDelivery>,
    pub revert_gate: Option<bool>,
}

/// A change to the application-wide values: an item left `None` is left alone.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct GitWorkflowSettingsChanges {
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub development: Option<GitDevelopment>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub squash_before_delivery: Option<bool>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub rebase_before_merge: Option<bool>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub delivery: Option<GitDelivery>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub revert_gate: Option<bool>,
}

impl From<GitWorkflowSettings> for GitWorkflowSettingsChanges {
    /// The changes that set every item to `settings`.
    fn from(settings: GitWorkflowSettings) -> Self {
        Self {
            development: Some(settings.development),
            squash_before_delivery: Some(settings.squash_before_delivery),
            rebase_before_merge: Some(settings.rebase_before_merge),
            delivery: Some(settings.delivery),
            revert_gate: Some(settings.revert_gate),
        }
    }
}

/// A change to a project's override. Per item, absent leaves it alone, an explicit `null` goes
/// back to following the application-wide value, and a value sets the project's own.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct GitWorkflowOverrideChanges {
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub development: Option<Option<GitDevelopment>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub squash_before_delivery: Option<Option<bool>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub rebase_before_merge: Option<Option<bool>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub delivery: Option<Option<GitDelivery>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub revert_gate: Option<Option<bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_uses_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&GitDevelopment::InPlace).unwrap(),
            r#""in_place""#
        );
        assert_eq!(
            serde_json::from_str::<GitDevelopment>(r#""branch""#).unwrap(),
            GitDevelopment::Branch
        );
        assert!(serde_json::from_str::<GitDevelopment>(r#"true"#).is_err());
    }

    #[test]
    fn delivery_uses_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&GitDelivery::PullRequest).unwrap(),
            r#""pull_request""#
        );
        assert!(serde_json::from_str::<GitDelivery>(r#""rebase""#).is_err());
    }

    #[test]
    fn override_changes_tell_absent_from_null_from_a_value() {
        let changes: GitWorkflowOverrideChanges =
            serde_json::from_str(r#"{"development":null,"delivery":"merge"}"#).unwrap();
        assert_eq!(changes.development, Some(None));
        assert_eq!(changes.delivery, Some(Some(GitDelivery::Merge)));
        assert_eq!(changes.revert_gate, None);
        assert!(serde_json::from_str::<GitWorkflowOverrideChanges>(r#"{"delivery":"x"}"#).is_err());
    }
}
