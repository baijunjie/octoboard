//! The worktree settings: how a project session is asked to use git worktrees, and how it delivers
//! finished work.
//!
//! Each item has an application-wide value and, per project, an override that either holds its own
//! value or follows the application-wide one. [`effective`] is the one place the two are combined:
//! anything that needs "the worktree settings of this project" asks it, so a rule that reads the
//! settings at launch and one that reads them while a session runs always agree.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::protocol::{present_option, Project};
use crate::store::Store;

/// How finished work leaves its branch.
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

/// The wire names of the items, the same in every shape below.
pub const ITEMS: [&str; 4] = [
    "use_worktrees",
    "rebase_before_merge",
    "delivery",
    "revert_gate",
];

/// The values the worktree settings take, application-wide or resolved for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSettings {
    /// Whether project sessions are asked to develop in a worktree of their own.
    pub use_worktrees: bool,
    /// Whether a branch is rebased onto its target branch before it is merged.
    pub rebase_before_merge: bool,
    pub delivery: Delivery,
    /// Whether a branch update that quietly undoes changes already on the target branch is
    /// blocked.
    pub revert_gate: bool,
}

impl Default for WorktreeSettings {
    /// The values of a fresh install.
    fn default() -> Self {
        Self {
            use_worktrees: true,
            rebase_before_merge: true,
            delivery: Delivery::Ask,
            revert_gate: true,
        }
    }
}

/// A project's own choices; an item left `None` follows the application-wide value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeOverride {
    pub use_worktrees: Option<bool>,
    pub rebase_before_merge: Option<bool>,
    pub delivery: Option<Delivery>,
    pub revert_gate: Option<bool>,
}

/// A change to the application-wide values: an item left `None` is left alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct WorktreeSettingsChanges {
    #[serde(default)]
    pub use_worktrees: Option<bool>,
    #[serde(default)]
    pub rebase_before_merge: Option<bool>,
    #[serde(default)]
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub revert_gate: Option<bool>,
}

impl WorktreeSettingsChanges {
    /// `current` with the changes applied.
    pub fn applied_to(&self, current: WorktreeSettings) -> WorktreeSettings {
        resolve(
            &current,
            &WorktreeOverride {
                use_worktrees: self.use_worktrees,
                rebase_before_merge: self.rebase_before_merge,
                delivery: self.delivery,
                revert_gate: self.revert_gate,
            },
        )
    }
}

impl From<WorktreeSettings> for WorktreeSettingsChanges {
    /// The changes that set every item to `settings`.
    fn from(settings: WorktreeSettings) -> Self {
        Self {
            use_worktrees: Some(settings.use_worktrees),
            rebase_before_merge: Some(settings.rebase_before_merge),
            delivery: Some(settings.delivery),
            revert_gate: Some(settings.revert_gate),
        }
    }
}

/// A change to a project's override. Per item, absent leaves it alone, an explicit `null` goes
/// back to following the application-wide value, and a value sets the project's own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct WorktreeOverrideChanges {
    #[serde(default, deserialize_with = "present_option")]
    pub use_worktrees: Option<Option<bool>>,
    #[serde(default, deserialize_with = "present_option")]
    pub rebase_before_merge: Option<Option<bool>>,
    #[serde(default, deserialize_with = "present_option")]
    pub delivery: Option<Option<Delivery>>,
    #[serde(default, deserialize_with = "present_option")]
    pub revert_gate: Option<Option<bool>>,
}

impl WorktreeOverrideChanges {
    /// `current` with the changes applied.
    pub fn applied_to(&self, current: WorktreeOverride) -> WorktreeOverride {
        WorktreeOverride {
            use_worktrees: self.use_worktrees.unwrap_or(current.use_worktrees),
            rebase_before_merge: self
                .rebase_before_merge
                .unwrap_or(current.rebase_before_merge),
            delivery: self.delivery.unwrap_or(current.delivery),
            revert_gate: self.revert_gate.unwrap_or(current.revert_gate),
        }
    }
}

/// Each item the project holds itself, the application-wide value for the rest.
pub fn resolve(global: &WorktreeSettings, project: &WorktreeOverride) -> WorktreeSettings {
    WorktreeSettings {
        use_worktrees: project.use_worktrees.unwrap_or(global.use_worktrees),
        rebase_before_merge: project
            .rebase_before_merge
            .unwrap_or(global.rebase_before_merge),
        delivery: project.delivery.unwrap_or(global.delivery),
        revert_gate: project.revert_gate.unwrap_or(global.revert_gate),
    }
}

/// The worktree settings in force for `project`, read from the store as they are now. The only
/// place the application-wide values and a project's override are combined: every consumer asks
/// this, none resolves them itself.
pub fn effective(store: &Store, project: &Project) -> Result<WorktreeSettings> {
    Ok(resolve(&store.worktree_settings()?, &project.worktrees))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_on_on_ask_on() {
        assert_eq!(
            WorktreeSettings::default(),
            WorktreeSettings {
                use_worktrees: true,
                rebase_before_merge: true,
                delivery: Delivery::Ask,
                revert_gate: true,
            }
        );
        assert_eq!(
            WorktreeOverride::default(),
            WorktreeOverride {
                use_worktrees: None,
                rebase_before_merge: None,
                delivery: None,
                revert_gate: None,
            }
        );
    }

    #[test]
    fn an_item_the_project_holds_wins_and_the_rest_follow_the_global_value() {
        let global = WorktreeSettings {
            use_worktrees: false,
            rebase_before_merge: false,
            delivery: Delivery::Merge,
            revert_gate: false,
        };
        assert_eq!(resolve(&global, &WorktreeOverride::default()), global);
        let own = WorktreeOverride {
            use_worktrees: Some(true),
            delivery: Some(Delivery::PullRequest),
            ..WorktreeOverride::default()
        };
        assert_eq!(
            resolve(&global, &own),
            WorktreeSettings {
                use_worktrees: true,
                rebase_before_merge: false,
                delivery: Delivery::PullRequest,
                revert_gate: false,
            }
        );
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
        let changes: WorktreeOverrideChanges =
            serde_json::from_str(r#"{"use_worktrees":null,"delivery":"merge"}"#).unwrap();
        assert_eq!(changes.use_worktrees, Some(None));
        assert_eq!(changes.delivery, Some(Some(Delivery::Merge)));
        assert_eq!(changes.revert_gate, None);
        let current = WorktreeOverride {
            use_worktrees: Some(false),
            revert_gate: Some(false),
            ..WorktreeOverride::default()
        };
        assert_eq!(
            changes.applied_to(current),
            WorktreeOverride {
                use_worktrees: None,
                rebase_before_merge: None,
                delivery: Some(Delivery::Merge),
                revert_gate: Some(false),
            }
        );
        assert!(serde_json::from_str::<WorktreeOverrideChanges>(r#"{"delivery":"x"}"#).is_err());
    }

    #[test]
    fn settings_changes_leave_absent_items_alone() {
        let changes: WorktreeSettingsChanges =
            serde_json::from_str(r#"{"revert_gate":false}"#).unwrap();
        let applied = changes.applied_to(WorktreeSettings::default());
        assert!(!applied.revert_gate);
        assert!(applied.use_worktrees);
        assert_eq!(applied.delivery, Delivery::Ask);
    }
}
