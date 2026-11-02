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
//! **The user's ssh** ([`reach_remote`]). `GIT_SSH_COMMAND` in the environment wins over the
//! user's own choice of ssh, so a command that may reach a remote (a fetch, a push, a clone) gets
//! that choice back, with `-oBatchMode=yes` put in: the ssh `git` itself would pick — the user's
//! own `GIT_SSH_COMMAND`, else `core.sshCommand` as the configuration the command runs under gives
//! it, else the user's `GIT_SSH`, else `ssh`. Without it, a `core.sshCommand = ssh -i
//! ~/.ssh/id_work` that a multi-account setup relies on gives way to the plain `ssh`, and a fetch
//! that works from the user's shell fails here, or authenticates as another account. The option
//! goes in only where the program the command starts is `ssh` itself, right after its name. A
//! command starting anything else — `plink`, a wrapper script, a variable assignment ahead of the
//! program — is left as the user wrote it, since it may not take the option: a prompt such a
//! program opens is not ruled out, and the run's deadline is what ends it. A configuration that
//! could not be read leaves the plain `ssh -oBatchMode=yes`. A command that reaches no remote
//! keeps the plain one too, and runs no read of the configuration for it; a command that only
//! may start an ssh — a fast-forward or a switch in a partial clone, which fetch the objects they
//! lack from the promisor remote — is built as one that reaches a remote.
//!
//! A clone reads no repository's configuration, so its `core.sshCommand` comes from the system and
//! global configuration, read from `/`. One that only an `includeIf "gitdir:"` or `hasconfig:`
//! conditional include supplies for the new repository is not seen there, and the clone does not
//! use it; the user can set `GIT_SSH_COMMAND`, or a global `core.sshCommand`, to have it used.
//!
//! **No pager** ([`BASE_OPTIONS`]). Every command `crate::git::run` builds, read or write, starts
//! with `--no-pager`, ahead of its family's own options. It changes nothing while a run's output is
//! a pipe, since `git` pages only to a terminal; it is there so that no run can ever wait on a
//! pager.
//!
//! **Read-only** ([`read_only`]). A read is not read-only on its own: `git status` refreshes the
//! stat information of a tracked file whose timestamps moved while its content did not, and
//! rewrites `.git/index` to keep it, holding `index.lock` while it does — which fails the `git add`
//! or `git commit` an agent is running in the same repository. `GIT_OPTIONAL_LOCKS=0` drops that
//! write-back without changing anything the read reports. A diff-family command refreshes the index
//! whatever `GIT_OPTIONAL_LOCKS` says, so `-c diff.autoRefreshIndex=false` is the same guarantee
//! for it.
//!
//! A read also runs without a configured filesystem monitor (`-c core.fsmonitor=false`). A monitor
//! is a program `git` starts from the configuration on anything that refreshes the index — a hook
//! script run each time, or with `core.fsmonitor=true` `git`'s own monitor daemon, which it starts
//! in the background and leaves running after the command ends. The daemon reads repositories on
//! repeating paths (a status check per project, the Git mode's listings), so a monitor would mean a
//! program started per read or a watcher left behind per worktree read, and a read that starts one
//! is no longer a read that leaves the repository alone. What it costs is speed in a very large
//! worktree, which `git` then scans itself within the run's deadline; nothing a read reports
//! changes.
//!
//! All three go on every read, so one added later cannot take part of the guarantee. A command
//! that writes to the repository does not get them: it takes the locks it needs by design, and it
//! keeps the user's monitor, because a `-c` reaches the hooks a command runs (through
//! `GIT_CONFIG_PARAMETERS`), and a write runs the user's hooks — a push its `pre-push` hook, a
//! fast-forward its `post-merge` hook, a switch its `post-checkout` hook — which must see the
//! configuration the user gave them. A family whose own writes should not start a monitor either
//! says so in its `Policy`, beside its users, as the branch cleanup does for the worktrees it
//! removes and the restore for the files it rewrites.
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

/// The `GIT_*` variables kept from the user's environment by [`user_configuration_only`]: they say
/// which of the user's own material to use, and how to reach a remote with it — their configuration
/// files, the certificates, keys and TLS settings a connection uses, the proxy helper, the variant
/// a `GIT_SSH` wrapper is told apart by — and dropping one would read some other configuration, or
/// fail a fetch that works from their shell. A variable that can change which repository a command
/// looks at, set any configuration item (`GIT_CONFIG_COUNT` and its keys and values,
/// `GIT_CONFIG_PARAMETERS`), or weaken verification (`GIT_SSL_NO_VERIFY`) is not kept.
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
    "GIT_SSH_VARIANT",
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

/// The options every command starts with, ahead of its family's own: see the module header.
pub const BASE_OPTIONS: &[&str] = &["--no-pager"];

/// The ssh option that turns a prompt into an immediate failure.
const BATCH_MODE: &str = "-oBatchMode=yes";

/// What the configuration a command that may reach a remote runs under says of `core.sshCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshConfiguration {
    /// Not set, or set empty.
    Unset,
    /// Set to this command line.
    Command(String),
    /// The configuration could not be read.
    Unknown,
}

impl SshConfiguration {
    /// The configuration whose `core.sshCommand` reads `value`, `None` where it sets none.
    pub fn of(value: Option<String>) -> Self {
        match value {
            Some(command) if !command.is_empty() => Self::Command(command),
            _ => Self::Unset,
        }
    }
}

/// Whether `user_env` leaves the ssh to `core.sshCommand`: it does not where the user's own
/// `GIT_SSH_COMMAND` is set, which wins over it, so no read of the configuration is needed.
pub fn ssh_configuration_matters(user_env: &HashMap<String, String>) -> bool {
    !user_env
        .get("GIT_SSH_COMMAND")
        .is_some_and(|command| !command.is_empty())
}

/// Gives `command`, built by [`non_interactive`] for a `git` that may reach a remote, the ssh `git`
/// would pick from `user_env`, the user's unfiltered environment, and `configured`, with
/// [`BATCH_MODE`] put in where that ssh is `ssh` itself. See the module header.
pub fn reach_remote(
    command: &mut Command,
    user_env: &HashMap<String, String>,
    configured: &SshConfiguration,
) {
    let set = |key: &str| user_env.get(key).filter(|value| !value.is_empty());
    if let Some(user) = set("GIT_SSH_COMMAND") {
        command.env("GIT_SSH_COMMAND", with_batch_mode(user));
        return;
    }
    match configured {
        SshConfiguration::Command(configured) => {
            command.env("GIT_SSH_COMMAND", with_batch_mode(configured));
        }
        // The plain ssh [`non_interactive`] set stays.
        SshConfiguration::Unknown => {}
        SshConfiguration::Unset => {
            let Some(program) = set("GIT_SSH") else {
                return;
            };
            if basename(program) == "ssh" {
                command.env(
                    "GIT_SSH_COMMAND",
                    format!("{} {BATCH_MODE}", shell_quote(program)),
                );
            } else {
                // Nothing in the configuration to come before it, so `git` runs the program.
                command.env_remove("GIT_SSH_COMMAND");
                command.env("GIT_SSH", program);
            }
        }
    }
}

/// `command_line` with [`BATCH_MODE`] right after its program word where that program is `ssh`,
/// and as it is otherwise: put there rather than at the end, it stays an option of `ssh` whatever
/// the rest of the line holds.
fn with_batch_mode(command_line: &str) -> String {
    match program_word(command_line) {
        Some((program, end)) if basename(&program) == "ssh" => format!(
            "{} {BATCH_MODE}{}",
            &command_line[..end],
            &command_line[end..]
        ),
        _ => command_line.to_string(),
    }
}

/// The first word of `command_line`, with its quotes and backslashes taken off as `git` does when
/// it looks at the program to tell which ssh it is, and the byte offset where the word ends; none
/// for a line with no word, or whose first word leaves a quote or a backslash unfinished.
fn program_word(command_line: &str) -> Option<(String, usize)> {
    let start = command_line.len()
        - command_line
            .trim_start_matches(|c: char| c.is_ascii_whitespace())
            .len();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command_line[start..].char_indices();
    while let Some((at, c)) = chars.next() {
        match (quote, c) {
            (None, c) if c.is_ascii_whitespace() => return Some((word, start + at)),
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (Some('\''), c) => word.push(c),
            (_, '\\') => word.push(chars.next()?.1),
            (_, c) => word.push(c),
        }
    }
    (quote.is_none() && !word.is_empty()).then_some((word, command_line.len()))
}

/// The last component of `path`.
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `value` as one word of a POSIX shell command line.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Keeps `command` from writing to the index of the repository it reads, and from starting a
/// configured filesystem monitor. The `-c` options are arguments, so this must be called before the
/// subcommand is added.
pub fn read_only(command: &mut Command) {
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command.args([
        "-c",
        "core.fsmonitor=false",
        "-c",
        "diff.autoRefreshIndex=false",
    ]);
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

    /// A `GIT_SSH` wrapper is run as it is, and tells git how to call it with `GIT_SSH_VARIANT`.
    #[test]
    fn a_git_ssh_wrapper_keeps_its_variant() {
        let env = user_env(&[("GIT_SSH", "/opt/wrap"), ("GIT_SSH_VARIANT", "ssh")]);
        let mut command = non_interactive("git", user_configuration_only(&env));
        reach_remote(&mut command, &env, &SshConfiguration::Unset);
        let set = env_of(&command);
        assert_eq!(set["GIT_SSH"].as_deref(), Some(OsStr::new("/opt/wrap")));
        assert_eq!(set["GIT_SSH_VARIANT"].as_deref(), Some(OsStr::new("ssh")));
    }

    fn user_env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    /// The ssh variables a remote command gets from `env` and `configured`: `GIT_SSH_COMMAND`, and
    /// `GIT_SSH`, each `None` where it is not passed at all.
    fn remote_ssh(
        env: &[(&str, &str)],
        configured: &SshConfiguration,
    ) -> (Option<String>, Option<String>) {
        let env = user_env(env);
        let mut command = non_interactive("git", user_configuration_only(&env));
        reach_remote(&mut command, &env, configured);
        let set = env_of(&command);
        let value = |key: &str| {
            set.get(key)
                .cloned()
                .flatten()
                .map(|value| value.to_string_lossy().into_owned())
        };
        (value("GIT_SSH_COMMAND"), value("GIT_SSH"))
    }

    fn command(text: &str) -> SshConfiguration {
        SshConfiguration::Command(text.to_string())
    }

    #[test]
    fn the_users_own_ssh_command_wins_and_cannot_prompt() {
        assert_eq!(
            remote_ssh(
                &[
                    ("GIT_SSH_COMMAND", "ssh -i ~/.ssh/id_work"),
                    ("GIT_SSH", "/bin/x")
                ],
                &command("ssh -i configured"),
            ),
            (
                Some("ssh -oBatchMode=yes -i ~/.ssh/id_work".to_string()),
                None
            )
        );
        assert!(!ssh_configuration_matters(&user_env(&[(
            "GIT_SSH_COMMAND",
            "ssh"
        )])));
        assert!(ssh_configuration_matters(&user_env(&[(
            "GIT_SSH_COMMAND",
            ""
        )])));
    }

    #[test]
    fn the_configured_ssh_command_comes_next() {
        assert_eq!(
            remote_ssh(&[("GIT_SSH", "/bin/x")], &command("/usr/bin/ssh -i k")),
            (Some("/usr/bin/ssh -oBatchMode=yes -i k".to_string()), None)
        );
        // The option goes after the program word, so what follows it stays the user's.
        assert_eq!(
            remote_ssh(&[], &command("  \"/opt/my tools/ssh\" -F cfg; true")),
            (
                Some("  \"/opt/my tools/ssh\" -oBatchMode=yes -F cfg; true".to_string()),
                None
            )
        );
        assert_eq!(
            remote_ssh(&[], &SshConfiguration::Unset),
            (Some("ssh -oBatchMode=yes".to_string()), None)
        );
    }

    #[test]
    fn a_program_that_is_not_ssh_is_left_as_the_user_wrote_it() {
        for line in [
            "plink -batch",
            "/home/u/bin/ssh-work",
            "FOO=1 ssh -i k",
            "\"ssh",
            "sshpass -e ssh",
        ] {
            assert_eq!(
                remote_ssh(&[], &command(line)),
                (Some(line.to_string()), None),
                "{line}"
            );
        }
    }

    #[test]
    fn the_users_git_ssh_comes_last() {
        assert_eq!(
            remote_ssh(&[("GIT_SSH", "/opt/it's/ssh")], &SshConfiguration::Unset),
            (
                Some(r"'/opt/it'\''s/ssh' -oBatchMode=yes".to_string()),
                None
            )
        );
        // Another program is run as it is, through `GIT_SSH`, with nothing ahead of it.
        assert_eq!(
            remote_ssh(&[("GIT_SSH", "/usr/bin/plink")], &SshConfiguration::Unset),
            (None, Some("/usr/bin/plink".to_string()))
        );
    }

    #[test]
    fn a_configuration_that_could_not_be_read_keeps_the_plain_ssh() {
        assert_eq!(
            remote_ssh(&[("GIT_SSH", "/usr/bin/plink")], &SshConfiguration::Unknown),
            (Some("ssh -oBatchMode=yes".to_string()), None)
        );
    }

    #[test]
    fn read_only_sets_the_index_guards_and_turns_the_monitor_off() {
        let mut command = non_interactive("git", &HashMap::<String, String>::new());
        read_only(&mut command);
        assert_eq!(
            env_of(&command)["GIT_OPTIONAL_LOCKS"].as_deref(),
            Some(OsStr::new("0"))
        );
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(
            args,
            [
                "-c",
                "core.fsmonitor=false",
                "-c",
                "diff.autoRefreshIndex=false"
            ]
        );
    }
}
