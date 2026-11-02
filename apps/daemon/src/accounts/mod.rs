//! The agent accounts: how two account names are compared (`account_name_key`, and
//! `clears_default_account_name` for the default account's), where each keeps its configuration,
//! signing one in, its plan quota, moving a removed one's directory to the Trash, sharing the
//! default account's configuration, and switching a session to another account (copying its
//! conversation over, carrying on with its turn, and doing so on its own when a turn stops on the
//! usage limit), with what keeps Codex's thread database in step with where its rollouts are.

pub(crate) mod auto_switch;
pub(crate) mod cli_run;
pub(crate) mod codex_threads;
pub(crate) mod dirs;
pub(crate) mod files;
pub(crate) mod login;
pub(crate) mod quotas;
pub(crate) mod relocate;
pub(crate) mod shared_config;
pub(crate) mod switch_continue;
pub(crate) mod trash;

/// The comparison key for an account name collision: trimmed and lower-cased, so "Work" and
/// " work " read as the same name.
pub(crate) fn account_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Whether `name`, given to a default account, clears its name instead: it is blank, or the word
/// shown for an unnamed default account.
pub(crate) fn clears_default_account_name(name: &str) -> bool {
    let key = account_name_key(name);
    key.is_empty() || key == account_name_key(crate::protocol::DEFAULT_ACCOUNT_NAME)
}
