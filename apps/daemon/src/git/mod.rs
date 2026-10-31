//! The daemon's one layer for running `git` and reading a repository's layout from disk: the
//! settings every `git` it runs shares ([`mod@env`]), where each of them is built and run
//! ([`run`]), where a worktree's git directories are and how a repository is found ([`layout`]),
//! what a repository's other administrative files say ([`repo_files`]), the branch reads and
//! worktree changes behind sessions' branches ([`branch_git`]), the reads isolated from the user's
//! environment ([`isolated`]) and the parsed list of a repository's worktrees ([`worktree_list`]).

pub mod branch_git;
pub mod env;
pub mod isolated;
pub mod layout;
pub mod repo_files;
pub mod run;
pub mod worktree_list;
