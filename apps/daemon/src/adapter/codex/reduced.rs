//! What a reduced Codex launch leaves out after the enhanced one exited as it started
//! (`docs/plans/terminal-first-degradation.md`, D7), read from what Codex wrote as it exited.
//!
//! Measured on 0.167.1 (`testdata/codex-exit-captures.md`; `docs/agent-cli-reference.md`, "What
//! Codex does with an option or a setting it does not take"): an option it does not take is refused
//! at argument parsing, before anything is drawn, with `error: unexpected argument '<arg>' found`;
//! a value of a setting it does not know is refused while the configuration loads, also before
//! anything is drawn, with `unknown variant …` and a line `in `<key>`` naming the whole key path; a
//! profile `default_permissions` names that it does not know is refused only once the terminal is
//! set up, with `Error loading configuration: default_permissions refers to unknown built-in
//! profile …`, still before anything is drawn; a key it does not know at all is passed over without
//! a word. So a refusal that names a key Octoboard set leaves out exactly that key's override (the
//! three permission keys together, since they make one preset), and anything else leaves out what a
//! release may not take as a whole: `--dangerously-bypass-hook-trust` with the hooks it lets
//! through (without the flag Codex would raise its hook review for them), and the private server
//! and its `--remote` terminal. Which release introduced either is not known, so neither has a
//! floor.

use super::private_server::PERMISSION_KEYS;
use crate::adapter::EarlyExit;
use crate::protocol::{Code, Notice};
use crate::providers::codex;

/// What a reduced launch leaves out of the enhanced one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LeftOut {
    /// The hook-trust bypass, the hooks and the private server.
    Hooks,
    /// The override of the configuration key Codex refused the value of.
    Setting(String),
    /// The permission preset's three overrides, one of whose keys Codex refused.
    Permissions(String),
}

impl LeftOut {
    /// What the reduced launch after `exit` leaves out.
    pub(super) fn after(exit: &EarlyExit) -> Self {
        match refused_key(&exit.output) {
            Some(key) if PERMISSION_KEYS.contains(&key) => Self::Permissions(key.to_string()),
            Some(key) => Self::Setting(key.to_string()),
            None => Self::Hooks,
        }
    }

    /// The configuration keys whose overrides the launch leaves out.
    pub(super) fn keys(&self) -> Vec<&str> {
        match self {
            Self::Hooks => Vec::new(),
            Self::Setting(key) => vec![key.as_str()],
            Self::Permissions(_) => PERMISSION_KEYS.to_vec(),
        }
    }

    /// The standing notice of what the launch leaves out.
    pub(super) fn notice(&self) -> Notice {
        match self {
            Self::Hooks => Notice::new(
                Code::of_provider(codex::HOOKS_LEFT_OUT),
                "Codex exited as it started, so it was started again without Octoboard's hooks \
                 and its message queue. This session's status follows what its terminal reports, \
                 edits to another project's files are not blocked, and its prompts can be \
                 answered only in its terminal.",
                &[],
            ),
            Self::Setting(key) => Notice::new(
                Code::of_provider(codex::SETTING_LEFT_OUT),
                format!(
                    "Codex exited as it started, refusing the value Octoboard sets for `{key}`, \
                     so it was started again without it."
                ),
                &[("key", key)],
            ),
            Self::Permissions(key) => Notice::new(
                Code::of_provider(codex::PERMISSIONS_LEFT_OUT),
                format!(
                    "Codex exited as it started, refusing the value Octoboard sets for `{key}`, \
                     so it was started again without the permission approval chosen in \
                     Octoboard. Codex's own configuration decides what it may do."
                ),
                &[("key", key)],
            ),
        }
        .standing()
    }
}

/// The key a refusal of a setting's value names: the `in `<key>`` line that follows Codex's
/// `unknown variant` error, or the key that starts `Error loading configuration: <key> refers to
/// unknown …`, as a profile `default_permissions` names and Codex does not know is refused.
/// `None` for any other output.
fn refused_key(output: &str) -> Option<&str> {
    let is_key = |key: &str| !key.is_empty() && !key.contains(char::is_whitespace);
    if output.contains("unknown variant") {
        let named = output.lines().find_map(|line| {
            let key = line.trim().strip_prefix("in `")?.strip_suffix('`')?;
            is_key(key).then_some(key)
        });
        if named.is_some() {
            return named;
        }
    }
    output.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(CONFIGURATION_ERROR)?;
        let (key, _) = rest.split_once(REFERS_TO_UNKNOWN)?;
        is_key(key).then_some(key)
    })
}

/// How Codex starts a refusal found once its configuration has been read (0.167.1).
const CONFIGURATION_ERROR: &str = "Error loading configuration: ";
/// What follows the key in such a refusal of a value that names something Codex does not know.
const REFERS_TO_UNKNOWN: &str = " refers to unknown ";
