//! The settings every `git` the daemon runs in a user's repository or against a user's remote
//! shares, and the reasons for them.
//!
//! **Non-interactive** ([`non_interactive`]). The daemon has no terminal anyone watches, yet `git`
//! and `ssh` read a missing credential, an unknown host key or a passphrase-protected key with no
//! agent from the controlling terminal, not stdin, so a null stdin does nothing to stop the prompt
//! and a daemon launched from a terminal would block forever on it. `GIT_TERMINAL_PROMPT=0` and
//! `GIT_SSH_COMMAND=ssh -oBatchMode=yes` turn all three into an immediate failure, and the
//! askpass helpers are removed so no graphical prompt takes their place.
//!
//! **Read-only** ([`read_only`]). A read is not read-only on its own: `git status` refreshes the
//! stat information of a tracked file whose timestamps moved while its content did not, and
//! rewrites `.git/index` to keep it, holding `index.lock` while it does — which fails the `git add`
//! or `git commit` an agent is running in the same repository. `GIT_OPTIONAL_LOCKS=0` drops that
//! write-back without changing anything the read reports. A diff-family command refreshes the index
//! whatever `GIT_OPTIONAL_LOCKS` says, so `-c diff.autoRefreshIndex=false` is the same guarantee
//! for it. Both go on every read, so one added later cannot take half of the guarantee. A command
//! that writes to the repository does not get them: it takes the locks it needs by design.

use std::ffi::OsStr;
use std::process::Command;

/// A `git` command that cannot prompt, running `git` with exactly `env` as its environment.
///
/// `env` is the whole environment: nothing of the daemon's own leaks in, so a caller that wants
/// variables dropped drops them from what it passes. The prompt settings are applied last and
/// win over anything `env` says.
pub fn non_interactive<K, V>(
    git: impl AsRef<OsStr>,
    env: impl IntoIterator<Item = (K, V)>,
) -> Command
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = Command::new(git);
    command.env_clear();
    command.envs(env);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    // After `env_clear`, a removal drops the key outright rather than marking it as removed, so
    // these two leave nothing behind for `envs` above to have inserted.
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");
    command
}

/// Keeps `command` from writing to the index of the repository it reads. The `-c` option is an
/// argument, so this must be called before the subcommand is added.
pub fn read_only(command: &mut Command) {
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command.args(["-c", "diff.autoRefreshIndex=false"]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsString;

    fn env_of(command: &Command) -> HashMap<String, Option<OsString>> {
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(OsStr::to_owned),
                )
            })
            .collect()
    }

    #[test]
    fn non_interactive_overrides_and_removes_prompt_settings() {
        let env = HashMap::from([
            ("GIT_TERMINAL_PROMPT".to_string(), "1".to_string()),
            ("GIT_ASKPASS".to_string(), "helper".to_string()),
            ("SSH_ASKPASS".to_string(), "helper".to_string()),
            ("KEPT".to_string(), "yes".to_string()),
        ]);
        let command = non_interactive("git", &env);
        let set = env_of(&command);
        assert_eq!(set["GIT_TERMINAL_PROMPT"].as_deref(), Some(OsStr::new("0")));
        assert_eq!(
            set["GIT_SSH_COMMAND"].as_deref(),
            Some(OsStr::new("ssh -oBatchMode=yes"))
        );
        assert!(!set.contains_key("GIT_ASKPASS"));
        assert!(!set.contains_key("SSH_ASKPASS"));
        assert_eq!(set["KEPT"].as_deref(), Some(OsStr::new("yes")));
        assert!(!set.contains_key("GIT_OPTIONAL_LOCKS"));
        assert_eq!(command.get_args().count(), 0);
    }

    #[test]
    fn read_only_sets_both_index_guards() {
        let mut command = non_interactive("git", &HashMap::<String, String>::new());
        read_only(&mut command);
        assert_eq!(
            env_of(&command)["GIT_OPTIONAL_LOCKS"].as_deref(),
            Some(OsStr::new("0"))
        );
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(args, ["-c", "diff.autoRefreshIndex=false"]);
    }
}
