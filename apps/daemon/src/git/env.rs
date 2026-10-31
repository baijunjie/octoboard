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
//!
//! **User configuration only** ([`user_configuration_only`]). `git` runs in the user's own shell
//! environment, so that it is their `git`, with their configuration and credentials; but a `GIT_*`
//! variable they exported for their own shell work can also change what a command looks at. A
//! `GIT_DIR` sends a status check to another repository and its fetch to that repository's
//! remotes, and what the check reads is written to the database and broadcast, not shown for a
//! moment. So every `GIT_*` variable is dropped except the few that say which of the user's own
//! material to use and how to reach a remote. Too wide a list reads another repository; too narrow
//! a one fails, for good and with nothing in the interface pointing at the cause, a fetch that
//! works from the user's shell (a `GIT_SSL_CAINFO` behind a TLS-intercepting proxy, say) and
//! leaves its message in the project's status for every later check.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::process::Command;

/// The `GIT_*` variables kept from the user's environment by [`user_configuration_only`]: they
/// say which of the user's own material to use, and how to reach a remote with it — their
/// configuration files, the certificates, keys and TLS settings a connection uses, the proxy
/// helper — and dropping one would read some other configuration, or fail a fetch that works from
/// their shell. A variable that can change which repository a command looks at, set any
/// configuration item (`GIT_CONFIG_COUNT` and its keys and values, `GIT_CONFIG_PARAMETERS`), or
/// weaken verification (`GIT_SSL_NO_VERIFY`) is not kept.
pub const KEPT_GIT_VARIABLES: &[&str] = &[
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_SSL_CAINFO",
    "GIT_SSL_CAPATH",
    "GIT_SSL_CERT",
    "GIT_SSL_KEY",
    "GIT_SSL_CERT_TYPE",
    "GIT_SSL_KEY_TYPE",
    "GIT_SSL_VERSION",
    "GIT_SSL_CIPHER_LIST",
    "GIT_PROXY_COMMAND",
    "GIT_HTTP_PROXY_AUTHMETHOD",
    "GIT_PROXY_SSL_CAINFO",
    "GIT_PROXY_SSL_CERT",
    "GIT_PROXY_SSL_KEY",
];

/// `env` without its `GIT_*` variables other than [`KEPT_GIT_VARIABLES`], for a command, read or
/// write, that works on the repository its working directory names. A `GIT_DIR` or a
/// `GIT_CONFIG_COUNT` exported in the user's login shell would otherwise send it to another
/// repository, or to that repository's configuration.
pub fn user_configuration_only(
    env: &HashMap<String, String>,
) -> impl Iterator<Item = (&String, &String)> {
    env.iter()
        .filter(|(key, _)| !key.starts_with("GIT_") || KEPT_GIT_VARIABLES.contains(&key.as_str()))
}

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
    use std::path::Path;

    use crate::test_support::redirecting_overrides;

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
    fn user_configuration_only_keeps_what_names_the_users_material_and_drops_the_rest() {
        let mut env: HashMap<String, String> = KEPT_GIT_VARIABLES
            .iter()
            .map(|key| (key.to_string(), "kept".to_string()))
            .collect();
        // Everything the shared redirect fixture sets, plus what changes which objects or refs a
        // command sees and what weakens verification, which no fixture read shows.
        let dropped = redirecting_overrides(Path::new("/theirs"))
            .into_iter()
            .map(|(key, _)| key)
            .chain([
                "GIT_OBJECT_DIRECTORY",
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                "GIT_NAMESPACE",
                "GIT_SSL_NO_VERIFY",
            ]);
        for key in dropped {
            env.insert(key.to_string(), "dropped".to_string());
        }
        env.insert("HOME".to_string(), "/home/u".to_string());
        let command = non_interactive("git", user_configuration_only(&env));
        let set = env_of(&command);
        for key in KEPT_GIT_VARIABLES {
            assert_eq!(set[*key].as_deref(), Some(OsStr::new("kept")), "{key}");
        }
        assert_eq!(set["HOME"].as_deref(), Some(OsStr::new("/home/u")));
        for key in set.keys().filter(|key| key.starts_with("GIT_")) {
            assert!(
                KEPT_GIT_VARIABLES.contains(&key.as_str())
                    || ["GIT_TERMINAL_PROMPT", "GIT_SSH_COMMAND"].contains(&key.as_str()),
                "{key} reached git"
            );
        }
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
