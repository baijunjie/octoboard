//! The git workflow settings: where a project session is asked to develop (on the current branch,
//! on a branch of its own, or in a worktree of its own), how it delivers finished work, and the
//! revert gate.
//!
//! Each item has an application-wide value and, per project, an override that either holds its own
//! value or follows the application-wide one. [`effective`] is the one place the two are combined:
//! anything that needs "the git workflow settings of this project" asks it, so a rule that reads
//! the settings at launch and one that reads them while a session runs always agree.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::protocol::{present_option, Project};
use crate::store::Store;

/// How finished work leaves its branch.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
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
pub enum Development {
    /// Directly on the current branch; the session is given no rule.
    InPlace,
    /// On a branch of its own, in the project directory.
    Branch,
    /// On a branch of its own, in a worktree of its own.
    Worktree,
}

/// The wire names of the items, the same in every shape below.
pub const ITEMS: [&str; 5] = [
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
    pub development: Development,
    /// Whether commits are consolidated into independently meaningful, revertible commits before
    /// delivering finished work by merge, push or pull request. Moot while `development` is
    /// `InPlace`.
    pub squash_before_delivery: bool,
    /// Whether a branch is rebased onto its target branch before delivery (merge, push or pull
    /// request). The wire name is kept for compatibility. Moot while `development` is `InPlace`.
    pub rebase_before_merge: bool,
    /// Moot while `development` is `InPlace`.
    pub delivery: Delivery,
    /// Whether a branch update that quietly undoes changes already on the target branch is
    /// blocked. Independent of `development`.
    pub revert_gate: bool,
}

impl Default for GitWorkflowSettings {
    /// The values of a fresh install.
    fn default() -> Self {
        Self {
            development: Development::Worktree,
            squash_before_delivery: false,
            rebase_before_merge: true,
            delivery: Delivery::Ask,
            revert_gate: false,
        }
    }
}

/// A project's own choices; an item left `None` follows the application-wide value.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorkflowOverride {
    pub development: Option<Development>,
    pub squash_before_delivery: Option<bool>,
    pub rebase_before_merge: Option<bool>,
    pub delivery: Option<Delivery>,
    pub revert_gate: Option<bool>,
}

/// A change to the application-wide values: an item left `None` is left alone.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct GitWorkflowSettingsChanges {
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub development: Option<Development>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub squash_before_delivery: Option<bool>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub rebase_before_merge: Option<bool>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub delivery: Option<Delivery>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub revert_gate: Option<bool>,
}

impl GitWorkflowSettingsChanges {
    /// `current` with the changes applied.
    pub fn applied_to(&self, current: GitWorkflowSettings) -> GitWorkflowSettings {
        resolve(
            &current,
            &GitWorkflowOverride {
                development: self.development,
                squash_before_delivery: self.squash_before_delivery,
                rebase_before_merge: self.rebase_before_merge,
                delivery: self.delivery,
                revert_gate: self.revert_gate,
            },
        )
    }
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
    pub development: Option<Option<Development>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub squash_before_delivery: Option<Option<bool>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub rebase_before_merge: Option<Option<bool>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub delivery: Option<Option<Delivery>>,
    #[serde(default, deserialize_with = "present_option")]
    #[cfg_attr(test, ts(optional))]
    pub revert_gate: Option<Option<bool>>,
}

impl GitWorkflowOverrideChanges {
    /// `current` with the changes applied.
    pub fn applied_to(&self, current: GitWorkflowOverride) -> GitWorkflowOverride {
        GitWorkflowOverride {
            development: self.development.unwrap_or(current.development),
            squash_before_delivery: self
                .squash_before_delivery
                .unwrap_or(current.squash_before_delivery),
            rebase_before_merge: self
                .rebase_before_merge
                .unwrap_or(current.rebase_before_merge),
            delivery: self.delivery.unwrap_or(current.delivery),
            revert_gate: self.revert_gate.unwrap_or(current.revert_gate),
        }
    }
}

/// Each item the project holds itself, the application-wide value for the rest.
pub fn resolve(global: &GitWorkflowSettings, project: &GitWorkflowOverride) -> GitWorkflowSettings {
    GitWorkflowSettings {
        development: project.development.unwrap_or(global.development),
        squash_before_delivery: project
            .squash_before_delivery
            .unwrap_or(global.squash_before_delivery),
        rebase_before_merge: project
            .rebase_before_merge
            .unwrap_or(global.rebase_before_merge),
        delivery: project.delivery.unwrap_or(global.delivery),
        revert_gate: project.revert_gate.unwrap_or(global.revert_gate),
    }
}

/// The git workflow settings in force for `project`, read from the store as they are now. The only
/// place the application-wide values and a project's override are combined: every consumer asks
/// this, none resolves them itself.
pub fn effective(store: &Store, project: &Project) -> Result<GitWorkflowSettings> {
    Ok(resolve(
        &store.git_workflow_settings()?,
        &project.git_workflow,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_worktree_on_ask_off() {
        assert_eq!(
            GitWorkflowSettings::default(),
            GitWorkflowSettings {
                development: Development::Worktree,
                squash_before_delivery: false,
                rebase_before_merge: true,
                delivery: Delivery::Ask,
                revert_gate: false,
            }
        );
        assert_eq!(
            GitWorkflowOverride::default(),
            GitWorkflowOverride {
                development: None,
                squash_before_delivery: None,
                rebase_before_merge: None,
                delivery: None,
                revert_gate: None,
            }
        );
    }

    #[test]
    fn an_item_the_project_holds_wins_and_the_rest_follow_the_global_value() {
        let global = GitWorkflowSettings {
            development: Development::InPlace,
            squash_before_delivery: false,
            rebase_before_merge: false,
            delivery: Delivery::Merge,
            revert_gate: false,
        };
        assert_eq!(resolve(&global, &GitWorkflowOverride::default()), global);
        let own = GitWorkflowOverride {
            development: Some(Development::Branch),
            delivery: Some(Delivery::PullRequest),
            ..GitWorkflowOverride::default()
        };
        assert_eq!(
            resolve(&global, &own),
            GitWorkflowSettings {
                development: Development::Branch,
                squash_before_delivery: false,
                rebase_before_merge: false,
                delivery: Delivery::PullRequest,
                revert_gate: false,
            }
        );
    }

    #[test]
    fn squash_changes_distinguish_inheritance_from_explicit_false() {
        let global = GitWorkflowSettings {
            squash_before_delivery: true,
            ..GitWorkflowSettings::default()
        };
        let own = GitWorkflowOverride {
            squash_before_delivery: Some(false),
            ..GitWorkflowOverride::default()
        };
        assert!(!resolve(&global, &own).squash_before_delivery);
        let absent: GitWorkflowOverrideChanges = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.applied_to(own), own);
        let inherit: GitWorkflowOverrideChanges =
            serde_json::from_str(r#"{"squash_before_delivery":null}"#).unwrap();
        assert!(resolve(&global, &inherit.applied_to(own)).squash_before_delivery);
        let off: GitWorkflowSettingsChanges =
            serde_json::from_str(r#"{"squash_before_delivery":false}"#).unwrap();
        assert!(!off.applied_to(global).squash_before_delivery);
    }

    #[test]
    fn development_uses_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&Development::InPlace).unwrap(),
            r#""in_place""#
        );
        assert_eq!(
            serde_json::from_str::<Development>(r#""branch""#).unwrap(),
            Development::Branch
        );
        assert!(serde_json::from_str::<Development>(r#"true"#).is_err());
    }

    #[test]
    fn delivery_uses_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&Delivery::PullRequest).unwrap(),
            r#""pull_request""#
        );
        assert!(serde_json::from_str::<Delivery>(r#""rebase""#).is_err());
    }

    #[test]
    fn override_changes_tell_absent_from_null_from_a_value() {
        let changes: GitWorkflowOverrideChanges =
            serde_json::from_str(r#"{"development":null,"delivery":"merge"}"#).unwrap();
        assert_eq!(changes.development, Some(None));
        assert_eq!(changes.delivery, Some(Some(Delivery::Merge)));
        assert_eq!(changes.revert_gate, None);
        let current = GitWorkflowOverride {
            development: Some(Development::InPlace),
            revert_gate: Some(false),
            ..GitWorkflowOverride::default()
        };
        assert_eq!(
            changes.applied_to(current),
            GitWorkflowOverride {
                development: None,
                squash_before_delivery: None,
                rebase_before_merge: None,
                delivery: Some(Delivery::Merge),
                revert_gate: Some(false),
            }
        );
        assert!(serde_json::from_str::<GitWorkflowOverrideChanges>(r#"{"delivery":"x"}"#).is_err());
    }

    #[test]
    fn settings_changes_leave_absent_items_alone() {
        let changes: GitWorkflowSettingsChanges =
            serde_json::from_str(r#"{"revert_gate":true}"#).unwrap();
        let applied = changes.applied_to(GitWorkflowSettings::default());
        assert!(applied.revert_gate);
        assert_eq!(applied.development, Development::Worktree);
        assert_eq!(applied.delivery, Delivery::Ask);
    }
}
