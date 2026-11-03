//! Combining the git workflow settings (where a project session is asked to develop, how it
//! delivers finished work, and the revert gate; the types are in `protocol`).
//!
//! Each item has an application-wide value and, per project, an override that either holds its own
//! value or follows the application-wide one. [`effective`] is the one place the two are combined:
//! anything that needs "the git workflow settings of this project" asks it, so a rule that reads
//! the settings at launch and one that reads them while a session runs always agree.

use anyhow::Result;

use crate::protocol::{
    GitWorkflowOverride, GitWorkflowOverrideChanges, GitWorkflowSettings,
    GitWorkflowSettingsChanges, Project,
};
use crate::store::Store;

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
    use crate::protocol::{GitDelivery, GitDevelopment};

    #[test]
    fn the_defaults_are_worktree_on_ask_off() {
        assert_eq!(
            GitWorkflowSettings::default(),
            GitWorkflowSettings {
                development: GitDevelopment::Worktree,
                squash_before_delivery: false,
                rebase_before_merge: true,
                delivery: GitDelivery::Ask,
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
            development: GitDevelopment::InPlace,
            squash_before_delivery: false,
            rebase_before_merge: false,
            delivery: GitDelivery::Merge,
            revert_gate: false,
        };
        assert_eq!(resolve(&global, &GitWorkflowOverride::default()), global);
        let own = GitWorkflowOverride {
            development: Some(GitDevelopment::Branch),
            delivery: Some(GitDelivery::PullRequest),
            ..GitWorkflowOverride::default()
        };
        assert_eq!(
            resolve(&global, &own),
            GitWorkflowSettings {
                development: GitDevelopment::Branch,
                squash_before_delivery: false,
                rebase_before_merge: false,
                delivery: GitDelivery::PullRequest,
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
    fn override_changes_tell_absent_from_cleared_from_a_value() {
        let changes = GitWorkflowOverrideChanges {
            development: Some(None),
            delivery: Some(Some(GitDelivery::Merge)),
            ..GitWorkflowOverrideChanges::default()
        };
        let current = GitWorkflowOverride {
            development: Some(GitDevelopment::InPlace),
            revert_gate: Some(false),
            ..GitWorkflowOverride::default()
        };
        assert_eq!(
            changes.applied_to(current),
            GitWorkflowOverride {
                development: None,
                squash_before_delivery: None,
                rebase_before_merge: None,
                delivery: Some(GitDelivery::Merge),
                revert_gate: Some(false),
            }
        );
    }

    #[test]
    fn settings_changes_leave_absent_items_alone() {
        let changes: GitWorkflowSettingsChanges =
            serde_json::from_str(r#"{"revert_gate":true}"#).unwrap();
        let applied = changes.applied_to(GitWorkflowSettings::default());
        assert!(applied.revert_gate);
        assert_eq!(applied.development, GitDevelopment::Worktree);
        assert_eq!(applied.delivery, GitDelivery::Ask);
    }
}
