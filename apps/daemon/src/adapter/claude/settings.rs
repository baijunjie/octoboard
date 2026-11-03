//! The Claude Code settings files a launch reads from, and what is read out of them: the user's
//! and the project's settings files with the `statusLine` they give, the `allowedHttpHookUrls`
//! that decide between HTTP and command hooks, and the trust state in the global config file. Read,
//! never written: Octoboard does not edit any of these files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::adapter::account_cli::{exported_dir, snapshot_home};
use crate::adapter::files::{
    canonical_project, is_agent_config_dir, read_bounded_text, read_regular_text, LARGE_FILE_LIMIT,
    SMALL_FILE_LIMIT,
};
use crate::adapter::LaunchSpec;
use crate::agents::layout;
use crate::protocol::{notice_code, Agent, Notice};

/// The configuration directory in effect for this launch's Claude Code, when there is one: the
/// session's, else one the user's own shell exports.
fn explicit_config_dir(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
) -> Option<PathBuf> {
    pinned
        .map(PathBuf::from)
        .or_else(|| exported_dir(shell_env, layout::CLAUDE.config_dir_var))
}

/// A settings file Claude Code reads: one the project keeps, read through `browse::live`, or one
/// it does not (the user's, the managed ones), read as a regular file wherever it lies, since a
/// home directory opened as the project holds the user's own.
pub(super) enum SettingsFile {
    Project(PathBuf),
    Other(PathBuf),
}

impl SettingsFile {
    fn path(&self) -> &Path {
        match self {
            Self::Project(path) | Self::Other(path) => path,
        }
    }

    /// Its text, within the size bound. `project` is the canonical working directory.
    fn read(&self, project: &Path) -> std::io::Result<String> {
        match self {
            Self::Project(path) => read_bounded_text(path, project, SMALL_FILE_LIMIT),
            Self::Other(path) => read_regular_text(path, SMALL_FILE_LIMIT),
        }
    }
}

/// The settings files the user and the project write, lowest precedence first: the user's in
/// `user_dir`, then the project's shared and local ones under `project`. Claude Code's flags and
/// its managed settings outrank all three. The project's are the user's own when its `.claude` is
/// `user_dir`, as in a home directory opened as the project.
fn own_settings_files(user_dir: &Path, project: &Path) -> [SettingsFile; 3] {
    let dir = project.join(".claude");
    let project_file = if is_agent_config_dir(&dir, user_dir) {
        SettingsFile::Other
    } else {
        SettingsFile::Project
    };
    [
        SettingsFile::Other(user_dir.join(layout::CLAUDE_SETTINGS_FILE)),
        project_file(dir.join("settings.json")),
        project_file(dir.join("settings.local.json")),
    ]
}

/// The `statusLine` the user's and the project's settings files give, without Octoboard's: each
/// file's object merged over the one below it key by key, as Claude Code merges its settings. A
/// file that cannot be read within its size bound or parsed, or holds no object there, counts for
/// nothing; `None` when no file has one.
pub fn user_status_line(
    user_dir: &Path,
    cwd: &Path,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let project = canonical_project(cwd);
    let mut merged: Option<serde_json::Map<String, serde_json::Value>> = None;
    for file in own_settings_files(user_dir, &project) {
        let Some(serde_json::Value::Object(line)) = file
            .read(&project)
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
/// `project` is the canonical working directory.
pub(super) fn settings_files(
    spec: &LaunchSpec<'_>,
    pinned: Option<&Path>,
    project: &Path,
) -> Vec<SettingsFile> {
    let mut files = Vec::from(own_settings_files(
        &crate::adapter::account_dir(Agent::Claude, pinned, spec.shell_env),
        project,
    ));
    files.extend(managed_settings_files(managed_settings_dir()));
    files
}

/// Where Claude Code reads the settings an administrator manages; under test, a directory that
/// does not exist, so that no test reads the managed settings of the machine it runs on.
fn managed_settings_dir() -> &'static Path {
    Path::new(if cfg!(test) {
        "/nonexistent/octoboard-test-managed"
    } else if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode"
    } else {
        "/etc/claude-code"
    })
}

/// The managed settings in `managed`, lowest precedence first: `managed-settings.json`, then the
/// entries Claude Code takes from the drop-in directory (read out of Claude Code 2.1.312): files
/// and links named `*.json`, not hidden, which it merges in the order of their names.
fn managed_settings_files(managed: &Path) -> Vec<SettingsFile> {
    let mut files = vec![SettingsFile::Other(managed.join("managed-settings.json"))];
    if let Ok(entries) = std::fs::read_dir(managed.join("managed-settings.d")) {
        let mut dropped: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_file() || kind.is_symlink())
                    && name.ends_with(".json")
                    && !name.starts_with('.')
            })
            .map(|entry| entry.path())
            .collect();
        dropped.sort();
        files.extend(dropped.into_iter().map(SettingsFile::Other));
    }
    files
}

/// The model Claude Code itself would pick from its configuration, which Octoboard then leaves
/// to it: `ANTHROPIC_MODEL` in `shell_env`, else in the `env` block of the highest-precedence
/// settings file that has one there, else the `model` of the highest-precedence settings file
/// that gives one (as Claude Code applies a settings file's `env` to its environment, where the
/// variable outranks `model`). The files are the managed settings, then the project's local and
/// shared ones when `cwd` (the session's working directory) is given, then the user's in the
/// configuration directory in effect (`pinned`, else one the shell exports, else `~/.claude`).
/// Policy delivered through an MDM profile or the claude.ai console is in none of these files and
/// is not seen, nor is the managed `availableModels`. A blank value, an unreadable file and one
/// that is not JSON count for nothing.
pub(crate) fn configured_model(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
    cwd: Option<&Path>,
) -> Option<String> {
    configured_model_in(shell_env, pinned, cwd, managed_settings_dir())
}

/// [`configured_model`] with the managed settings directory given.
fn configured_model_in(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
    cwd: Option<&Path>,
    managed: &Path,
) -> Option<String> {
    let named = |text: &str| {
        Some(text.trim())
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    if let Some(model) = shell_env.get(MODEL_VARIABLE).and_then(|model| named(model)) {
        return Some(model);
    }
    let user_dir = crate::adapter::account_dir(Agent::Claude, pinned, shell_env);
    let (project, mut files) = match cwd {
        Some(cwd) => {
            let project = canonical_project(cwd);
            let files = Vec::from(own_settings_files(&user_dir, &project));
            (project, files)
        }
        None => (
            PathBuf::new(),
            vec![SettingsFile::Other(
                user_dir.join(layout::CLAUDE_SETTINGS_FILE),
            )],
        ),
    };
    files.extend(managed_settings_files(managed));
    // Highest precedence first.
    let settings: Vec<serde_json::Value> = files
        .iter()
        .rev()
        .filter_map(|file| serde_json::from_str(&file.read(&project).ok()?).ok())
        .collect();
    let find = |name: &dyn Fn(&serde_json::Value) -> Option<String>| settings.iter().find_map(name);
    find(&|settings| named(settings.get("env")?.get(MODEL_VARIABLE)?.as_str()?))
        .or_else(|| find(&|settings| named(settings.get("model")?.as_str()?)))
}

/// The environment variable that names the model Claude Code starts with.
const MODEL_VARIABLE: &str = "ANTHROPIC_MODEL";

/// Whether Claude Code would run an HTTP hook for `url` given `files`: it does unless some file
/// defines `allowedHttpHookUrls`, whose arrays merge across files, and no merged pattern matches. A
/// pattern's `*` stands for anything. As in Claude Code, which parses each pattern as a URL
/// (2.1.314), only the scheme and authority (the host) compare without regard to case; the path and
/// query do not, so a pattern naming `/HOOK/*` lets no hook to `/hook/…` through. A file that is
/// absent, is not a regular file once links are followed, or cannot be parsed counts for nothing. A
/// regular file that cannot be read here answers no: Claude Code may still read what is refused
/// here (a project file linked out of `project`, which is the canonical working directory, or a
/// file above the size bound), and an allowlist in it leaving the daemon out would drop every HTTP
/// hook without a word, while the command form always runs.
pub(super) fn http_hooks_allowed(url: &str, files: &[SettingsFile], project: &Path) -> bool {
    let mut patterns: Option<Vec<String>> = None;
    for file in files {
        let text = match file.read(project) {
            Ok(text) => text,
            // `metadata` does not open the file, so a FIFO cannot block it.
            Err(_) if std::fs::metadata(file.path()).is_ok_and(|meta| meta.is_file()) => {
                return false
            }
            Err(_) => continue,
        };
        let Ok(settings) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(list) = settings
            .get("allowedHttpHookUrls")
            .and_then(serde_json::Value::as_array)
        {
            patterns.get_or_insert_with(Vec::new).extend(
                list.iter()
                    .filter_map(|p| p.as_str().map(fold_scheme_and_host)),
            );
        }
    }
    let url = fold_scheme_and_host(url);
    patterns.is_none_or(|patterns| {
        patterns
            .iter()
            .any(|pattern| wildcard_matches(pattern, &url))
    })
}

/// `text` with its scheme and authority lower-cased and the rest, from the path on, left as it is.
/// Text with no `://` is left as it is too.
fn fold_scheme_and_host(text: &str) -> String {
    let Some(scheme_end) = text.find("://") else {
        return text.to_owned();
    };
    let authority = scheme_end + "://".len();
    let end = text[authority..]
        .find(['/', '?', '#'])
        .map_or(text.len(), |at| authority + at);
    format!("{}{}", text[..end].to_lowercase(), &text[end..])
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
pub(crate) fn global_config_file(
    shell_env: &HashMap<String, String>,
    pinned: Option<&Path>,
) -> PathBuf {
    match explicit_config_dir(shell_env, pinned) {
        Some(dir) => dir.join(".claude.json"),
        None => snapshot_home(shell_env).join(".claude.json"),
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
        &read_regular_text(
            &global_config_file(spec.shell_env, pinned),
            LARGE_FILE_LIMIT,
        )
        .ok()?,
    )
    .ok()?;

    let canonical = canonical_project(spec.cwd);
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

    /// What sits in a project or a config directory is whatever was put there: a FIFO at a settings
    /// or config path counts as unreadable instead of holding the launch until a writer appears.
    #[test]
    fn a_fifo_at_a_settings_or_config_path_does_not_hold_the_launch() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        std::fs::create_dir_all(home.join(".claude")).expect("user config dir");
        std::fs::create_dir_all(fixture.cwd.join(".claude")).expect("project config dir");
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        // A version that reads `allowedHttpHookUrls` too.
        fixture.agent_version = Some((2, 1, 63));
        for path in [
            fixture.cwd.join(".claude/settings.local.json"),
            fixture.cwd.join(".claude/settings.json"),
            home.join(".claude/settings.json"),
            home.join(".claude.json"),
        ] {
            let c = std::ffi::CString::new(path.into_os_string().into_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        }

        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let plan = ClaudeAdapter.plan(&fixture.spec()).map(|plan| plan.notice);
            let _ = done.send(plan.is_ok_and(|notice| notice.is_none()));
        });
        assert_eq!(
            finished.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(true),
            "the launch was held by a FIFO or did not plan"
        );
    }

    /// A settings file above its size bound counts for nothing, as an unreadable one does.
    #[test]
    fn an_oversized_settings_file_counts_for_nothing() {
        let dir = crate::test_support::ScratchDir::new("oversized-settings");
        let user = dir.join("user");
        let cwd = dir.join("project");
        std::fs::create_dir_all(&user).expect("user dir");
        std::fs::create_dir_all(cwd.join(".claude")).expect("project dir");
        let padded = |settings: serde_json::Value| {
            let mut settings = settings;
            settings["pad"] = serde_json::json!("x".repeat(SMALL_FILE_LIMIT));
            settings.to_string()
        };
        let line = serde_json::json!({ "statusLine": { "type": "command", "command": "x" } });
        std::fs::write(user.join("settings.json"), padded(line.clone())).expect("user settings");
        std::fs::write(
            cwd.join(".claude/settings.local.json"),
            padded(line.clone()),
        )
        .expect("project settings");
        assert!(user_status_line(&user, &cwd).is_none());
        std::fs::write(user.join("settings.json"), line.to_string()).expect("user settings");
        assert!(user_status_line(&user, &cwd).is_some());

        let url = "http://127.0.0.1:4321/hook/session-1?key=feedface";
        // Read, the allowlist lets the daemon in; above the bound it cannot be read here while
        // Claude Code may still read it, so the command form is kept.
        let allowlist = serde_json::json!({ "allowedHttpHookUrls": ["http://127.0.0.1:*"] });
        let project = canonical_project(&cwd);
        let file = project.join(".claude/settings.json");
        std::fs::write(&file, padded(allowlist.clone())).expect("allowlist");
        let files = [SettingsFile::Project(file.clone())];
        assert!(!http_hooks_allowed(url, &files, &project));
        std::fs::write(&file, allowlist.to_string()).expect("allowlist");
        assert!(http_hooks_allowed(url, &files, &project));
    }

    /// The model Claude Code configures itself comes from the environment first, then from the
    /// highest-precedence settings file that names one, the project's files only when a working
    /// directory is given; blank, unreadable and non-string values name nothing.
    #[test]
    fn the_configured_model_is_the_one_claude_code_would_pick() {
        let dir = crate::test_support::ScratchDir::new("configured-model");
        let user = dir.join("user");
        let cwd = dir.join("project");
        let managed = dir.join("managed");
        std::fs::create_dir_all(&user).expect("user dir");
        std::fs::create_dir_all(cwd.join(".claude")).expect("project dir");
        std::fs::create_dir_all(managed.join("managed-settings.d")).expect("managed dir");
        let write = |path: PathBuf, text: &str| std::fs::write(path, text).expect("settings");
        let configured = |env: &[(&str, &str)], project: bool| {
            let env: HashMap<String, String> = env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            configured_model_in(
                &env,
                Some(&user),
                project.then_some(cwd.as_path()),
                &managed,
            )
        };

        assert_eq!(configured(&[], true), None);
        write(user.join("settings.json"), r#"{"model":" sonnet "}"#);
        assert_eq!(configured(&[], false).as_deref(), Some("sonnet"));
        // The project's files outrank the user's, the local one the shared one, and are not
        // known until a launch gives the directory.
        write(cwd.join(".claude/settings.json"), r#"{"model":"opus"}"#);
        assert_eq!(configured(&[], false).as_deref(), Some("sonnet"));
        assert_eq!(configured(&[], true).as_deref(), Some("opus"));
        write(
            cwd.join(".claude/settings.local.json"),
            r#"{"model":"haiku"}"#,
        );
        assert_eq!(configured(&[], true).as_deref(), Some("haiku"));
        // A file that names nothing usable is passed over.
        write(cwd.join(".claude/settings.local.json"), r#"{"model":"  "}"#);
        write(cwd.join(".claude/settings.json"), r#"{"model":7}"#);
        assert_eq!(configured(&[], true).as_deref(), Some("sonnet"));
        write(user.join("settings.json"), "not json");
        assert_eq!(configured(&[], true), None);
        // Managed settings outrank the project's, and a later drop-in the base file.
        write(managed.join("managed-settings.json"), r#"{"model":"base"}"#);
        write(
            managed.join("managed-settings.d/10-a.json"),
            r#"{"model":"a"}"#,
        );
        write(
            managed.join("managed-settings.d/20-b.json"),
            r#"{"effort":"low"}"#,
        );
        write(
            managed.join("managed-settings.d/.hidden.json"),
            r#"{"model":"hidden"}"#,
        );
        write(cwd.join(".claude/settings.json"), r#"{"model":"opus"}"#);
        assert_eq!(configured(&[], true).as_deref(), Some("a"));
        // An `ANTHROPIC_MODEL` in a file's `env` block outranks every `model`, the higher file's
        // first.
        write(
            user.join("settings.json"),
            r#"{"env":{"ANTHROPIC_MODEL":"from-user-env"}}"#,
        );
        assert_eq!(configured(&[], true).as_deref(), Some("from-user-env"));
        write(
            cwd.join(".claude/settings.local.json"),
            r#"{"env":{"ANTHROPIC_MODEL":"from-local-env","OTHER":"x"}}"#,
        );
        assert_eq!(configured(&[], true).as_deref(), Some("from-local-env"));
        write(
            managed.join("managed-settings.json"),
            r#"{"env":{"ANTHROPIC_MODEL":" "}}"#,
        );
        assert_eq!(configured(&[], true).as_deref(), Some("from-local-env"));
        write(user.join("settings.json"), "not json");
        write(cwd.join(".claude/settings.local.json"), "{}");
        // The variable wins over every file; a blank one counts for nothing.
        assert_eq!(
            configured(&[("ANTHROPIC_MODEL", "claude-x")], true).as_deref(),
            Some("claude-x")
        );
        assert_eq!(
            configured(&[("ANTHROPIC_MODEL", " ")], true).as_deref(),
            Some("a")
        );
    }

    /// A home directory opened as the project holds the user's own settings and global config,
    /// which a dotfiles manager may link elsewhere: they are read as Claude Code reads them, not
    /// refused as links out of the project.
    #[test]
    fn the_users_files_under_a_home_opened_as_the_project_are_read_through_their_links() {
        let mut fixture = spec_fixture();
        let dotfiles = crate::test_support::ScratchDir::new("dotfiles");
        let home = canonical_project(&fixture.cwd);
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        std::fs::create_dir_all(dotfiles.join("claude")).expect("linked config dir");
        std::fs::write(
            dotfiles.join("claude/settings.json"),
            serde_json::json!({ "statusLine": { "type": "command", "command": "x" } }).to_string(),
        )
        .expect("user settings");
        std::fs::write(
            dotfiles.join(".claude.json"),
            serde_json::json!({
                "projects": { home.to_string_lossy(): { "hasTrustDialogAccepted": false } }
            })
            .to_string(),
        )
        .expect("global config");
        std::os::unix::fs::symlink(dotfiles.join("claude"), home.join(".claude")).expect("link");
        std::os::unix::fs::symlink(dotfiles.join(".claude.json"), home.join(".claude.json"))
            .expect("link");

        assert!(user_status_line(&home.join(".claude"), &home).is_some());
        // The project's `.claude` is the user's: its files hold no allowlist, so HTTP hooks stand.
        let files = own_settings_files(&home.join(".claude"), &home);
        assert!(http_hooks_allowed(
            "http://127.0.0.1:4321/hook/session-1?key=feedface",
            &files,
            &home
        ));
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_some());
    }

    /// An `allowedHttpHookUrls` that leaves the daemon's URL out keeps the command form, whichever
    /// file defines it.
    #[test]
    fn an_http_hook_allowlist_without_the_daemon_keeps_the_command_form() {
        let url = "http://127.0.0.1:4321/hook/session-1?key=feedface";
        let dir = crate::test_support::ScratchDir::new("hook-allowlist");
        let others = |paths: &[&PathBuf]| -> Vec<SettingsFile> {
            paths
                .iter()
                .map(|path| SettingsFile::Other(path.to_path_buf()))
                .collect()
        };
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

        let project = dir.join("project");
        // Not a regular file: nothing Claude Code takes an allowlist from either.
        let directory = dir.join("directory.json");
        std::fs::create_dir(&directory).expect("directory");
        let allowed = |paths: &[&PathBuf]| http_hooks_allowed(url, &others(paths), &project);
        assert!(allowed(&[&none, &broken, &missing, &directory]));
        assert!(!allowed(&[&other]));
        assert!(!allowed(&[&empty]));
        assert!(allowed(&[&loopback]));
        // The arrays merge, so one file naming the daemon is enough.
        assert!(allowed(&[&other, &none, &loopback]));
        // Only the scheme and host fold case, as in Claude Code: a path or query in other case
        // leaves the daemon out.
        let folded = file(
            "folded.json",
            r#"{"allowedHttpHookUrls":["HTTP://127.0.0.1:*/hook/*?key=*"]}"#,
        );
        let upper_path = file(
            "upper-path.json",
            r#"{"allowedHttpHookUrls":["http://127.0.0.1:*/HOOK/*"]}"#,
        );
        let upper_query = file(
            "upper-query.json",
            r#"{"allowedHttpHookUrls":["http://127.0.0.1:*/hook/*?KEY=*"]}"#,
        );
        assert!(allowed(&[&folded]));
        assert!(!allowed(&[&upper_path]));
        assert!(!allowed(&[&upper_query]));
        assert!(http_hooks_allowed(
            "HTTP://127.0.0.1:4321/hook/session-1?key=feedface",
            &others(&[&folded]),
            &project
        ));
        assert!(!http_hooks_allowed(
            "http://127.0.0.1:4321/HOOK/session-1?key=feedface",
            &others(&[&folded]),
            &project
        ));
        assert_eq!(
            fold_scheme_and_host("HTTP://LocalHost:9/A/b?C=D#E"),
            "http://localhost:9/A/b?C=D#E"
        );
        assert_eq!(fold_scheme_and_host("HTTP://Host?Q"), "http://host?Q");
        assert_eq!(fold_scheme_and_host("*"), "*");
        assert!(wildcard_matches("http://*/hook/*", url));
        assert!(wildcard_matches("http://127.0.0.1:*", url));
        assert!(!wildcard_matches("http://*/hook", url));
    }
}
