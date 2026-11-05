//! The daemon's one layer for running `git` and reading a repository's layout from disk: the
//! settings every `git` it runs shares ([`mod@env`]), where each of them is built and run, and the
//! runner a family of call sites holds them through ([`run`]), where a worktree's git directories
//! are and how a repository is found ([`layout`]), what a repository's other administrative files
//! say ([`repo_files`]), what its reflogs say of a branch's creation ([`reflog`]), a repository's
//! worktrees and the changes they hold ([`worktrees`]), the reads isolated from the user's
//! environment ([`isolated`]), how a path goes to `git` so that it names that path on every host
//! ([`pathspec`]), and the small readings of `git`'s text that more than one family of
//! callers shares: object ids ([`object_id`]), `<remote>/<branch>` ([`remote_branch`]) and the
//! entries of a file listing ([`tree_entries`]); the lookup of a branch's remote-tracking branch
//! and of the remote default branch ([`remote_branches`]).

pub mod env;
pub mod isolated;
pub mod layout;
pub mod object_id;
pub mod pathspec;
pub mod reflog;
pub mod remote_branch;
pub mod remote_branches;
pub mod repo_files;
pub mod run;
pub mod tree_entries;
pub mod worktrees;
