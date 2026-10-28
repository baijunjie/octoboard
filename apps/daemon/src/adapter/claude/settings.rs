//! The Claude Code settings files a launch reads from, and what is read out of them: the user's
//! and the project's settings files with the `statusLine` they give, the `allowedHttpHookUrls`
//! that decide between HTTP and command hooks, and the trust state in the global config file. Read,
//! never written: Octoboard does not edit any of these files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::adapter::LaunchSpec;
use crate::protocol::{notice_code, Notice};

/// The configuration directory in effect for this launch's Claude Code, when there is one: the
/// session's, else one the user's own shell exports.
fn explicit_config_dir(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
) -> Option<PathBuf> {
    pinned.map(PathBuf::from).or_else(|| {
        shell_env
            .get("CLAUDE_CONFIG_DIR")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
    })
}

/// The home directory the launch's shell snapshot carries, else the daemon's own.
fn home_dir(shell_env: &HashMap<String, String>) -> PathBuf {
    shell_env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(crate::paths::home_dir)
}

/// The configuration directory this launch's Claude Code reads its user settings from: the one in
/// effect, else `~/.claude`.
pub(super) fn user_config_dir(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> PathBuf {
    explicit_config_dir(spec.shell_env, pinned)
        .unwrap_or_else(|| home_dir(spec.shell_env).join(".claude"))
}

/// The settings files the user and the project write, lowest precedence first: the user's in
/// `user_dir`, then the project's shared and local ones under `cwd`. Claude Code's flags and its
/// managed settings outrank all three.
fn own_settings_files(user_dir: &Path, cwd: &Path) -> [PathBuf; 3] {
    [
        user_dir.join("settings.json"),
        cwd.join(".claude/settings.json"),
        cwd.join(".claude/settings.local.json"),
    ]
}

/// The `statusLine` the user's and the project's settings files give, without Octoboard's: each
/// file's object merged over the one below it key by key, as Claude Code merges its settings. A
/// file that cannot be read or parsed, or holds no object there, counts for nothing; `None` when no
/// file has one.
pub fn user_status_line(
    user_dir: &Path,
    cwd: &Path,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut merged: Option<serde_json::Map<String, serde_json::Value>> = None;
    for file in own_settings_files(user_dir, cwd) {
        let Some(serde_json::Value::Object(line)) = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|mut settings| settings.get_mut("statusLine").map(std::mem::take))
        else {
            continue;
        };
        merged.get_or_insert_with(Default::default).extend(line);
    }
    merged
}

/// The settings files whose `allowedHttpHookUrls` this launch's Claude Code would merge: the
/// user's, the project's two, and the managed file and drop-in directory. Policy delivered through
/// an MDM profile or the claude.ai console is in none of them and cannot be read from here.
pub(super) fn settings_files(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> Vec<PathBuf> {
    let managed = Path::new(if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode"
    } else {
        "/etc/claude-code"
    });
    let mut files = own_settings_files(&user_config_dir(spec, pinned), spec.cwd).to_vec();
    files.push(managed.join("managed-settings.json"));
    if let Ok(entries) = std::fs::read_dir(managed.join("managed-settings.d")) {
        files.extend(entries.flatten().map(|entry| entry.path()));
    }
    files
}

/// Whether Claude Code would run an HTTP hook for `url` given `files`: it does unless some file
/// defines `allowedHttpHookUrls`, whose arrays merge across files, and no merged pattern matches.
/// A pattern's `*` stands for anything; a file that cannot be read or parsed counts for nothing.
pub(super) fn http_hooks_allowed(url: &str, files: &[PathBuf]) -> bool {
    let mut patterns: Option<Vec<String>> = None;
    for file in files {
        let Some(settings) = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        else {
            continue;
        };
        if let Some(list) = settings
            .get("allowedHttpHookUrls")
            .and_then(serde_json::Value::as_array)
        {
            patterns.get_or_insert_with(Vec::new).extend(
                list.iter()
                    .filter_map(|p| p.as_str().map(str::to_lowercase)),
            );
        }
    }
    let url = url.to_lowercase();
    patterns.is_none_or(|patterns| {
        patterns
            .iter()
            .any(|pattern| wildcard_matches(pattern, &url))
    })
}

/// Whether `text` matches `pattern`, in which `*` stands for any run of characters.
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let mut parts = parts.peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return rest.ends_with(part);
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.is_empty()
}

/// The global config file this launch's Claude Code will read: inside the configuration directory
/// when one is in effect — the session's, else one the user's own shell exports — and
/// `~/.claude.json` otherwise. `shell_env` is the launch's shell snapshot.
pub(in crate::adapter) fn global_config_file(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
) -> PathBuf {
    match explicit_config_dir(shell_env, pinned) {
        Some(dir) => dir.join(".claude.json"),
        None => home_dir(shell_env).join(".claude.json"),
    }
}

/// The entry of the global config file's `projects` map for exactly the directory `dir`.
pub(in crate::adapter) fn project_entry<'a>(
    config: &'a serde_json::Value,
    dir: &Path,
) -> Option<&'a serde_json::Value> {
    config.get("projects")?.get(dir.to_string_lossy().as_ref())
}

/// What to tell the user when Claude Code will ignore this project's own `allow` rules.
///
/// Deliberately silent unless the trust state is explicitly negative: the field is only read, so
/// an absent project entry or a renamed key must leave the user alone rather than warn them on
/// every launch about something that may not be true.
///
/// A directory's own entry saying `false` is not enough on its own. Claude Code creates that entry
/// with `false` the first time it runs in a directory, and also counts a directory as trusted when
/// a directory above it carries an accepted trust flag — so a directory under a trusted one never
/// shows the trust screen and keeps its `false` (read out of Claude Code 2.1.294). Claude Code
/// stops that walk at the repository root inside a git repository; it is not stopped here, which
/// can only leave a notice out, the side this notice errs on.
pub(super) fn untrusted_workspace_notice(
    spec: &LaunchSpec<'_>,
    pinned: Option<&Path>,
) -> Option<Notice> {
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(global_config_file(spec.shell_env, pinned)).ok()?,
    )
    .ok()?;

    let canonical = std::fs::canonicalize(spec.cwd).unwrap_or_else(|_| spec.cwd.to_path_buf());
    let accepted = |dir: &Path| {
        project_entry(&config, dir)
            .and_then(|entry| entry.get("hasTrustDialogAccepted"))
            .and_then(serde_json::Value::as_bool)
    };
    if canonical
        .ancestors()
        .skip(1)
        .any(|dir| accepted(dir) == Some(true))
    {
        return None;
    }
    match accepted(&canonical) {
        Some(false) => Some(Notice::new(
            notice_code::CLAUDE_WORKSPACE_UNTRUSTED,
            format!(
                "Claude Code has not been trusted with this directory, so this project's own `allow` \
                 permission rules are ignored until Claude Code's trust prompt is answered — its `deny` \
                 rules still apply, so a session is only more restrictive, never less. {} \
                 answers that prompt for you once you have agreed to it.",
                crate::APP_NAME
            ),
            &[],
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::claude::ClaudeAdapter;
    use crate::adapter::tests::spec_fixture;
    use crate::adapter::AgentAdapter;

    /// Reading the trust state wrong must not nag: only an explicit `false` warns, so an absent
    /// project entry or a key that was renamed upstream leaves the user alone.
    #[test]
    fn only_an_explicitly_untrusted_workspace_produces_a_notice() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        std::fs::create_dir_all(&home).expect("home");
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        let config = home.join(".claude.json");

        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());

        std::fs::write(&config, r#"{"projects":{}}"#).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());

        let untrusted = serde_json::json!({
            "projects": { canonical.to_string_lossy(): { "hasTrustDialogAccepted": false } }
        });
        std::fs::write(&config, untrusted.to_string()).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_some());

        let trusted = serde_json::json!({
            "projects": { canonical.to_string_lossy(): { "hasTrustDialogAccepted": true } }
        });
        std::fs::write(&config, trusted.to_string()).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());

        // Claude Code trusts a directory below a trusted one without asking, and leaves the
        // directory's own entry at `false`.
        let under_trusted = serde_json::json!({
            "projects": {
                canonical.to_string_lossy(): { "hasTrustDialogAccepted": false },
                canonical.parent().expect("a parent").to_string_lossy(): { "hasTrustDialogAccepted": true },
            }
        });
        std::fs::write(&config, under_trusted.to_string()).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());
    }

    /// The trust state lives in the config file Claude Code will actually read, which moves into the
    /// configuration directory when there is one: the session's, or failing that the shell's.
    #[test]
    fn the_trust_state_is_read_from_the_file_inside_the_config_dir() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        let pinned = fixture.scratch.join("pinned");
        let from_shell = fixture.scratch.join("from-shell");
        for dir in [&home, &pinned, &from_shell] {
            std::fs::create_dir_all(dir).expect("directory");
        }
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        let write = |dir: &std::path::Path, trusted: bool| {
            let config = serde_json::json!({
                "projects": { canonical.to_string_lossy(): { "hasTrustDialogAccepted": trusted } }
            });
            std::fs::write(dir.join(".claude.json"), config.to_string()).expect("config");
        };
        let warns = |fixture: &crate::adapter::tests::SpecFixture| {
            ClaudeAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .notice
                .is_some()
        };

        // Trusted in the default file, untrusted in the pinned directory's: only the latter counts.
        write(&home, true);
        write(&pinned, false);
        assert!(!warns(&fixture));
        fixture.config_dir = Some(pinned.clone());
        assert!(warns(&fixture));

        // The pinned directory has no config file yet: silent, and the default file is not a
        // fallback because Claude Code would not read it either.
        std::fs::remove_file(pinned.join(".claude.json")).expect("removed");
        write(&home, false);
        assert!(!warns(&fixture));

        // A directory the user's own shell exports is followed when the console pins nothing, and
        // the console's wins when it does.
        fixture.config_dir = None;
        fixture.shell_env.insert(
            "CLAUDE_CONFIG_DIR".to_string(),
            from_shell.to_string_lossy().into_owned(),
        );
        write(&from_shell, false);
        assert!(warns(&fixture));
        write(&from_shell, true);
        assert!(!warns(&fixture));
        write(&pinned, false);
        fixture.config_dir = Some(pinned);
        assert!(warns(&fixture));
    }

    /// An `allowedHttpHookUrls` that leaves the daemon's URL out keeps the command form, whichever
    /// file defines it.
    #[test]
    fn an_http_hook_allowlist_without_the_daemon_keeps_the_command_form() {
        let url = "http://127.0.0.1:4321/hook/session-1";
        let dir = crate::test_support::ScratchDir::new("hook-allowlist");
        let file = |name: &str, content: &str| {
            let path = dir.join(name);
            std::fs::write(&path, content).expect("settings");
            path
        };
        let none = file("none.json", "{}");
        let broken = file("broken.json", "{");
        let other = file(
            "other.json",
            r#"{"allowedHttpHookUrls":["https://hooks.example.com/*"]}"#,
        );
        let loopback = file(
            "loopback.json",
            r#"{"allowedHttpHookUrls":["http://127.0.0.1:*"]}"#,
        );
        let empty = file("empty.json", r#"{"allowedHttpHookUrls":[]}"#);
        let missing = dir.join("missing.json");

        assert!(http_hooks_allowed(url, &[none.clone(), broken, missing]));
        assert!(!http_hooks_allowed(url, std::slice::from_ref(&other)));
        assert!(!http_hooks_allowed(url, &[empty]));
        assert!(http_hooks_allowed(url, std::slice::from_ref(&loopback)));
        // The arrays merge, so one file naming the daemon is enough.
        assert!(http_hooks_allowed(url, &[other, none, loopback]));
        assert!(wildcard_matches("http://*/hook/*", url));
        assert!(!wildcard_matches("http://*/hook", url));
    }
}
