//! The directory of one launch that stands for Antigravity CLI's `~/.gemini`, given with
//! `--gemini_dir` (1.5.14; `docs/agent-cli-reference.md`, section "Antigravity CLI").
//!
//! Antigravity CLI reads its hooks, its MCP servers and its global rule from that directory and
//! from nowhere a launch can name, so the launch gets one of its own in the session's scratch
//! directory:
//!
//! - `config/hooks.json`, `config/mcp_config.json` and `GEMINI.md` are written for the launch: the
//!   user's own content with Octoboard's hooks, server and role text added. A file of the user's
//!   that is there and cannot be used fails the plan, since a launch of Octoboard's without it would
//!   run without the user's hooks or servers and say nothing of it: the session starts bare, the CLI
//!   reading the file itself as in a plain terminal, and the notice names the file.
//! - Every other entry, `antigravity-cli` (the CLI's settings, conversations and trust) among
//!   them, is a link to the user's own, so what the session saves is saved where the user's other
//!   sessions find it. The two directories the CLI makes on its first run (`antigravity-cli` and
//!   `config/projects`) are made in the user's directory first where it lacks them: the CLI does
//!   not make a directory through a link that leads to a missing one (it ends with status 1,
//!   measured). They are what `~/.gemini` holds anyway and nothing of Octoboard's own, which the
//!   user ruled allowed (2026-11-23).
//! - `config/.migrated`, the marker of the CLI's migration, is the user's own where the CLI would
//!   find one through the launch's directory, and an empty file of the launch's where it would
//!   not (the user has none, or theirs is a link that leads nowhere), so that the CLI never
//!   migrates through a launch's directory. The migration moves what an earlier layout kept in
//!   `antigravity-cli` (`rules`, `workflows`, `skills`, `plugins`, `hooks.json`,
//!   `mcp_config.json`) into `config`, leaving a link behind each: run through a launch's
//!   directory it was measured to move the user's own out of their home into the launch's, over
//!   the two files written for the launch, and to leave links in the home that lead nowhere once
//!   the launch's directory is gone. With the launch's own marker nothing moves and the user's
//!   home gets no marker.
//! - A home with no marker that still holds any of those six entries in `antigravity-cli`, as
//!   anything but the link the migration leaves, fails the plan ([`require_migrated_layout`]): behind the
//!   launch's own marker the session would run without those rules, hooks and servers and say
//!   nothing of it, and a hook left out may be one that denies. The session then starts bare,
//!   against the user's own home, where the CLI migrates in place as a plain terminal run would,
//!   after which the next launch is Octoboard's again. The check comes first, before a file of the
//!   user's is read, the launch's directory is made or the directory flag is probed: it leaves
//!   nothing behind and starts no process. Of the CLI's short commands, `agy models`
//!   was measured to run the migration, in the home it is run against (where that is the user's
//!   own home it is the user's own run, and the check passes after it); `--version`,
//!   `--help` and `mcp list`, the probe of the directory flag, do not. The listing of the models
//!   (`super::models`) is never run against the home: it gets a directory of its own with its own
//!   marker ([`write_marker`]), and asks the same question first, running nothing where a launch's
//!   plan would fail.
//! - Where the migration would also lose a file, the failure is another: `hooks.json` or
//!   `mcp_config.json` not yet moved while `config` has a file of that name that holds something,
//!   which the migration replaces with the earlier one, keeping no copy (measured). The session
//!   still starts bare, and its CLI's own migration replaces the file as a plain terminal run would
//!   (decided with the user: launch, and tell afterwards); the notice names both files. The
//!   listing of the models runs nothing there. Directories in both places are neither moved,
//!   merged nor deleted.
//!
//! Three things stay in the launch's copy and are gone with it. What a session changes in the
//! three written files (a server added from inside it). `config/config.json`, where the CLI keeps
//! which plugins are on: it writes that file by replacing it, so the link to the user's own becomes
//! a file of the launch's (measured for a user who had none; a session of such a user starts from
//! the CLI's defaults every time). And the launch's settings ([`write_settings`]), which are for
//! the launch alone.
//!
//! Two things a launch goes on without and tells the user of, each as a notice of its own, since
//! the CLI itself says them only in its log, both among the launch's further notices, in this
//! order ([`settings_notice`], [`leftovers_notice`]); the plan's own notice is left for a setting
//! a reduced plan leaves out.
//!
//! - The CLI's own `antigravity-cli/settings.json`, where the CLI reads it as nothing (it does not
//!   parse, or a key the CLI knows has a value it does not take): the CLI runs on its defaults for
//!   the whole file (measured), and so does the launch, which repeats none of the file's allow
//!   rules. Which values the CLI takes is asked of the CLI itself ([`rejected_by_cli`]).
//! - `rules` and `plugins` left in `antigravity-cli` beside the ones of `config` by a migration
//!   that could not move them: the CLI reads the ones of `config` alone (measured; `skills` left
//!   there is still loaded, and nothing is known of `workflows`). The notice says which of the
//!   two is left.
//!
//! The CLI has one account, its login being one item of the system keychain: a launch for an
//! account with a directory of its own is refused before anything else
//! (`super::AntigravityProvider::launch_refusal`), the one refusal a plain terminal shares.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::hooks::{
    FILE_WRITING_TOOLS, POST_TOOL_USE, PRE_INVOCATION, PRE_TOOL_USE, SESSION_START, STOP,
};
use crate::adapter::account_cli::{self, account_dir};
use crate::adapter::files::SMALL_FILE_LIMIT;
use crate::adapter::user_json::{self, Takes};
use crate::adapter::{
    guard_command, hook_command, mcp_server_command, LaunchSpec, COMMAND_HOOK_TIMEOUT_SECS,
    GUARD_HOOK_TIMEOUT_SECS, MCP_TOOL_TIMEOUT_SECS,
};
use crate::hostfs::read_regular_text;
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::protocol::{Code, CodedError, Notice};
use crate::providers::antigravity::{
    APP_DATA_DIR, CONFIG_DIR, EARLIER_LAYOUT_FILE_IN_BOTH_PLACES,
    EARLIER_LAYOUT_LEFTOVERS_NOT_READ, EARLIER_LAYOUT_NOT_MIGRATED, GLOBAL_RULE_FILE, HOOKS_FILE,
    LAUNCH_DIRECTORY_FLAG_NOT_TAKEN, MCP_CONFIG_FILE, MEASURED_VERSION, SETTINGS_FILE,
    SETTINGS_NOT_READ, USER_CONFIG_UNUSABLE,
};
use crate::providers::AgentId;
use crate::session_tools;

const ONBOARDING_RECORD: &str = "cache/onboarding.json";
const ONBOARDING_FLAGS: &[&str] = &[
    "consumerOnboardingComplete",
    "enterpriseOnboardingComplete",
    "onboardingComplete",
];

/// Trusts only the login workspace and carries forward the user's completed onboarding steps.
pub(super) fn prepare_login_home(scratch: &Path, account: &Path) -> Result<PathBuf> {
    let home = scratch.join(crate::providers::antigravity::LAYOUT.default_dir);
    let data = home.join(APP_DATA_DIR);
    crate::platform::fs::create_private_dir(&data)?;
    let settings = json!({"trustedWorkspaces": [scratch.to_string_lossy()]});
    write_private(&data.join(SETTINGS_FILE), settings.to_string().as_bytes())?;

    let record = account.join(APP_DATA_DIR).join(ONBOARDING_RECORD);
    if let Some(onboarding) = read_regular_text(&record, SMALL_FILE_LIMIT)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        let accepted: serde_json::Map<String, Value> = ONBOARDING_FLAGS
            .iter()
            .filter_map(|flag| {
                onboarding[*flag]
                    .as_bool()
                    .map(|value| ((*flag).to_owned(), Value::Bool(value)))
            })
            .collect();
        if !accepted.is_empty() {
            crate::platform::fs::create_private_dir(&data.join("cache"))?;
            write_private(
                &data.join(ONBOARDING_RECORD),
                Value::Object(accepted).to_string().as_bytes(),
            )?;
        }
    }
    Ok(home)
}

/// The launch's directory, inside the session's scratch directory.
pub(super) const LAUNCH_DIR: &str = "antigravity";

/// The flag that names the launch's directory. Only this spelling, joined to its value, is taken
/// by the interactive CLI (1.5.14).
pub(super) const DIRECTORY_FLAG: &str = "--gemini_dir";

/// The name Octoboard's hooks are registered under in `hooks.json`, which maps a name to events. A
/// user's own entry of this name is overwritten in the launch's copy: the name is the session's.
const HOOK_NAME: &str = "octoboard-session";

/// Entries of `config/` the CLI makes itself, beside the two files a launch writes.
const PROJECTS_DIR: &str = "projects";
const MIGRATED_MARKER: &str = ".migrated";

/// What the CLI's migration moves from its data directory into its configuration directory,
/// leaving a link at each (1.5.14): the entries an earlier layout kept there.
const EARLIER_LAYOUT_ENTRIES: [&str; 6] = [
    "rules",
    "workflows",
    "skills",
    "plugins",
    HOOKS_FILE,
    MCP_CONFIG_FILE,
];

/// Those of them that are files. Where the configuration directory already holds one, the
/// migration replaces it with the earlier layout's (measured); a directory already there is left,
/// with the earlier one unmoved.
const EARLIER_LAYOUT_FILES: [&str; 2] = [HOOKS_FILE, MCP_CONFIG_FILE];

/// Those of them the CLI no longer reads from its data directory once its configuration directory
/// is the one in use (1.5.14): a rule file reached the model from `config/rules` alone, and a
/// plugin was listed and loaded from `config/plugins` alone. `skills` left there is still loaded,
/// and a file of `workflows` gave no command from either place, so neither is told of.
const LEFTOVERS_NOT_READ: [&str; 2] = ["rules", "plugins"];

/// What the leftovers notice's `left` says where both of them are left; one alone is named.
const LEFT_BOTH: &str = "both";

/// The events Octoboard's hook is registered for as a plain entry: those of a turn, which carry
/// the conversation's id (`SessionStart` only for a new conversation's first turn,
/// `PreInvocation` for each model request, a resumed conversation's first included, `Stop` for
/// every turn that ends by itself).
const TURN_EVENTS: [&str; 3] = [SESSION_START, PRE_INVOCATION, STOP];

/// The events of a tool call, for which Octoboard's hook is registered for every tool: the
/// background work and the pending interactions read the calls of all of them. A tool event runs
/// only an entry with a `matcher` (1.5.14: a plain entry under `PreToolUse` or `PostToolUse` never
/// ran, one with `"matcher": "*"` ran for every tool).
const TOOL_EVENTS: [&str; 2] = [PRE_TOOL_USE, POST_TOOL_USE];
const EVERY_TOOL: &str = "*";

/// The cross-project guard (`octoboardd guard-hook`, which decides from the launch's list without
/// asking the daemon) answers `PreToolUse` for the two tools that write a file (1.5.14, through
/// the stand-in and through a real model alike), as an entry of its own after Octoboard's hook in
/// that event's list: the CLI runs every entry of the list that matches the call, and a `deny` of
/// the guard's stops the call whatever Octoboard's hook, which writes nothing, gave (measured: the
/// file was not written, and a call no entry denied still drew its permission prompt).
const GUARD_EVENT: &str = PRE_TOOL_USE;

/// What a failed plan's message says was not done: the session itself starts bare.
const LAUNCH_NOT_PREPARED: &str = "Octoboard's additions to the session were not prepared";

/// How long the probe of the directory flag may take.
const FLAG_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// How an `agy` command ends for a flag it does not define: this status, which it gives for
/// every error of usage, and this line on its standard error, the flag named with one dash.
const UNDEFINED_FLAG_STATUS: i32 = 2;
const UNDEFINED_FLAG_LINE: &str = "flags provided but not defined: -gemini_dir";

/// The variable that keeps the CLI from updating itself in the background, set for the short runs
/// Octoboard makes of it: such a run is ended with everything it started, and an update cut short
/// is not something to leave behind. A session's own run is given nothing.
pub(super) const NO_UPDATE_VAR: &str = "AGY_CLI_DISABLE_AUTO_UPDATE";

/// The most a file of the user's directory is copied to, on a host that cannot link a file.
const COPIED_FILE_LIMIT: usize = 4 * 1024 * 1024;

/// The most a probe's output is read to.
const FLAG_PROBE_OUTPUT_LIMIT: usize = 64 * 1024;

/// The file of the launch's directory that holds the launch's settings, given with `--settings`.
/// The CLI reads nothing of that name by itself.
const LAUNCH_SETTINGS_FILE: &str = "octoboard-launch-settings.json";

/// The file of the launch's directory that keeps why the CLI does not read the user's settings
/// file, written by [`build`] where it does not, for [`settings_notice`]: the file's path and the
/// reason, as JSON.
const SETTINGS_NOT_READ_FILE: &str = "octoboard-settings-not-read.json";

/// The file of the launch's directory that holds the user's settings while the CLI is asked
/// whether it takes them ([`rejected_by_cli`]). It is removed once the CLI has answered.
const SETTINGS_QUESTION_FILE: &str = "octoboard-settings-question.json";

/// How the CLI ends a command whose `--settings` names a key it does not know, and one whose
/// value it does not take (1.5.14): with this status and a line on its standard error that starts
/// with one of these, the key after it in double quotes.
const SETTINGS_REFUSED_STATUS: i32 = 1;
const SETTINGS_UNKNOWN_KEY_LINE: &str = "Error: --settings: unknown setting ";
const SETTINGS_INVALID_VALUE_LINE: &str = "Error: --settings: invalid value for setting ";

/// The lists of rules inside the settings' `permissions`, each of which the CLI takes only as a
/// list of texts, a `null` element among them (1.5.14: `[1]` and `[true]` rejected the file,
/// `[null]` and `["…", null]` did not, in its own load and through `--settings` alike).
const PERMISSION_LISTS: [&str; 3] = ["allow", "deny", "ask"];

/// What a launch is given.
pub(super) struct LaunchDir {
    /// The directory, for `--gemini_dir`.
    pub(super) dir: PathBuf,
    /// The user's own `permissions.allow` rules, which the launch's settings have to repeat: an
    /// `allow` given with `--settings` takes the place of the user's list (their `deny` and `ask`
    /// lists stand as they are).
    pub(super) user_allow: Vec<Value>,
}

/// Builds the launch's directory from the default account's and returns it.
pub(super) fn build(spec: &LaunchSpec<'_>) -> Result<LaunchDir> {
    // An account directory of its own was refused before planning
    // (`AntigravityProvider::launch_refusal`).
    let account = account_dir(AgentId::ANTIGRAVITY, None, spec.shell_env);
    require_migrated_layout(&account, LAUNCH_NOT_PREPARED)?;
    // Read before anything is written, so a failure leaves nothing behind.
    let hooks = with_hooks(
        user_object(
            &account.join(CONFIG_DIR).join(HOOKS_FILE),
            BYTE_ORDER_MARK_ONLY,
        )?,
        spec,
    );
    let mcp_config = account.join(CONFIG_DIR).join(MCP_CONFIG_FILE);
    let servers = with_server(read_mcp_config(&mcp_config)?, &mcp_config, spec)?;
    let rule = with_role(user_text(&account.join(GLOBAL_RULE_FILE))?, spec);
    let settings_path = account.join(APP_DATA_DIR).join(SETTINGS_FILE);
    let user_settings = user_settings(&settings_path)?;

    let dir = spec.scratch.join(LAUNCH_DIR);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {}", dir.display()))?;
    }
    let config = dir.join(CONFIG_DIR);
    std::fs::create_dir_all(&config).with_context(|| format!("creating {}", config.display()))?;
    // The CLI's own data has to be the user's, or the conversation would be kept in the launch's
    // directory and lost with it; the CLI makes this directory itself on its first run.
    super::native_dirs::ensure_account_dirs(
        &account.join(APP_DATA_DIR),
        &account.join(CONFIG_DIR).join(PROJECTS_DIR),
    )?;

    link_entries(&account, &dir, &[CONFIG_DIR, GLOBAL_RULE_FILE])?;
    link_entries(
        &account.join(CONFIG_DIR),
        &config,
        &[HOOKS_FILE, MCP_CONFIG_FILE],
    )?;
    // Where the CLI would find no marker of a finished migration here, the launch gets one of
    // its own, so the CLI does not migrate through this directory (the module's notes say what
    // that moves). What the CLI finds is what counts, so links are followed: a marker of the
    // user's that is a link leading nowhere was linked above as it is, and is taken away first,
    // as a link, the user's own left where it is. The user's home gets no marker.
    let marker = config.join(MIGRATED_MARKER);
    if std::fs::metadata(&marker).is_err() {
        crate::hostfs::remove_any(&marker)
            .with_context(|| format!("clearing {}", marker.display()))?;
        write_marker(&dir)?;
    }
    write_private(&config.join(HOOKS_FILE), hooks.to_string().as_bytes())?;
    write_private(
        &config.join(MCP_CONFIG_FILE),
        servers.to_string().as_bytes(),
    )?;
    write_private(&dir.join(GLOBAL_RULE_FILE), rule.as_bytes())?;

    // Asked once the directory has its marker and its files, so the CLI's answers cost the user's
    // home nothing; the directory flag first, so a CLI that does not take it is asked nothing else.
    require_directory_flag(spec, &dir)?;
    // Where the CLI does not read the file the launch repeats no rule of it; the user is told by
    // one of the launch's further notices ([`settings_notice`]), from the reason found here, kept
    // in the directory, so that the notice says what the rules passed were decided by.
    let user_allow = match allow_or_why(user_settings, &dir, spec) {
        Ok(allow) => allow,
        Err(why) => {
            let kept = json!({ "path": settings_path.to_string_lossy(), "why": why });
            write_private(
                &dir.join(SETTINGS_NOT_READ_FILE),
                kept.to_string().as_bytes(),
            )?;
            Vec::new()
        }
    };

    Ok(LaunchDir { dir, user_allow })
}

/// The user's own `permissions.allow` rules where the CLI reads their settings file, or why it
/// does not: by [`user_settings`] alone, or by the CLI's answer ([`rejected_by_cli`]), asked in
/// the launch's directory `dir`.
fn allow_or_why(
    user_settings: UserSettings,
    dir: &Path,
    spec: &LaunchSpec<'_>,
) -> std::result::Result<Vec<Value>, String> {
    match user_settings {
        UserSettings::Read { allow, settings } => match rejected_by_cli(dir, &settings, spec) {
            Some(why) => Err(why),
            None => Ok(allow),
        },
        UserSettings::NotRead { why } => Err(why),
    }
}

/// The notice of the user's settings file the CLI does not read, for a launch of `spec` whose
/// directory [`build`] made, one of the launch's further notices: from the reason [`build`] found
/// and kept in the directory ([`SETTINGS_NOT_READ_FILE`]), so that neither the file nor the CLI is
/// asked again and the notice agrees with the rules the launch passed. `None` where the CLI reads
/// the file, and where the directory holds no reason that can be read.
pub(super) fn settings_notice(spec: &LaunchSpec<'_>) -> Option<Notice> {
    let kept = spec.scratch.join(LAUNCH_DIR).join(SETTINGS_NOT_READ_FILE);
    let kept: Value =
        serde_json::from_str(&read_regular_text(&kept, SMALL_FILE_LIMIT).ok()?).ok()?;
    let path = kept.get("path")?.as_str()?;
    let why = kept.get("why")?.as_str()?;
    Some(settings_not_read(Path::new(path), why))
}

/// Writes the marker of a finished migration, empty, into `dir`, a directory of Octoboard's own
/// that stands for `~/.gemini`, making its configuration directory: a run of the CLI given `dir`
/// then migrates nothing.
pub(super) fn write_marker(dir: &Path) -> Result<()> {
    let config = dir.join(CONFIG_DIR);
    std::fs::create_dir_all(&config).with_context(|| format!("creating {}", config.display()))?;
    write_private(&config.join(MIGRATED_MARKER), b"")
}

/// The notice for the user's settings file at `path` that the CLI reads as nothing, `why` saying
/// what is wrong with it.
fn settings_not_read(path: &Path, why: &str) -> Notice {
    Notice::new(
        Code::of_provider(SETTINGS_NOT_READ),
        format!(
            "Antigravity CLI does not read `{}` ({why}) and runs on its default settings, so \
             this session has none of the file's permission rules, trusted folders or other \
             settings: repair the file",
            path.display()
        ),
        &[("path", &path.to_string_lossy())],
    )
    // It holds for the whole launch: the CLI read the file once, as it started.
    .standing()
}

/// What a launch of `spec` tells the user of `rules` and `plugins` left in the home's data
/// directory ([`leftovers_not_read`]), beside the launch's own notice. Read from the home again,
/// not kept from [`build`]: it is asked for after the plan, once per launch. None for an account
/// with a directory of its own, whose launch is refused (`super::AntigravityProvider::launch_refusal`).
pub(super) fn leftovers_notice(spec: &LaunchSpec<'_>) -> Option<Notice> {
    if spec.config_dir.is_some() {
        return None;
    }
    leftovers_not_read(&account_dir(AgentId::ANTIGRAVITY, None, spec.shell_env))
}

/// The notice for a home whose data directory still holds `rules` or `plugins` with something in
/// them, as anything but the link the migration leaves, where a launch is not refused for them (the
/// home has its marker): the CLI reads the ones of its configuration directory alone. `None` for a
/// home with neither. It says which is left as `left`: the directory's name where there is one,
/// `both` where both are.
fn leftovers_not_read(account: &Path) -> Option<Notice> {
    let (app_data, config) = (account.join(APP_DATA_DIR), account.join(CONFIG_DIR));
    let left: Vec<&str> = LEFTOVERS_NOT_READ
        .into_iter()
        .filter(|name| {
            let entry = app_data.join(name);
            !left_by_migration(&entry, &config.join(name))
                && std::fs::read_dir(&entry).is_ok_and(|mut entries| entries.next().is_some())
        })
        .collect();
    let (named, param) = match left.as_slice() {
        [] => return None,
        [one] => (format!("the `{one}` directory"), *one),
        _ => (
            "the `rules` and `plugins` directories".to_string(),
            LEFT_BOTH,
        ),
    };
    Some(Notice::new(
        Code::of_provider(EARLIER_LAYOUT_LEFTOVERS_NOT_READ),
        format!(
            "Antigravity CLI does not read {named} left in `{}` from the layout of an earlier \
             version: it reads only what is in `{}`, so this session runs without what is only in \
             the earlier place: move what you want to keep",
            app_data.display(),
            config.display()
        ),
        &[
            ("path", &app_data.to_string_lossy()),
            ("current_path", &config.to_string_lossy()),
            ("left", param),
        ],
    ))
}

/// Fails for a home whose configuration the CLI has not yet moved out of an earlier layout: no
/// marker of a finished migration that the CLI would find (links followed), and one of the entries
/// the migration moves still in the data directory as anything but the link the migration leaves
/// behind, which leads to the entry of that name in the configuration directory: a home that was
/// migrated and lost its marker passes, while a link of the user's own to somewhere else
/// (a directory of dotfiles) is an entry not yet moved like any other.
///
/// The narrower case is told first, under a code of its own: one of the two files not yet moved
/// while the configuration directory has a file of that name that holds something. A run of the
/// CLI replaces that file with the earlier one, keeping no copy, so the user is told of both files:
/// before the run where nothing has run it yet (the listing of the models), and as the reason a
/// launch started bare, whose own run makes that replacement as a plain terminal run would. An
/// empty file there, which a run of the CLI leaves in a home that had none, holds nothing to lose
/// and is no such case.
///
/// Whatever would run the CLI's migration asks this first: a launch, and the listing of the
/// models, whose command migrates the home it is run against. A launch's plan fails on it and the
/// session starts bare; the listing runs nothing. `refused` says what was not done, for the
/// message.
pub(super) fn require_migrated_layout(account: &Path, refused: &str) -> Result<()> {
    let config = account.join(CONFIG_DIR);
    if std::fs::metadata(config.join(MIGRATED_MARKER)).is_ok() {
        return Ok(());
    }
    let app_data = account.join(APP_DATA_DIR);
    let unmoved = |name: &str| {
        let entry = app_data.join(name);
        std::fs::symlink_metadata(&entry).is_ok() && !left_by_migration(&entry, &config.join(name))
    };
    for name in EARLIER_LAYOUT_FILES {
        let (earlier, current) = (app_data.join(name), config.join(name));
        let holds_something =
            std::fs::metadata(&current).is_ok_and(|found| found.is_file() && found.len() > 0);
        if unmoved(name) && holds_something {
            return Err(CodedError::raised(
                Code::of_provider(EARLIER_LAYOUT_FILE_IN_BOTH_PLACES),
                format!(
                    "Antigravity CLI has `{}` and, from the layout of an earlier version, `{}`, \
                     and a run of its own replaces the first with the second, keeping no copy, so \
                     {refused}: keep the one you want and move or remove the other, then run `agy` \
                     once in a terminal",
                    current.display(),
                    earlier.display()
                ),
                &[
                    ("path", &current.to_string_lossy()),
                    ("earlier_path", &earlier.to_string_lossy()),
                ],
            ));
        }
    }
    if !EARLIER_LAYOUT_ENTRIES.iter().any(|name| unmoved(name)) {
        return Ok(());
    }
    Err(CodedError::raised(
        Code::of_provider(EARLIER_LAYOUT_NOT_MIGRATED),
        format!(
            "Antigravity CLI's configuration in `{}` is still in the layout of an earlier \
             version, which the CLI moves on a run of its own, so {refused}: run `agy` once in a \
             terminal",
            app_data.display()
        ),
        &[],
    ))
}

/// Whether `entry`, of the data directory, is the link the migration leaves where it moved an
/// entry to `moved`, in the configuration directory: a link that resolves to `moved`, or, where
/// `moved` is no longer there to resolve to, one that names it as the CLI wrote it.
fn left_by_migration(entry: &Path, moved: &Path) -> bool {
    let is_link =
        std::fs::symlink_metadata(entry).is_ok_and(|found| found.file_type().is_symlink());
    is_link
        && (crate::hostfs::is_same_path(entry, moved)
            || std::fs::read_link(entry).is_ok_and(|target| target == moved))
}

/// What a probe of the directory flag found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FlagProbe {
    /// The CLI ended the way it does for a flag it does not define.
    Undefined,
    /// Anything else: the flag was taken, or the probe did not run, timed out or failed some
    /// other way, none of which says the flag is missing.
    NotRefused,
}

/// What this process found the CLI to answer, by what was asked, each with when it was kept
/// ([`kept_or_asked`]).
type Kept<K, A> = Mutex<Vec<(K, A, Instant)>>;

/// How long an answer that says nothing of what was asked (a run that timed out, could not start
/// or ended some other way) is kept before the question is asked again. Such a run may have met a
/// passing stall (an exec held by endpoint-security software), and its answer counts as the file
/// read for that launch. Keeping the answer for a few minutes spares a CLI that hangs every time
/// from costing its wait at every launch.
const PASSING_ANSWER_KEPT: Duration = Duration::from_secs(5 * 60);

/// The answer `kept` holds for `key` at `now`, or else `ask`'s, kept with `now`. An answer
/// `for_good` says is not for good is dropped once [`PASSING_ANSWER_KEPT`] has passed since it
/// was kept, so it is asked again. The lock is not held while `ask` runs: two launches at once may
/// both ask, and the later answer stands.
fn kept_or_asked<K: PartialEq, A: Clone>(
    kept: &Kept<K, A>,
    key: K,
    now: Instant,
    for_good: impl Fn(&A) -> bool,
    ask: impl FnOnce() -> A,
) -> A {
    let lock = || {
        kept.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    {
        let mut kept = lock();
        kept.retain(|(_, answer, at)| {
            for_good(answer) || now.saturating_duration_since(*at) < PASSING_ANSWER_KEPT
        });
        if let Some((_, answer, _)) = kept.iter().find(|(asked, _, _)| *asked == key) {
            return answer.clone();
        }
    }
    let answer = ask();
    let mut kept = lock();
    kept.retain(|(asked, _, _)| *asked != key);
    kept.push((key, answer.clone(), now));
    answer
}

/// The probes made so far by this process, by the binary probed and its version: a launch starts
/// no process for a binary already probed, a CLI that hangs costs its wait once, and an update of
/// the CLI, which changes its version, is probed afresh. A binary whose version was not read
/// (`None`) is not: an update at the same path keeps its key, so a refusal found for it stands
/// until the daemon restarts. Every probe is kept for good: one that says nothing only lets the
/// launch fail or not by itself.
static FLAG_PROBES: Kept<FlagProbeKey, FlagProbe> = Mutex::new(Vec::new());

type FlagProbeKey = (PathBuf, Option<crate::adapter::AgentVersion>);

/// The binary a launch of `spec` runs and its version, which key what the CLI was found to answer
/// ([`FLAG_PROBES`], [`SETTINGS_ANSWERS`]).
fn probe_key(spec: &LaunchSpec<'_>) -> FlagProbeKey {
    let binary = account_cli::resolve_account_binary(AgentId::ANTIGRAVITY, spec.shell_env)
        .map(|binary| binary.program)
        .unwrap_or_default();
    (binary, spec.agent_version)
}

/// What the CLI answered to settings asked about with `--settings` ([`rejected_by_cli`]), by the
/// binary, its version and a digest of the settings' text: the answer depends on nothing else, so
/// a launch asks no question already answered. An answer that says something is kept for good; a
/// run that said nothing ([`SettingsAnswer::Nothing`]) only for [`PASSING_ANSWER_KEPT`], since it
/// counts as the file read, which repeats the user's allow rules. It holds a digest for each
/// settings text a user's file has had while the daemon ran, or a few more for the keys taken out
/// of the question.
static SETTINGS_ANSWERS: Kept<(FlagProbeKey, [u8; 32]), SettingsAnswer> = Mutex::new(Vec::new());

/// Fails the plan for a CLI that does not take the directory flag, before the session's terminal is
/// up, so the session starts bare (the probe stays a probe: nothing is refused for it). An
/// undefined flag ends any `agy` command at once, with status 2 and a line naming the flag on its
/// standard error, and `mcp list` with a flag it takes reads one file and exits (1.5.14), so the
/// probe needs no sign-in and no network. Only that status with that line fails: the status
/// alone is what every error of usage gives, and a probe that could not be run, timed out or
/// failed some other way says nothing about the flag, so the launch goes on to fail or not by
/// itself.
fn require_directory_flag(spec: &LaunchSpec<'_>, dir: &Path) -> Result<()> {
    let found = kept_or_asked(
        &FLAG_PROBES,
        probe_key(spec),
        Instant::now(),
        |_| true,
        || probe_directory_flag(dir, spec.shell_env),
    );
    match found {
        FlagProbe::NotRefused => Ok(()),
        FlagProbe::Undefined => Err(CodedError::raised(
            Code::of_provider(LAUNCH_DIRECTORY_FLAG_NOT_TAKEN),
            format!(
                "this Antigravity CLI does not take `{DIRECTORY_FLAG}`, the option Octoboard \
                 starts a session with ({MEASURED_VERSION} does), so {LAUNCH_NOT_PREPARED}"
            ),
            &[("version", MEASURED_VERSION)],
        )),
    }
}

/// Runs the probe: `agy --gemini_dir=<dir> mcp list`, with the CLI's own update turned off for
/// the run.
fn probe_directory_flag(dir: &Path, shell_env: &HashMap<String, String>) -> FlagProbe {
    let flag = directory_argument(dir);
    let output = account_cli::query(
        AgentId::ANTIGRAVITY,
        &without_update(shell_env),
        None,
        &[flag.as_str(), "mcp", "list"],
        FLAG_PROBE_TIMEOUT,
        FLAG_PROBE_OUTPUT_LIMIT,
        None,
    );
    match output {
        Ok(output) if says_flag_undefined(output.status.code(), &output.stderr) => {
            FlagProbe::Undefined
        }
        _ => FlagProbe::NotRefused,
    }
}

/// Whether a run that ended with `status` and wrote `stderr` ended for the flag not being defined.
fn says_flag_undefined(status: Option<i32>, stderr: &[u8]) -> bool {
    status == Some(UNDEFINED_FLAG_STATUS)
        && String::from_utf8_lossy(stderr).contains(UNDEFINED_FLAG_LINE)
}

/// `shell_env` with the CLI's background update turned off, for a short run of it.
pub(super) fn without_update(shell_env: &HashMap<String, String>) -> HashMap<String, String> {
    let mut env = shell_env.clone();
    env.insert(NO_UPDATE_VAR.to_string(), "true".to_string());
    env
}

/// Keep the real HOME for the keychain and give the CLI a private directory through its flag.
/// The lock lives beside the probe's home so the home can be cleared after the child exits.
pub(super) fn quota_home(
    run_homes: &Path,
    shell_env: &HashMap<String, String>,
    wait: &account_cli::RunWait<'_>,
) -> Result<account_cli::RunHome> {
    use std::sync::atomic::Ordering;

    use crate::platform::fs::{create_private_dir, try_lock_file, Busy};

    let dir = super::quota::own_home(run_homes);
    let parent = dir.parent().context("the quota directory has no parent")?;
    create_private_dir(parent).context("creating the Antigravity quota directory")?;
    let lock_path = parent.join("probe.lock");
    let lock = loop {
        if Instant::now() >= wait.until
            || wait
                .cancel
                .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            anyhow::bail!("the Antigravity quota probe stopped waiting for its directory");
        }
        match try_lock_file(&lock_path) {
            Ok(lock) => break lock,
            Err(Busy::Held) => std::thread::sleep(crate::session::wait::POLL),
            Err(Busy::Open(err) | Busy::Lock(err)) => {
                return Err(err).context("locking the Antigravity quota directory")
            }
        }
    };
    crate::hostfs::remove_any(&dir).context("clearing the previous Antigravity quota probe")?;
    create_private_dir(&dir).context("creating the Antigravity quota home")?;

    let home = account_cli::snapshot_home(shell_env)
        .to_string_lossy()
        .into_owned();
    let cleanup = dir.clone();
    Ok(account_cli::RunHome {
        dir,
        work_in_home: false,
        env: vec![
            ("HOME".to_owned(), home),
            (NO_UPDATE_VAR.to_owned(), "true".to_owned()),
        ],
        after_run: Some(Box::new(move || {
            let _ = crate::hostfs::remove_any(&cleanup);
        })),
        _lock: Some(lock),
    })
}

/// The flag with the launch's directory joined to it.
pub(super) fn directory_argument(dir: &Path) -> String {
    format!("{DIRECTORY_FLAG}={}", dir.to_string_lossy())
}

/// Links every entry of `from` but `written` into `to`: a directory always, a file where the host
/// links files. Where it does not, a file is copied, read as a file that is not Octoboard's is
/// (a regular file, within a bound): one that is not a regular file, or is larger, is left out,
/// and the session runs without that entry of the user's directory.
fn link_entries(from: &Path, to: &Path, written: &[&str]) -> Result<()> {
    let entries = match std::fs::read_dir(from) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("reading {}", from.display())),
    };
    let file_links = crate::platform::fs::file_links_available();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if written.iter().any(|written| name == *written) {
            continue;
        }
        let at = to.join(&name);
        // The fully resolved target, so an entry that is itself a link is reached through one.
        let target = crate::hostfs::canonical(entry.path()).unwrap_or_else(|_| entry.path());
        let is_dir = target.is_dir();
        if is_dir || file_links {
            crate::hostfs::alias(&target, &at, is_dir)
                .with_context(|| format!("linking {}", at.display()))?;
        } else if let Ok(bytes) = crate::hostfs::read_regular(&target, COPIED_FILE_LIMIT) {
            write_private(&at, &bytes)?;
        }
    }
    Ok(())
}

/// Writes the launch's settings, `json`, to a file of the launch's directory, private to the
/// user, and returns its path for `--settings`, which takes a file as well as the JSON itself
/// (1.5.14). The user's own allow rules are in it, and a rule can hold a command line: in a file
/// they are in no process listing and do not count against the length of the arguments.
pub(super) fn write_settings(dir: &Path, json: &str) -> Result<PathBuf> {
    write_launch_file(dir, LAUNCH_SETTINGS_FILE, json.as_bytes())
}

/// Writes `bytes` as the file `name` of the launch's directory `dir`, private to the user, and
/// gives its path.
fn write_launch_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = dir.join(name);
    write_private(&path, bytes)?;
    Ok(path)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    replace_beside(path, bytes, Permissions::Private)
        .with_context(|| format!("writing {}", path.display()))
}

/// What a session of the CLI takes beyond JSON in the user's `mcp_config.json` (1.5.14, each
/// measured by whether the file's server was started): a byte order mark, `//` and `/* */`
/// comments and a comma before a closing brace. Its `mcp list` command is stricter and fails on
/// each of them; a `#` comment, and anything else, makes the session read no server from the file.
const MCP_CONFIG_TAKES: Takes = Takes {
    byte_order_mark: true,
    comments: true,
    trailing_commas: true,
    leading_comment_lines: false,
};

pub(super) fn read_mcp_config(path: &Path) -> Result<Map<String, Value>> {
    user_object(path, MCP_CONFIG_TAKES)
}

/// What a session takes beyond JSON in the user's `hooks.json` and in its own `settings.json`: a
/// byte order mark and nothing else (1.5.14). With a comment or a trailing comma it loads no hook
/// of the file (`failed to parse hooks.json`, `loaded 0 named hooks`), which fails the plan,
/// and runs on its default settings, which the launch does too ([`user_settings`]).
const BYTE_ORDER_MARK_ONLY: Takes = Takes {
    byte_order_mark: true,
    comments: false,
    trailing_commas: false,
    leading_comment_lines: false,
};

/// The JSON object the user's file at `path` holds, read as a session of the CLI reads that file
/// (`takes`): empty for a file that is missing or holds only white space, which the CLI reads as
/// nothing configured. Anything else that is not an object, and anything the CLI would not read,
/// fails the plan, so the session starts bare and the notice names the file: the CLI itself goes on
/// without the file and says so only in its log, and a launch of Octoboard's started without it
/// would run without the user's hooks or servers and say nothing.
fn user_object(path: &Path, takes: Takes) -> Result<Map<String, Value>> {
    let text = match read_regular_text(path, SMALL_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(err) => return Err(unusable(path, &err.to_string())),
    };
    if text.trim_start_matches('\u{FEFF}').trim().is_empty() {
        return Ok(Map::new());
    }
    match user_json::parse(&text, takes) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(unusable(path, "the file is not a JSON object")),
        Err(err) => Err(unusable(path, &err.to_string())),
    }
}

/// The text of the user's file at `path`, empty when there is none.
fn user_text(path: &Path) -> Result<String> {
    match read_regular_text(path, SMALL_FILE_LIMIT) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(unusable(path, &err.to_string())),
    }
}

fn unusable(path: &Path, why: &str) -> anyhow::Error {
    CodedError::raised(
        Code::of_provider(USER_CONFIG_UNUSABLE),
        format!(
            "Antigravity CLI's `{}` cannot be used ({why}), so {LAUNCH_NOT_PREPARED}: repair \
             the file or remove it",
            path.display()
        ),
        &[("path", &path.to_string_lossy())],
    )
}

/// `user`, the user's hooks, with Octoboard's added under a name of their own, after the user's
/// (an entry of the user's with that name is overwritten). The payload names its event in a field
/// of its own spelling, so each event's command is told its name.
fn with_hooks(mut user: Map<String, Value>, spec: &LaunchSpec<'_>) -> Value {
    let command = |event: &str| {
        hook_command(
            spec.self_exe,
            spec.session_id,
            spec.launch_id,
            spec.daemon_port,
            spec.key_file,
            Some(event),
        )
    };
    let entry = |event: &str| {
        json!({
            "type": "command",
            "command": command(event),
            "timeout": COMMAND_HOOK_TIMEOUT_SECS,
        })
    };
    let mut ours = Map::new();
    for event in TURN_EVENTS {
        ours.insert(event.to_string(), json!([entry(event)]));
    }
    for event in TOOL_EVENTS {
        let mut list = vec![json!({ "matcher": EVERY_TOOL, "hooks": [entry(event)] })];
        if let Some(guard) = spec.guard.as_ref().filter(|_| event == GUARD_EVENT) {
            list.push(json!({
                "matcher": FILE_WRITING_TOOLS.join("|"),
                "hooks": [{
                    "type": "command",
                    "command": guard_command(guard),
                    "timeout": GUARD_HOOK_TIMEOUT_SECS,
                }],
            }));
        }
        ours.insert(event.to_string(), Value::Array(list));
    }
    user.insert(HOOK_NAME.to_string(), Value::Object(ours));
    Value::Object(user)
}

/// `user`, the user's MCP configuration at `path`, with Octoboard's server added to its servers,
/// after the user's. The time limit on a call, in seconds, is above Octoboard's own forwarding
/// limit. A server of the user's under Octoboard's name is overwritten: the session's tools have
/// to be the session's. `mcpServers` that is there and is neither an object nor `null` (which the
/// CLI reads as no servers) fails the plan, as the rest of an unusable file does.
fn with_server(mut user: Map<String, Value>, path: &Path, spec: &LaunchSpec<'_>) -> Result<Value> {
    let (command, args) = mcp_server_command(spec);
    let mut servers = match user.remove("mcpServers") {
        Some(Value::Object(servers)) => servers,
        None | Some(Value::Null) => Map::new(),
        Some(_) => return Err(unusable(path, "`mcpServers` is not an object")),
    };
    servers.insert(
        session_tools::catalogue::SERVER_KEY.to_string(),
        json!({
            "command": command,
            "args": args,
            "timeoutSeconds": MCP_TOOL_TIMEOUT_SECS,
        }),
    );
    user.insert("mcpServers".to_string(), Value::Object(servers));
    Ok(Value::Object(user))
}

/// What a launch takes from the CLI's own settings file.
#[derive(Debug, PartialEq)]
enum UserSettings {
    /// The file parses and nothing Octoboard reads of it is of the wrong kind, or there is none:
    /// the user's own `permissions.allow` rules, none for a file that is missing or empty, or that
    /// names none, and the whole of `settings`, for the CLI to be asked whether it takes them
    /// ([`rejected_by_cli`]).
    Read {
        allow: Vec<Value>,
        settings: Map<String, Value>,
    },
    /// The CLI reads the file as nothing and runs on its defaults, saying so only in its log;
    /// `why` is what is wrong with it.
    NotRead { why: String },
}

/// The CLI's settings at `path`, read as a session of the CLI reads them (1.5.14): strict JSON
/// with a byte order mark taken off. A file that does not parse, a comment or a trailing comma
/// included, and one whose `permissions` is not an object, whose `permissions.allow`, `deny` or
/// `ask` is not a list of texts ([`PERMISSION_LISTS`]), or whose `trustedWorkspaces` is not a
/// list, make the CLI run on
/// its defaults for the whole file (measured), so the launch does the same
/// ([`UserSettings::NotRead`]) and repeats none of its rules: a launch that repeated them would
/// allow what the CLI's own session, run on its defaults, asks about. The CLI rejects the whole
/// file for more than those (any of the 39 keys it knows with a value it does not take): the rest
/// is the CLI's to say ([`rejected_by_cli`]), and those are what is told without it, where the
/// CLI cannot be asked.
///
/// A file that is there and cannot be read at all (a directory in its place, no permission)
/// still fails the plan: what the CLI does with one was not measured.
///
/// The file is read once, with no second look: the CLI, which writes it while its sessions run,
/// writes a file beside it and renames that over it (1.5.14), so a read finds the old file or the
/// new one whole.
fn user_settings(path: &Path) -> Result<UserSettings> {
    let nothing = || {
        Ok(UserSettings::Read {
            allow: Vec::new(),
            settings: Map::new(),
        })
    };
    let text = match read_regular_text(path, SMALL_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return nothing(),
        Err(err) => return Err(unusable(path, &err.to_string())),
    };
    if text.trim_start_matches('\u{FEFF}').trim().is_empty() {
        return nothing();
    }
    let not_read = |why: &str| {
        Ok(UserSettings::NotRead {
            why: why.to_string(),
        })
    };
    let settings = match user_json::parse(&text, BYTE_ORDER_MARK_ONLY) {
        Ok(Value::Object(settings)) => settings,
        Ok(_) => return not_read("the file is not a JSON object"),
        Err(err) => return not_read(&err.to_string()),
    };
    if !matches!(
        settings.get("trustedWorkspaces"),
        None | Some(Value::Null | Value::Array(_))
    ) {
        return not_read("`trustedWorkspaces` is not a list");
    }
    let allow = match settings.get("permissions") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Object(permissions)) => {
            for list in PERMISSION_LISTS {
                let texts = match permissions.get(list) {
                    None | Some(Value::Null) => true,
                    // A `null` element is taken, by the file's load and by `--settings` alike.
                    Some(Value::Array(rules)) => {
                        rules.iter().all(|rule| rule.is_string() || rule.is_null())
                    }
                    Some(_) => false,
                };
                if !texts {
                    return not_read(&format!("`permissions.{list}` is not a list of texts"));
                }
            }
            match permissions.get("allow") {
                Some(Value::Array(rules)) => rules.clone(),
                _ => Vec::new(),
            }
        }
        Some(_) => return not_read("`permissions` is not an object"),
    };
    Ok(UserSettings::Read { allow, settings })
}

/// What the CLI said of settings given to it with `--settings`.
#[derive(Clone, Debug, PartialEq)]
enum SettingsAnswer {
    /// It took them.
    Taken,
    /// It does not know this key, and looked no further.
    UnknownKey(String),
    /// This key has a value it does not take.
    InvalidValue(String),
    /// Anything else: the run was not made, timed out, or ended in a way that says nothing of the
    /// settings.
    Nothing,
}

/// Why the CLI reads the user's `settings`, the object their settings file holds, as nothing, or
/// `None` where it reads them, or cannot be asked. The CLI rejects the whole file for any key it
/// knows whose value it does not take (1.5.14: 39 keys, several taking only a few words, so the
/// rule is the CLI's own and is not repeated here), and says so only in the log of a session. So
/// it is asked: `agy --gemini_dir=<dir> --settings <a copy> mcp list` validates the copy the same
/// way and ends at once, with no sign-in and no network, and writes nothing (measured key by key
/// against the file's own load, eight kinds of value each). `dir` is the launch's own directory,
/// so the run reads and migrates nothing of the user's home.
///
/// `--settings` differs from the file in two ways, both taken out of the question. A key that is
/// `null` is unset in the file, while `--settings` refuses it for some keys: such keys are left
/// out. A key the CLI does not know is passed over in the file, while `--settings` refuses it
/// before it looks at any value: it is taken out and the question asked again, once for each key
/// of the file at most.
///
/// A CLI that cannot be asked (not found, timed out, an answer of another shape) says nothing,
/// and the file counts as read: what [`user_settings`] tells by itself still stands.
///
/// A launch reuses the answer in [`SETTINGS_ANSWERS`] while it is kept. A `Nothing` answer
/// expires after [`PASSING_ANSWER_KEPT`], so a later launch asks the CLI again.
fn rejected_by_cli(
    dir: &Path,
    settings: &Map<String, Value>,
    spec: &LaunchSpec<'_>,
) -> Option<String> {
    let mut asked: Map<String, Value> = settings
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let question = dir.join(SETTINGS_QUESTION_FILE);
    let directory = directory_argument(dir);
    let env = without_update(spec.shell_env);
    let binary = probe_key(spec);
    let mut rejected = None;
    for _ in 0..=settings.len() {
        if asked.is_empty() {
            break;
        }
        let text = Value::Object(asked.clone()).to_string();
        let key = (binary.clone(), Sha256::digest(text.as_bytes()).into());
        let answer = kept_or_asked(
            &SETTINGS_ANSWERS,
            key,
            Instant::now(),
            SettingsAnswer::kept_for_good,
            || {
                if write_private(&question, text.as_bytes()).is_err() {
                    return SettingsAnswer::Nothing;
                }
                let named = question.to_string_lossy();
                let output = account_cli::query(
                    AgentId::ANTIGRAVITY,
                    &env,
                    None,
                    &[directory.as_str(), "--settings", &*named, "mcp", "list"],
                    FLAG_PROBE_TIMEOUT,
                    FLAG_PROBE_OUTPUT_LIMIT,
                    None,
                );
                output.map_or(SettingsAnswer::Nothing, |output| {
                    settings_answer(output.status.code(), &output.stderr)
                })
            },
        );
        match answer {
            SettingsAnswer::UnknownKey(key) if asked.remove(&key).is_some() => {}
            SettingsAnswer::InvalidValue(key) => {
                rejected = Some(format!("`{key}` has a value the CLI does not take"));
                break;
            }
            _ => break,
        }
    }
    // Best effort: the file is private to the user and goes with the launch's directory.
    let _ = std::fs::remove_file(&question);
    rejected
}

impl SettingsAnswer {
    /// Whether the answer is kept for good ([`SETTINGS_ANSWERS`]): every answer that says something
    /// of the settings, and not one that says nothing.
    fn kept_for_good(&self) -> bool {
        *self != SettingsAnswer::Nothing
    }
}

/// What a run that ended with `status` and wrote `stderr` said of the settings it was given.
fn settings_answer(status: Option<i32>, stderr: &[u8]) -> SettingsAnswer {
    match status {
        Some(0) => return SettingsAnswer::Taken,
        Some(SETTINGS_REFUSED_STATUS) => {}
        _ => return SettingsAnswer::Nothing,
    }
    match settings_refused(&String::from_utf8_lossy(stderr)) {
        Some(SettingsRefused::UnknownKey(key)) => SettingsAnswer::UnknownKey(key),
        Some(SettingsRefused::InvalidValue(key)) => SettingsAnswer::InvalidValue(key),
        None => SettingsAnswer::Nothing,
    }
}

/// How the CLI refused settings given with `--settings`, by the key it named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SettingsRefused {
    /// It does not know this key.
    UnknownKey(String),
    /// This key has a value it does not take.
    InvalidValue(String),
}

impl SettingsRefused {
    /// The key the CLI named.
    pub(super) fn key(&self) -> &str {
        match self {
            SettingsRefused::UnknownKey(key) | SettingsRefused::InvalidValue(key) => key,
        }
    }
}

/// The refusal of `--settings` that `output`, what the CLI wrote, holds: a line that starts with
/// one of the two refusal lines (1.5.14), the key after it as a quoted text. The same line ends
/// a short run of the CLI and an interactive launch, before the launch draws anything (measured:
/// `testdata/antigravity_exit_settings_*.bin`). `None` for output with neither line, a
/// `--settings` file that could not be read among it.
pub(super) fn settings_refused(output: &str) -> Option<SettingsRefused> {
    // The key is written as a quoted text, which reads as a JSON one; what follows it on the
    // line of an invalid value is the CLI's own account of the value.
    let key_after = |start: &str| {
        let rest = output
            .lines()
            .find_map(|line| line.trim_start().strip_prefix(start))?;
        serde_json::Deserializer::from_str(rest)
            .into_iter::<String>()
            .next()?
            .ok()
    };
    if let Some(key) = key_after(SETTINGS_UNKNOWN_KEY_LINE) {
        Some(SettingsRefused::UnknownKey(key))
    } else {
        key_after(SETTINGS_INVALID_VALUE_LINE).map(SettingsRefused::InvalidValue)
    }
}

/// `user`, the user's global rule, with the session's role text after it.
fn with_role(user: String, spec: &LaunchSpec<'_>) -> String {
    let role = session_tools::role::role_description(
        spec.kind,
        spec.owner.as_ref(),
        AgentId::ANTIGRAVITY,
        &spec.role_options(),
    );
    if user.trim().is_empty() {
        role
    } else {
        format!("{}\n\n{role}", user.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::adapter::antigravity::tests::{isolated, user_gemini};
    use crate::adapter::tests::SpecFixture;
    use crate::adapter::{GuardHook, COMMAND_HOOK_TIMEOUT_SECS};

    /// Where a test's link of the home's migration marker leads, given the fixture whose home it
    /// is in.
    #[cfg(unix)]
    type LinkTarget<'a> = &'a dyn Fn(&SpecFixture) -> PathBuf;

    fn user_config(fixture: &SpecFixture) -> PathBuf {
        user_gemini(fixture).join("config")
    }

    /// Writes a file of the user's, making its directory.
    fn user_file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directory");
        std::fs::write(path, text).expect("file");
    }

    fn json_of(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).expect("file")).expect("JSON")
    }

    #[test]
    fn login_keeps_completed_onboarding_and_trusts_its_workspace() {
        let fixture = crate::test_support::ScratchDir::new("antigravity-login-home");
        let account = fixture.join("account");
        let workspace = fixture.join("login");
        let record = account.join(APP_DATA_DIR).join(ONBOARDING_RECORD);
        let prior = json!({
            "consumerOnboardingComplete": true,
            "enterpriseOnboardingComplete": false,
            "onboardingComplete": true,
            "other": "not imported",
        });
        user_file(&record, &prior.to_string());

        let home = prepare_login_home(&workspace, &account).unwrap();
        let data = home.join(APP_DATA_DIR);
        assert_eq!(
            json_of(&data.join(SETTINGS_FILE)),
            json!({"trustedWorkspaces": [workspace.to_string_lossy()]}),
        );
        assert_eq!(
            json_of(&data.join(ONBOARDING_RECORD)),
            json!({
                "consumerOnboardingComplete": true,
                "enterpriseOnboardingComplete": false,
                "onboardingComplete": true,
            }),
        );
        assert_eq!(json_of(&record), prior);
    }

    fn hooks_of(launch: &LaunchDir) -> Value {
        json_of(&launch.dir.join("config").join("hooks.json"))
    }

    fn servers_of(launch: &LaunchDir) -> Value {
        json_of(&launch.dir.join("config").join("mcp_config.json"))
    }

    fn rule_of(launch: &LaunchDir) -> String {
        std::fs::read_to_string(launch.dir.join("GEMINI.md")).expect("rule")
    }

    fn guard<'a>(list: &'a Path, script: &'a Path) -> GuardHook<'a> {
        GuardHook {
            exe: "/opt/octoboard/octoboardd",
            list,
            script,
        }
    }

    /// The user's own hook, MCP server and rule survive in the launch's copies, ahead of
    /// Octoboard's, and everything else the user's files hold with them; Octoboard's hooks sit
    /// under a name of their own, each event's command told its name, and its server carries the
    /// time limit above Octoboard's forwarding limit.
    #[test]
    fn the_three_files_hold_the_users_content_first_and_octoboards_after_it() {
        let fixture = isolated();
        let user_hooks = json!({
            "my-hook": { "SessionStart": [{ "type": "command", "command": "echo mine" }] }
        });
        user_file(
            &user_config(&fixture).join("hooks.json"),
            &user_hooks.to_string(),
        );
        user_file(
            &user_config(&fixture).join("mcp_config.json"),
            r#"{"mcpServers":{"mine":{"command":"my-server","args":["--x"]}},"other":1}"#,
        );
        user_file(&user_gemini(&fixture).join("GEMINI.md"), "my rule\n");
        let role_only = {
            let bare = isolated();
            rule_of(&build(&bare.spec()).expect("built"))
        };
        assert!(!role_only.trim().is_empty());

        let spec = fixture.spec();
        let launch = build(&spec).expect("built");

        let hooks = hooks_of(&launch);
        assert_eq!(hooks["my-hook"], user_hooks["my-hook"]);
        assert_eq!(
            hooks.as_object().expect("hooks").keys().collect::<Vec<_>>(),
            ["my-hook", "octoboard-session"]
        );
        let ours = hooks["octoboard-session"].as_object().expect("our hooks");
        assert_eq!(
            ours.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "SessionStart",
                "PreInvocation",
                "Stop",
                "PreToolUse",
                "PostToolUse"
            ]
        );
        let entry = |event: &str| {
            json!({
                "type": "command",
                "command": crate::adapter::hook_command(
                    spec.self_exe, spec.session_id, spec.launch_id, spec.daemon_port, spec.key_file, Some(event)
                ),
                "timeout": COMMAND_HOOK_TIMEOUT_SECS,
            })
        };
        for event in ["SessionStart", "PreInvocation", "Stop"] {
            assert_eq!(ours[event], json!([entry(event)]), "{event}");
        }
        // A tool event runs only an entry with a matcher: `*` for every tool.
        for event in ["PreToolUse", "PostToolUse"] {
            assert_eq!(
                ours[event],
                json!([{ "matcher": "*", "hooks": [entry(event)] }]),
                "{event}"
            );
        }
        assert_ne!(ours["SessionStart"], ours["Stop"]);

        let servers = servers_of(&launch);
        assert_eq!(servers["other"], 1);
        assert_eq!(
            servers["mcpServers"]
                .as_object()
                .expect("servers")
                .keys()
                .collect::<Vec<_>>(),
            ["mine", "octoboard"]
        );
        assert_eq!(
            servers["mcpServers"]["mine"],
            json!({ "command": "my-server", "args": ["--x"] })
        );
        let (command, args) = crate::adapter::mcp_server_command(&spec);
        assert_eq!(
            servers["mcpServers"]["octoboard"],
            json!({
                "command": command,
                "args": args,
                "timeoutSeconds": crate::adapter::MCP_TOOL_TIMEOUT_SECS,
            })
        );
        assert!(
            servers["mcpServers"]["octoboard"]["timeoutSeconds"]
                .as_u64()
                .expect("a number")
                > crate::session_tools::CALL_TIMEOUT.as_secs()
        );

        let rule = rule_of(&launch);
        assert!(rule.starts_with("my rule"), "{rule}");
        assert!(rule.ends_with(role_only.trim_end()), "{rule}");
    }

    /// Building again over the same scratch directory leaves the same files: Octoboard's entries
    /// are not added twice, and the user's are not repeated.
    #[test]
    fn building_twice_does_not_duplicate_octoboards_entries() {
        let fixture = isolated();
        user_file(
            &user_config(&fixture).join("hooks.json"),
            r#"{"my-hook":{"Stop":[{"type":"command","command":"echo mine"}]}}"#,
        );
        user_file(&user_gemini(&fixture).join("GEMINI.md"), "my rule");
        let read = |launch: &LaunchDir| (hooks_of(launch), servers_of(launch), rule_of(launch));
        let first = read(&build(&fixture.spec()).expect("built"));
        let second = read(&build(&fixture.spec()).expect("built twice"));
        assert_eq!(first, second);
        assert_eq!(first.0.as_object().expect("hooks").len(), 2);
        assert_eq!(
            first.0["my-hook"]["Stop"].as_array().expect("list").len(),
            1
        );
        assert_eq!(first.1["mcpServers"].as_object().expect("servers").len(), 1);
        assert_eq!(first.2.matches("my rule").count(), 1);
    }

    /// The cross-project guard is registered for the two tools that write a file, with its own
    /// command and timeout, only where the launch has a guard: as an entry of its own after
    /// Octoboard's hook in `PreToolUse`'s list, neither replacing the other. `PostToolUse` never
    /// has it.
    #[test]
    fn the_guard_is_registered_only_where_the_launch_has_one() {
        let fixture = isolated();
        let list = fixture.scratch.join("guard-list");
        let script = fixture.scratch.join("guard-script");
        let guarded = guard(&list, &script);
        let mut spec = fixture.spec();
        spec.guard = Some(guarded);
        let hooks = hooks_of(&build(&spec).expect("built"));
        let ours = hooks["octoboard-session"].as_object().expect("our hooks");
        let pre = ours["PreToolUse"].as_array().expect("a list");
        assert_eq!(pre.len(), 2, "{pre:?}");
        assert_eq!(pre[0]["matcher"], "*");
        assert_eq!(
            pre[1],
            json!({
                "matcher": "write_file|edit_file",
                "hooks": [{
                    "type": "command",
                    "command": crate::adapter::guard_command(&guarded),
                    "timeout": crate::adapter::GUARD_HOOK_TIMEOUT_SECS,
                }],
            })
        );
        assert_eq!(ours["PostToolUse"].as_array().expect("a list").len(), 1);

        spec.guard = None;
        let hooks = hooks_of(&build(&spec).expect("built without a guard"));
        let ours = hooks["octoboard-session"].as_object().expect("our hooks");
        let pre = ours["PreToolUse"].as_array().expect("a list");
        assert_eq!(pre.len(), 1, "{pre:?}");
        assert_eq!(pre[0]["matcher"], "*");
    }

    /// Octoboard's hook is registered for every event something of the provider reads
    /// (`super::super::hooks::EVENTS_READ`: the interpreter, the background work and the pending
    /// interactions), so that no reader waits on an event a real session never posts; a tool
    /// event with an entry that matches every tool, since a plain entry there never runs
    /// (`docs/conventions/tests.md`: the registration holds each event the interpreter reads).
    #[test]
    fn the_hook_is_registered_for_every_event_read() {
        let fixture = isolated();
        let spec = fixture.spec();
        let hooks = hooks_of(&build(&spec).expect("built"));
        let ours = hooks["octoboard-session"].as_object().expect("our hooks");
        for event in super::super::hooks::EVENTS_READ {
            let command = crate::adapter::hook_command(
                spec.self_exe,
                spec.session_id,
                spec.launch_id,
                spec.daemon_port,
                spec.key_file,
                Some(event),
            );
            let list = ours
                .get(event)
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("{event} is not registered: {ours:?}"));
            let tool_event = matches!(event, "PreToolUse" | "PostToolUse");
            let runs_ours = list.iter().any(|entry| match entry.get("matcher") {
                Some(matcher) => {
                    (!tool_event || matcher == "*")
                        && entry["hooks"]
                            .as_array()
                            .is_some_and(|hooks| hooks.iter().any(|h| h["command"] == command))
                }
                None => !tool_event && entry["command"] == command,
            });
            assert!(runs_ours, "{event}: {list:?}");
        }
    }

    /// Every entry of the user's `.gemini` and of its `config` but the three files is a link, so
    /// what the session saves is saved where the user's other sessions find it; the three are
    /// the launch's own copies, private to the user, and the user's own files are left as they
    /// were.
    #[test]
    fn every_other_entry_is_a_link_and_the_three_files_are_private_copies() {
        let fixture = isolated();
        let gemini = user_gemini(&fixture);
        let config = user_config(&fixture);
        user_file(&gemini.join("antigravity-cli").join("settings.json"), "{}");
        user_file(&gemini.join("other.txt"), "other");
        user_file(&config.join("config.json"), "{\"a\":1}");
        user_file(&config.join("projects").join("p.json"), "{}");
        let user_hooks = r#"{"my-hook":{}}"#;
        user_file(&config.join("hooks.json"), user_hooks);
        user_file(&config.join("mcp_config.json"), "{}");
        user_file(&gemini.join("GEMINI.md"), "my rule");

        let launch = build(&fixture.spec()).expect("built");

        // A change through the launch's entry is the user's own entry changed.
        let through = |relative: &str| launch.dir.join(relative);
        std::fs::write(through("antigravity-cli/new.json"), "saved").expect("saved through");
        assert_eq!(
            std::fs::read_to_string(gemini.join("antigravity-cli").join("new.json")).expect("file"),
            "saved"
        );
        assert_eq!(
            std::fs::read_to_string(through("antigravity-cli/settings.json")).expect("file"),
            "{}"
        );
        std::fs::write(through("config/projects/q.json"), "saved").expect("saved through");
        assert!(config.join("projects").join("q.json").is_file());
        for (relative, text) in [("other.txt", "other"), ("config/config.json", "{\"a\":1}")] {
            assert_eq!(
                std::fs::read_to_string(through(relative)).expect("file"),
                text,
                "{relative}"
            );
            if crate::platform::fs::file_links_available() {
                std::fs::write(through(relative), "changed").expect("write through");
                let user = gemini.join(relative);
                assert_eq!(
                    std::fs::read_to_string(&user).expect("file"),
                    "changed",
                    "{relative} is not a link"
                );
            }
        }

        // The three are written for the launch, not links, and the user's own are untouched.
        // So is the launch's own settings file, once written.
        let launch_settings = write_settings(&launch.dir, "{}").expect("settings written");
        assert_eq!(launch_settings.parent(), Some(launch.dir.as_path()));
        assert!(!gemini
            .join(launch_settings.file_name().expect("a name"))
            .exists());
        for relative in [
            "config/hooks.json",
            "config/mcp_config.json",
            "GEMINI.md",
            "octoboard-launch-settings.json",
        ] {
            let path = through(relative);
            assert!(
                !std::fs::symlink_metadata(&path)
                    .expect("entry")
                    .file_type()
                    .is_symlink(),
                "{relative}"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&path)
                    .expect("entry")
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600, "{relative}");
            }
        }
        assert_eq!(
            std::fs::read_to_string(config.join("hooks.json")).expect("file"),
            user_hooks
        );
        assert_eq!(
            std::fs::read_to_string(config.join("mcp_config.json")).expect("file"),
            "{}"
        );
        assert_eq!(
            std::fs::read_to_string(gemini.join("GEMINI.md")).expect("file"),
            "my rule"
        );
    }

    /// With none of the user's files there, each counts as empty: the launch holds Octoboard's
    /// alone, and the CLI's own data directory is made under the user's home when missing, which
    /// the launch's `antigravity-cli` leads to.
    #[test]
    fn missing_user_files_count_as_empty_and_the_data_directory_is_made() {
        let fixture = isolated();
        assert!(!user_gemini(&fixture).exists());
        let launch = build(&fixture.spec()).expect("built");
        assert!(user_gemini(&fixture).join("antigravity-cli").is_dir());
        assert!(launch.dir.join("antigravity-cli").is_dir());

        let hooks = hooks_of(&launch);
        assert_eq!(
            hooks.as_object().expect("hooks").keys().collect::<Vec<_>>(),
            ["octoboard-session"]
        );
        let servers = servers_of(&launch);
        assert_eq!(
            servers["mcpServers"]
                .as_object()
                .expect("servers")
                .keys()
                .collect::<Vec<_>>(),
            ["octoboard"]
        );
        assert!(!rule_of(&launch).trim().is_empty());
    }

    /// What the CLI reads as nothing configured (an empty file, white space, `{}`, a null list of
    /// servers) is accepted, and so is a byte order mark in front of strict JSON, which a session
    /// was measured to read in both files though the CLI's `mcp list` command fails on one.
    #[test]
    fn what_a_session_reads_as_empty_and_a_byte_order_mark_are_accepted() {
        for file in ["hooks.json", "mcp_config.json"] {
            for text in ["", "  \n\t ", "{}", r#"{"mcpServers":null}"#] {
                let fixture = isolated();
                user_file(&user_config(&fixture).join(file), text);
                let launch = build(&fixture.spec())
                    .unwrap_or_else(|err| panic!("{file} {text:?} refused: {err}"));
                assert!(
                    servers_of(&launch)["mcpServers"]["octoboard"].is_object(),
                    "{file} {text:?}"
                );
                assert!(
                    hooks_of(&launch)["octoboard-session"].is_object(),
                    "{file} {text:?}"
                );
            }
        }
        let fixture = isolated();
        user_file(
            &user_config(&fixture).join("mcp_config.json"),
            "\u{FEFF}{\"mcpServers\":{\"mine\":{\"command\":\"x\"}}}",
        );
        user_file(
            &user_config(&fixture).join("hooks.json"),
            "\u{FEFF}{\"my-hook\":{}}",
        );
        let launch = build(&fixture.spec()).expect("a byte order mark is taken off");
        assert!(servers_of(&launch)["mcpServers"]["mine"].is_object());
        assert!(servers_of(&launch)["mcpServers"]["octoboard"].is_object());
        assert!(hooks_of(&launch)["my-hook"].is_object());
    }

    /// A file of the user's that is there and that a session of the CLI would not read refuses the
    /// launch with the provider's code naming the file, and leaves nothing written: an array, a
    /// bare value, a `#` comment, a file that cannot be read (a directory stands in for one), in
    /// `hooks.json` a comment or a trailing comma as well, and in the MCP file a list of servers
    /// that is neither an object nor null. The CLI's settings file refuses only when it cannot be
    /// read at all.
    #[test]
    fn an_unusable_user_file_refuses_the_launch_and_leaves_nothing_written() {
        let neither = ["[]", r#""text""#, "42", "# a comment\n{}", "{ not json"];
        let beyond_json = ["{ // a comment\n}", "/* a comment */ {}", r#"{"a":1,}"#];
        for file in ["hooks.json", "mcp_config.json"] {
            let refused_texts: Vec<&str> = if file == "hooks.json" {
                neither.iter().chain(&beyond_json).copied().collect()
            } else {
                neither.to_vec()
            };
            for text in refused_texts {
                let fixture = isolated();
                let path = user_config(&fixture).join(file);
                user_file(&path, text);
                refused(&fixture, &path, &format!("{file} {text:?}"));
            }
            let fixture = isolated();
            let path = user_config(&fixture).join(file);
            std::fs::create_dir_all(&path).expect("a directory in the file's place");
            refused(&fixture, &path, &format!("{file} unreadable"));
        }
        let fixture = isolated();
        let path = user_gemini(&fixture).join("GEMINI.md");
        std::fs::create_dir_all(&path).expect("a directory in the file's place");
        refused(&fixture, &path, "GEMINI.md unreadable");

        for text in [
            r#"{"mcpServers":[]}"#,
            r#"{"mcpServers":"x"}"#,
            r#"{"mcpServers":5}"#,
            r#"{"mcpServers":true}"#,
        ] {
            let fixture = isolated();
            let path = user_config(&fixture).join("mcp_config.json");
            user_file(&path, text);
            refused(&fixture, &path, &format!("mcp_config.json {text:?}"));
        }

        // The CLI's settings file refuses only where it cannot be read at all.
        let fixture = isolated();
        let path = user_settings_file(&fixture);
        std::fs::create_dir_all(&path).expect("a directory in the file's place");
        refused(&fixture, &path, "settings.json unreadable");
    }

    fn user_settings_file(fixture: &SpecFixture) -> PathBuf {
        user_gemini(fixture)
            .join("antigravity-cli")
            .join("settings.json")
    }

    /// A settings file the CLI reads as nothing (1.5.14: one that does not parse, a comment or a
    /// trailing comma included, one whose `permissions` is not an object, whose `allow`, `deny` or
    /// `ask` in it is not a list or holds an element that is neither a text nor `null`, and one
    /// whose `trustedWorkspaces` is not a list) does not refuse the launch: the launch runs on the
    /// defaults as the CLI does, repeating none of the file's allow rules, even those of a file
    /// whose rules are a list, and carries the notice that names the file. The file is left as it
    /// was.
    #[test]
    fn a_settings_file_the_cli_does_not_read_starts_the_launch_on_the_defaults_with_a_notice() {
        for text in [
            "{ not json",
            "[]",
            "42",
            "// mine\n{\"permissions\":{\"allow\":[\"Bash(ls:*)\"]}}",
            r#"{"permissions":{"allow":["Bash(ls:*)"]},}"#,
            r#"{"permissions":[]}"#,
            r#"{"permissions":"allow"}"#,
            r#"{"permissions":{"allow":"mcp(x/*)"}}"#,
            r#"{"permissions":{"allow":{"a":1}}}"#,
            r#"{"permissions":{"allow":["Bash(ls:*)"]},"trustedWorkspaces":"/a"}"#,
            r#"{"trustedWorkspaces":{"/a":true}}"#,
            // Each rule list is a list of texts, or the CLI rejects the file (measured; a `null`
            // element is taken, and is in the test of a file the CLI reads).
            r#"{"permissions":{"allow":[1]}}"#,
            r#"{"permissions":{"allow":["Bash(ls:*)"],"deny":"Bash(rm:*)"}}"#,
            r#"{"permissions":{"allow":["Bash(ls:*)"],"deny":[true]}}"#,
            r#"{"permissions":{"allow":["Bash(ls:*)"],"ask":{}}}"#,
        ] {
            let fixture = isolated();
            let path = user_settings_file(&fixture);
            user_file(&path, text);
            let launch =
                build(&fixture.spec()).unwrap_or_else(|err| panic!("{text:?} refused: {err}"));
            assert_eq!(launch.user_allow, Vec::<Value>::new(), "{text:?}");
            let notice =
                settings_notice(&fixture.spec()).unwrap_or_else(|| panic!("{text:?}: no notice"));
            assert_eq!(
                notice.code,
                Code::of_provider(SETTINGS_NOT_READ),
                "{text:?}"
            );
            assert_eq!(
                notice.params.get("path").map(String::as_str),
                Some(path.to_string_lossy().as_ref()),
                "{text:?}"
            );
            assert!(
                notice.message.contains(path.to_string_lossy().as_ref()),
                "{text:?}: {}",
                notice.message
            );
            assert_eq!(std::fs::read_to_string(&path).expect("file"), text);
        }
    }

    /// A settings file the CLI reads carries no notice, whatever else it holds: none, an empty one,
    /// one with a byte order mark, one with keys Octoboard does not read and lists of the right
    /// kind.
    #[test]
    fn a_settings_file_the_cli_reads_carries_no_notice() {
        for text in [
            None,
            Some(""),
            Some("{}"),
            Some("\u{FEFF}{\"permissions\":{\"allow\":[\"Bash(ls:*)\"]}}"),
            Some(r#"{"permissions":null,"trustedWorkspaces":null}"#),
            Some(r#"{"trustedWorkspaces":["/a"],"unknown":{"a":1},"modelProvider":"gemini"}"#),
            Some(r#"{"permissions":{"allow":[],"deny":["Bash(rm:*)"],"ask":null,"other":1}}"#),
            // A `null` element is taken too (measured), in each of the three lists.
            Some(r#"{"permissions":{"allow":["Bash(ls:*)",null],"deny":[null],"ask":["x",null]}}"#),
        ] {
            let fixture = isolated();
            if let Some(text) = text {
                user_file(&user_settings_file(&fixture), text);
            }
            build(&fixture.spec()).expect("built");
            assert!(settings_notice(&fixture.spec()).is_none(), "{text:?}");
        }
    }

    /// `rules` or `plugins` left in the data directory with something in them, in a home that has
    /// its marker, are told of at the launch, which goes on: the notice names the data directory
    /// and the configuration directory. An empty one, the link the migration leaves, `skills` (still
    /// loaded) and `workflows` (nothing known) are not told of, and a home with none has no notice.
    /// It is a further notice of the launch, after the settings file's.
    #[test]
    fn rules_and_plugins_left_in_the_data_directory_are_told_of_at_the_launch() {
        let migrated = || {
            let fixture = isolated();
            user_file(&user_config(&fixture).join(".migrated"), "");
            fixture
        };
        let data = |fixture: &SpecFixture| user_gemini(fixture).join("antigravity-cli");

        for name in ["rules", "plugins"] {
            let fixture = migrated();
            user_file(&data(&fixture).join(name).join("mine.md"), "mine");
            assert!(
                build(&fixture.spec()).is_ok() && settings_notice(&fixture.spec()).is_none(),
                "{name}"
            );
            let notice =
                leftovers_notice(&fixture.spec()).unwrap_or_else(|| panic!("{name}: no notice"));
            assert_eq!(
                notice.code,
                Code::of_provider(EARLIER_LAYOUT_LEFTOVERS_NOT_READ),
                "{name}"
            );
            assert_eq!(
                notice.params.get("path").map(String::as_str),
                Some(data(&fixture).to_string_lossy().as_ref()),
                "{name}"
            );
            assert_eq!(
                notice.params.get("current_path").map(String::as_str),
                Some(user_config(&fixture).to_string_lossy().as_ref()),
                "{name}"
            );
            // The one that is left is named, and the other is not.
            assert_eq!(
                notice.params.get("left").map(String::as_str),
                Some(name),
                "{name}"
            );
            let other = if name == "rules" { "plugins" } else { "rules" };
            assert!(
                notice.message.contains(&format!("`{name}`"))
                    && !notice.message.contains(&format!("`{other}`")),
                "{name}: {}",
                notice.message
            );
        }

        // Both left: the notice says so, and names both.
        let fixture = migrated();
        for name in ["rules", "plugins"] {
            user_file(&data(&fixture).join(name).join("mine.md"), "mine");
        }
        let notice = leftovers_notice(&fixture.spec()).expect("a notice for both");
        assert_eq!(notice.params.get("left").map(String::as_str), Some("both"));
        assert!(
            notice.message.contains("`rules`") && notice.message.contains("`plugins`"),
            "{}",
            notice.message
        );
        // One left with something in it beside an empty other: only the first is named.
        let fixture = migrated();
        user_file(&data(&fixture).join("plugins").join("mine.md"), "mine");
        std::fs::create_dir_all(data(&fixture).join("rules")).expect("an empty directory");
        let notice = leftovers_notice(&fixture.spec()).expect("a notice for the one");
        assert_eq!(
            notice.params.get("left").map(String::as_str),
            Some("plugins")
        );

        for name in ["skills", "workflows"] {
            let fixture = migrated();
            user_file(&data(&fixture).join(name).join("mine.md"), "mine");
            assert!(leftovers_notice(&fixture.spec()).is_none(), "{name}");
        }
        let fixture = migrated();
        std::fs::create_dir_all(data(&fixture).join("rules")).expect("an empty directory");
        assert!(leftovers_notice(&fixture.spec()).is_none());
        assert!(leftovers_notice(&migrated().spec()).is_none());

        // What the migration leaves where it moved the rules: a link to the ones of `config`.
        #[cfg(unix)]
        {
            let fixture = migrated();
            let moved = user_config(&fixture).join("rules");
            user_file(&moved.join("mine.md"), "mine");
            std::fs::create_dir_all(data(&fixture)).expect("data directory");
            std::os::unix::fs::symlink(&moved, data(&fixture).join("rules")).expect("link");
            assert!(leftovers_notice(&fixture.spec()).is_none());
        }
    }

    /// Makes the fixture's `agy` a stand-in for the CLI's reading of `--settings` (1.5.14): given
    /// a file there, it refuses the key `mine` as one it does not know, before any value, then a
    /// `toolPermission` that is `nope` as a value it does not take, and takes anything else; run
    /// any other way it ends with `otherwise`. Returns the file it appends each settings text it
    /// was given to, a line each.
    #[cfg(unix)]
    fn answering_settings(fixture: &mut SpecFixture, otherwise: i32) -> PathBuf {
        let home = fixture.scratch.join("home");
        let asked = home.join("settings-asked");
        let body = format!(
            "[ \"$2\" = --settings ] || exit {otherwise}\n\
             cat \"$3\" >> '{asked}'; echo >> '{asked}'\n\
             if grep -q '\"mine\"' \"$3\"; then\n\
             echo 'Error: --settings: unknown setting \"mine\"' >&2; exit 1\n\
             fi\n\
             if grep -q '\"toolPermission\":\"nope\"' \"$3\"; then\n\
             echo 'Error: --settings: invalid value for setting \"toolPermission\": toolPermission: unrecognized value \"nope\"' >&2\n\
             exit 1\n\
             fi\n\
             exit 0\n",
            asked = asked.display()
        );
        fixture.shell_env = crate::test_support::fake_agent(&home, "agy", &body);
        asked
    }

    /// The settings texts the stand-in was given, in order.
    #[cfg(unix)]
    fn asked_of(asked: &Path) -> Vec<Value> {
        std::fs::read_to_string(asked)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("the settings asked about"))
            .collect()
    }

    /// The CLI rejects the whole settings file for any key it knows whose value it does not take,
    /// so the CLI is asked: a file it rejects starts the launch on the defaults with the notice,
    /// none of the file's allow rules repeated. It is asked with the keys that are `null` left
    /// out, and again without a key it does not know, which the file's own reading passes over.
    /// The copy it is asked with does not outlive the question.
    #[cfg(unix)]
    #[test]
    fn a_settings_file_the_cli_itself_rejects_is_not_read() {
        let mut fixture = isolated();
        let asked = answering_settings(&mut fixture, 0);
        let path = user_settings_file(&fixture);
        user_file(
            &path,
            r#"{"mine":1,"permissions":{"allow":["Bash(ls:*)"],"deny":["Bash(rm:*)"]},"toolPermission":"nope","agentMode":null}"#,
        );
        let launch = build(&fixture.spec()).expect("built");

        assert_eq!(launch.user_allow, Vec::<Value>::new());
        let notice = settings_notice(&fixture.spec()).expect("a notice");
        assert_eq!(notice.code, Code::of_provider(SETTINGS_NOT_READ));
        assert!(notice.standing, "the file is not read for the whole launch");
        assert_eq!(
            notice.params.get("path").map(String::as_str),
            Some(path.to_string_lossy().as_ref())
        );
        assert!(
            notice.message.contains("`toolPermission`"),
            "{}",
            notice.message
        );
        let permissions = json!({ "allow": ["Bash(ls:*)"], "deny": ["Bash(rm:*)"] });
        assert_eq!(
            asked_of(&asked),
            [
                json!({ "mine": 1, "permissions": permissions.clone(), "toolPermission": "nope" }),
                json!({ "permissions": permissions, "toolPermission": "nope" }),
            ]
        );
        assert!(!launch.dir.join(SETTINGS_QUESTION_FILE).exists());

        // The notice keeps the reason the launch found: the file repaired since is not read
        // again, and the CLI is not asked again.
        user_file(&path, r#"{"permissions":{"allow":["Bash(ls:*)"]}}"#);
        let again = settings_notice(&fixture.spec()).expect("still the notice");
        assert_eq!(again.message, notice.message);
        assert_eq!(asked_of(&asked).len(), 2);
    }

    /// A file the launch read stays read for its notice: one broken after the launch was built
    /// raises no notice, which would disagree with the rules the launch passed.
    #[test]
    fn a_settings_file_broken_after_the_launch_raises_no_notice() {
        let fixture = isolated();
        let path = user_settings_file(&fixture);
        user_file(&path, r#"{"permissions":{"allow":["Bash(ls:*)"]}}"#);
        let launch = build(&fixture.spec()).expect("built");
        assert_eq!(launch.user_allow, [json!("Bash(ls:*)")]);
        user_file(&path, "{ not json");
        assert!(settings_notice(&fixture.spec()).is_none());
    }

    /// A settings file the CLI takes, once the keys it does not know are out of the question, is
    /// read: its allow rules are repeated and there is no notice. So is one the CLI cannot be
    /// asked about, its run ending some other way: what is told of the file without the CLI
    /// stands. A file with nothing to ask about (none, an empty object, `null` keys only) asks
    /// nothing.
    #[cfg(unix)]
    #[test]
    fn a_settings_file_the_cli_takes_or_cannot_be_asked_about_is_read() {
        let text = r#"{"mine":1,"permissions":{"allow":["Bash(ls:*)"]}}"#;
        let mut fixture = isolated();
        let asked = answering_settings(&mut fixture, 0);
        user_file(&user_settings_file(&fixture), text);
        let launch = build(&fixture.spec()).expect("built");
        assert_eq!(launch.user_allow, [json!("Bash(ls:*)")]);
        assert!(settings_notice(&fixture.spec()).is_none());
        assert_eq!(asked_of(&asked).len(), 2);
        assert!(!launch.dir.join(SETTINGS_QUESTION_FILE).exists());

        // A CLI that does not answer the question: here one that ends some other way.
        let mut fixture = isolated();
        let home = fixture.scratch.join("home");
        fixture.shell_env =
            crate::test_support::fake_agent(&home, "agy", "echo 'something else' >&2; exit 3");
        user_file(&user_settings_file(&fixture), text);
        let launch = build(&fixture.spec()).expect("built");
        assert_eq!(launch.user_allow, [json!("Bash(ls:*)")]);
        assert!(settings_notice(&fixture.spec()).is_none());
        assert!(!launch.dir.join(SETTINGS_QUESTION_FILE).exists());

        for text in [None, Some("{}"), Some(r#"{"agentMode":null,"mine":null}"#)] {
            let mut fixture = isolated();
            let asked = answering_settings(&mut fixture, 0);
            if let Some(text) = text {
                user_file(&user_settings_file(&fixture), text);
            }
            build(&fixture.spec()).expect("built");
            assert!(settings_notice(&fixture.spec()).is_none(), "{text:?}");
            assert_eq!(asked_of(&asked), Vec::<Value>::new(), "{text:?}");
        }
    }

    /// What the CLI answered is kept for its binary, its version and the settings' text: a launch
    /// whose settings were asked about asks nothing again, whatever the answer was, while other
    /// settings, and the same ones of another version, are asked about afresh.
    #[cfg(unix)]
    #[test]
    fn a_settings_question_is_asked_once_for_a_binary_a_version_and_a_text() {
        let mut fixture = isolated();
        let asked = answering_settings(&mut fixture, 0);
        let path = user_settings_file(&fixture);
        let taken = r#"{"permissions":{"allow":["Bash(ls:*)"]}}"#;
        user_file(&path, taken);
        for _ in 0..2 {
            let launch = build(&fixture.spec()).expect("built");
            assert_eq!(launch.user_allow, [json!("Bash(ls:*)")]);
        }
        assert_eq!(asked_of(&asked).len(), 1);

        // A rejection, an unknown key among them, is kept as well, and still rejects.
        user_file(&path, r#"{"mine":1,"toolPermission":"nope"}"#);
        for _ in 0..2 {
            build(&fixture.spec()).expect("built");
            assert!(settings_notice(&fixture.spec()).is_some());
        }
        assert_eq!(asked_of(&asked).len(), 3);

        // Another version of the same binary is asked again.
        fixture.agent_version = Some((1, 6, 0));
        user_file(&path, taken);
        build(&fixture.spec()).expect("built");
        assert_eq!(asked_of(&asked).len(), 4);
    }

    /// An answer that says something is kept for good; one that says nothing is kept for
    /// [`PASSING_ANSWER_KEPT`] and then asked again, since it may come from a passing stall. The
    /// settings question's own rule of which is which, driven by the time given, not the clock.
    #[test]
    fn a_passing_answer_is_kept_for_a_while_and_a_real_one_for_good() {
        let kept: Kept<&str, SettingsAnswer> = Mutex::new(Vec::new());
        let start = Instant::now();
        let asks = std::cell::Cell::new(0);
        let ask_at = |key: &'static str, at: Instant, answer: SettingsAnswer| {
            kept_or_asked(&kept, key, at, SettingsAnswer::kept_for_good, || {
                asks.set(asks.get() + 1);
                answer
            })
        };
        let invalid = || SettingsAnswer::InvalidValue("agentMode".to_string());

        assert_eq!(ask_at("real", start, invalid()), invalid());
        assert_eq!(
            ask_at("passing", start, SettingsAnswer::Nothing),
            SettingsAnswer::Nothing
        );
        assert_eq!(asks.get(), 2);

        // Within the bound neither is asked again, whatever a new run would answer.
        let within = start + PASSING_ANSWER_KEPT - Duration::from_secs(1);
        assert_eq!(ask_at("real", within, SettingsAnswer::Taken), invalid());
        assert_eq!(
            ask_at("passing", within, SettingsAnswer::Taken),
            SettingsAnswer::Nothing
        );
        assert_eq!(asks.get(), 2);

        // Past it the passing answer is asked again, and the real one still stands.
        let past = start + PASSING_ANSWER_KEPT;
        assert_eq!(
            ask_at("passing", past, SettingsAnswer::Taken),
            SettingsAnswer::Taken
        );
        assert_eq!(ask_at("real", past, SettingsAnswer::Taken), invalid());
        assert_eq!(asks.get(), 3);
        // What the new run said is kept for good in its place.
        let much_later = past + PASSING_ANSWER_KEPT * 10;
        assert_eq!(
            ask_at("passing", much_later, SettingsAnswer::Nothing),
            SettingsAnswer::Taken
        );
        assert_eq!(asks.get(), 3);
        assert_eq!(kept.lock().expect("kept").len(), 2);
    }

    /// A CLI that does not take the directory flag is refused before it is asked of the settings.
    #[cfg(unix)]
    #[test]
    fn a_cli_without_the_directory_flag_is_asked_nothing_of_the_settings() {
        let mut fixture = isolated();
        let home = fixture.scratch.join("home");
        let asked = home.join("settings-asked");
        fixture.shell_env = crate::test_support::fake_agent(
            &home,
            "agy",
            &format!(
                "[ \"$2\" = --settings ] && echo asked >> '{}'\n\
                 echo 'flags provided but not defined: -gemini_dir' >&2; exit 2\n",
                asked.display()
            ),
        );
        user_file(&user_settings_file(&fixture), r#"{"agentMode":"auto"}"#);
        let err = build(&fixture.spec()).err().expect("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            Code::of_provider(LAUNCH_DIRECTORY_FLAG_NOT_TAKEN)
        );
        assert!(!asked.exists(), "the settings were asked about");
    }

    /// How the CLI's answer to settings given with `--settings` is read (1.5.14): status 0 takes
    /// them; status 1 with its line for a key it does not know, or for a value it does not take,
    /// names the key, which is written as a quoted text; anything else says nothing.
    #[test]
    fn the_clis_answer_to_given_settings_is_read_from_its_status_and_its_line() {
        assert_eq!(settings_answer(Some(0), b""), SettingsAnswer::Taken);
        assert_eq!(
            settings_answer(Some(1), b"Error: --settings: unknown setting \"aa\"\n"),
            SettingsAnswer::UnknownKey("aa".to_string())
        );
        assert_eq!(
            settings_answer(
                Some(1),
                b"Error: --settings: unknown setting \"we\\\"ird\"\n"
            ),
            SettingsAnswer::UnknownKey("we\"ird".to_string())
        );
        assert_eq!(
            settings_answer(
                Some(1),
                b"Error: --settings: invalid value for setting \"permissions\": permissions: invalid value {\"allow\":[\"command(ls)\"],\"deny\":\"command(rm)\"}\n"
            ),
            SettingsAnswer::InvalidValue("permissions".to_string())
        );
        assert_eq!(
            settings_answer(
                Some(1),
                b"Error: --settings: invalid value for setting \"agentMode\": agentMode: unrecognized value \"nope\"\n"
            ),
            SettingsAnswer::InvalidValue("agentMode".to_string())
        );
        for (status, stderr) in [
            (
                Some(1),
                "Error: --settings must be a JSON object of settings.json keys: invalid character",
            ),
            (Some(2), "flags provided but not defined: -gemini_dir"),
            (Some(1), "Error: --settings: unknown setting aa"),
            (Some(1), ""),
            (Some(2), "Error: --settings: unknown setting \"aa\""),
            (None, "Error: --settings: unknown setting \"aa\""),
        ] {
            assert_eq!(
                settings_answer(status, stderr.as_bytes()),
                SettingsAnswer::Nothing,
                "{status:?} {stderr}"
            );
        }
    }

    /// Where the settings file is not read and rules are left behind too, both are told, each as
    /// a notice of its own: the settings one as the launch's own, the leftovers one beside it.
    #[test]
    fn a_settings_file_not_read_and_leftovers_are_two_notices() {
        let fixture = isolated();
        user_file(&user_config(&fixture).join(".migrated"), "");
        user_file(
            &user_gemini(&fixture)
                .join("antigravity-cli")
                .join("rules")
                .join("mine.md"),
            "mine",
        );
        user_file(&user_settings_file(&fixture), "{ not json");
        build(&fixture.spec()).expect("built");
        let notice = settings_notice(&fixture.spec()).expect("a notice");
        assert_eq!(notice.code, Code::of_provider(SETTINGS_NOT_READ));
        let further = leftovers_notice(&fixture.spec()).expect("the leftovers notice");
        assert_eq!(
            further.code,
            Code::of_provider(EARLIER_LAYOUT_LEFTOVERS_NOT_READ)
        );
        assert_eq!(
            further.params.get("left").map(String::as_str),
            Some("rules")
        );
    }

    /// An account with a directory of its own is refused, by the provider's launch refusal, before
    /// anything is read or made: the CLI has the default account only, its login being one item of
    /// the system keychain.
    #[test]
    fn an_account_with_a_directory_of_its_own_is_refused() {
        let mut fixture = isolated();
        let account = fixture.scratch.join("account").join(".gemini");
        std::fs::create_dir_all(&account).expect("account directory");
        fixture.config_dir = Some(account.clone());
        let err = crate::adapter::provider(AgentId::ANTIGRAVITY)
            .launch_refusal(&fixture.spec())
            .expect_err("refused");
        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(
            coded.code,
            crate::protocol::error_code::PROVIDER_UNSUPPORTED
        );
        assert_eq!(
            coded.params.get("operation").map(String::as_str),
            Some("owned_accounts")
        );
        assert!(!fixture.scratch.join("antigravity").exists());
        assert!(leftovers_notice(&fixture.spec()).is_none());
        assert!(!user_gemini(&fixture).exists());
        assert_eq!(
            std::fs::read_dir(&account)
                .expect("account directory")
                .count(),
            0
        );
    }

    /// `mcp_config.json` is read as a session of the CLI reads it (1.5.14): with `//` and
    /// `/* */` comments, a comma before a closing brace and a byte order mark, alone or together,
    /// the user's server is carried into the launch's copy, which is strict JSON, beside
    /// Octoboard's, and the user's file is left as written.
    #[test]
    fn an_mcp_config_with_comments_and_trailing_commas_starts_a_session() {
        for text in [
            "// mine\n{\"mcpServers\":{\"mine\":{\"command\":\"x\"}}}",
            "{\n  /* mine */\n  \"mcpServers\": {\"mine\": {\"command\": \"x\"}}\n}",
            "{\"mcpServers\":{\"mine\":{\"command\":\"x\",},},}",
            "\u{FEFF}// mine\n{\"mcpServers\":{\"mine\":{\"command\":\"x\"}}, // last\n}",
        ] {
            let fixture = isolated();
            let path = user_config(&fixture).join("mcp_config.json");
            user_file(&path, text);
            let launch =
                build(&fixture.spec()).unwrap_or_else(|err| panic!("{text:?} refused: {err}"));
            let servers = servers_of(&launch);
            assert_eq!(
                servers["mcpServers"]["mine"],
                json!({ "command": "x" }),
                "{text:?}"
            );
            assert!(servers["mcpServers"]["octoboard"].is_object(), "{text:?}");
            assert_eq!(std::fs::read_to_string(&path).expect("file"), text);
        }
    }

    fn refused(fixture: &SpecFixture, path: &Path, case: &str) {
        let err = build(&fixture.spec())
            .err()
            .unwrap_or_else(|| panic!("{case}: not refused"));
        let coded = err
            .downcast_ref::<CodedError>()
            .unwrap_or_else(|| panic!("{case}: not coded: {err}"));
        assert_eq!(
            coded.code,
            Code::of_provider(USER_CONFIG_UNUSABLE),
            "{case}"
        );
        assert_eq!(
            coded.params.get("path").map(String::as_str),
            Some(path.to_string_lossy().as_ref()),
            "{case}"
        );
        assert!(
            coded.message.contains(path.to_string_lossy().as_ref()),
            "{case}: {}",
            coded.message
        );
        assert!(!fixture.scratch.join("antigravity").exists(), "{case}");
    }

    /// The launch's directory is one of the scratch directory's own, made afresh for each launch:
    /// what an earlier launch left in it does not stay.
    #[test]
    fn a_launch_directory_left_by_an_earlier_launch_is_made_afresh() {
        let fixture = isolated();
        let first = build(&fixture.spec()).expect("built");
        assert_eq!(first.dir, fixture.scratch.join("antigravity"));
        std::fs::write(first.dir.join("stale"), "left behind").expect("file");
        let second = build(&fixture.spec()).expect("built again");
        assert_eq!(second.dir, first.dir);
        assert!(!second.dir.join("stale").exists());
    }

    /// The argument is the flag with the directory joined by `=`: the one spelling the CLI takes.
    #[test]
    fn the_directory_argument_joins_the_flag_to_its_value() {
        assert_eq!(
            directory_argument(Path::new("/a b/launch")),
            "--gemini_dir=/a b/launch"
        );
    }

    /// The user's `permissions.allow` rules are read from the CLI's settings as they are, in their
    /// order, for the launch's settings to repeat; a settings file that is missing, empty, white
    /// space only, or has no such list, names none, and `deny` and `ask` are never read into it.
    #[test]
    fn the_users_allow_rules_are_read_from_the_settings_in_order() {
        let allow_of = |settings: Option<&str>| {
            let fixture = isolated();
            if let Some(settings) = settings {
                user_file(
                    &user_gemini(&fixture)
                        .join("antigravity-cli")
                        .join("settings.json"),
                    settings,
                );
            }
            build(&fixture.spec()).expect("built").user_allow
        };
        assert_eq!(
            allow_of(Some(
                r#"{"permissions":{"allow":["Bash(ls:*)","mcp(other/*)","Bash(a:*)"],"deny":["Bash(rm:*)"],"ask":["Bash(curl:*)"]}}"#
            )),
            [
                json!("Bash(ls:*)"),
                json!("mcp(other/*)"),
                json!("Bash(a:*)")
            ]
        );
        assert_eq!(
            allow_of(Some(
                "\u{FEFF}{\"permissions\":{\"allow\":[\"Bash(ls:*)\"]}}"
            )),
            [json!("Bash(ls:*)")]
        );
        for settings in [
            None,
            Some(""),
            Some("  \n "),
            Some("{}"),
            Some(r#"{"permissions":{}}"#),
            Some(r#"{"permissions":{"deny":["Bash(rm:*)"]}}"#),
            Some(r#"{"theme":"dark"}"#),
        ] {
            assert_eq!(allow_of(settings), Vec::<Value>::new(), "{settings:?}");
        }
    }

    /// Octoboard's hook entry and its server go in under their own names, which overwrite an entry
    /// of the user's of that name, the user's others standing; `mcpServers` is put last among the
    /// MCP file's keys, after the user's own, whatever place it had.
    #[test]
    fn octoboards_names_overwrite_the_users_and_the_servers_come_last() {
        let fixture = isolated();
        user_file(
            &user_config(&fixture).join("hooks.json"),
            r#"{"octoboard-session":{"Stop":[{"type":"command","command":"echo imposter"}]},"mine":{}}"#,
        );
        user_file(
            &user_config(&fixture).join("mcp_config.json"),
            r#"{"mcpServers":{"octoboard":{"command":"imposter"},"mine":{"command":"x"}},"other":1,"another":2}"#,
        );
        let spec = fixture.spec();
        let launch = build(&spec).expect("built");

        let hooks = hooks_of(&launch);
        let mut names: Vec<&String> = hooks.as_object().expect("hooks").keys().collect();
        names.sort();
        assert_eq!(names, ["mine", "octoboard-session"]);
        let ours = hooks["octoboard-session"].as_object().expect("our hooks");
        assert_eq!(
            ours.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "SessionStart",
                "PreInvocation",
                "Stop",
                "PreToolUse",
                "PostToolUse"
            ]
        );
        assert!(!hooks.to_string().contains("imposter"));

        let servers = servers_of(&launch);
        let mut keys: Vec<&String> = servers.as_object().expect("servers").keys().collect();
        assert_eq!(keys.pop(), Some(&"mcpServers".to_string()));
        keys.sort();
        assert_eq!(keys, ["another", "other"]);
        assert_eq!(servers["other"], 1);
        assert_eq!(servers["another"], 2);
        let (command, _) = crate::adapter::mcp_server_command(&spec);
        assert_eq!(servers["mcpServers"]["octoboard"]["command"], command);
        assert_eq!(servers["mcpServers"]["mine"]["command"], "x");
    }

    /// The directory the CLI makes at its first run, `config/projects`, is made in the user's home
    /// ahead of the build and linked, so what the session writes there lands in the home.
    #[test]
    fn the_projects_directory_is_made_in_the_home_and_linked() {
        let fixture = isolated();
        let launch = build(&fixture.spec()).expect("built");
        let home_projects = user_config(&fixture).join("projects");
        assert!(home_projects.is_dir());
        std::fs::write(
            launch.dir.join("config/projects/default-cli-project.json"),
            "{}",
        )
        .expect("written through the link");
        assert!(home_projects.join("default-cli-project.json").is_file());
    }

    /// Where the user's home has no marker of a finished migration and nothing of an earlier
    /// layout, the launch gets an empty marker of its own, a file and not a link, and the home is
    /// given none. A marker the home already has is the launch's as any other entry is.
    #[test]
    fn a_home_without_the_migration_marker_gets_none_and_the_launch_its_own() {
        let fixture = isolated();
        let launch = build(&fixture.spec()).expect("built");
        let marker = launch.dir.join("config/.migrated");
        let entry = std::fs::symlink_metadata(&marker).expect("the launch has a marker");
        assert!(entry.file_type().is_file(), "the marker is a link");
        assert_eq!(entry.len(), 0);
        assert!(std::fs::symlink_metadata(user_config(&fixture).join(".migrated")).is_err());
        std::fs::write(&marker, "migrated").expect("written");
        assert!(
            std::fs::symlink_metadata(user_config(&fixture).join(".migrated")).is_err(),
            "the home got a marker through the launch's"
        );

        let fixture = isolated();
        user_file(&user_config(&fixture).join(".migrated"), "done");
        let launch = build(&fixture.spec()).expect("built");
        assert_eq!(
            std::fs::read_to_string(launch.dir.join("config/.migrated")).expect("file"),
            "done"
        );
        assert_eq!(
            std::fs::read_to_string(user_config(&fixture).join(".migrated")).expect("home's file"),
            "done"
        );
    }

    /// A marker of the user's that is a link leading nowhere, or a link to itself, is no marker
    /// to the CLI, which would then migrate through the launch's directory: the launch gets a
    /// regular file of its own in the link's place, and the user's link is left as it was.
    #[cfg(unix)]
    #[test]
    fn a_marker_that_is_a_link_leading_nowhere_gives_the_launch_its_own() {
        let nowhere = |fixture: &SpecFixture| user_config(fixture).join("gone-target");
        let itself = |fixture: &SpecFixture| user_config(fixture).join(".migrated");
        let targets: [(&str, LinkTarget<'_>); 2] = [("dangling", &nowhere), ("loop", &itself)];
        for (case, target) in targets {
            let fixture = isolated();
            let home_marker = user_config(&fixture).join(".migrated");
            std::fs::create_dir_all(user_config(&fixture)).expect("config directory");
            let target = target(&fixture);
            std::os::unix::fs::symlink(&target, &home_marker).expect("link");
            user_file(&user_config(&fixture).join("config.json"), "{\"a\":1}");

            let launch = build(&fixture.spec()).unwrap_or_else(|err| panic!("{case}: {err}"));

            let marker = launch.dir.join("config/.migrated");
            let entry = std::fs::symlink_metadata(&marker).expect("the launch has a marker");
            assert!(entry.file_type().is_file(), "{case}: the marker is a link");
            assert_eq!(entry.len(), 0, "{case}");
            assert!(std::fs::metadata(&marker).is_ok(), "{case}");
            // Nothing of the home changed: the link is the same link, and it still leads nowhere.
            assert_eq!(
                std::fs::read_link(&home_marker).expect("still a link"),
                target,
                "{case}"
            );
            assert!(std::fs::metadata(&home_marker).is_err(), "{case}");
            assert!(
                !user_config(&fixture).join("gone-target").exists(),
                "{case}"
            );
            assert_eq!(
                std::fs::read_to_string(user_config(&fixture).join("config.json")).expect("file"),
                "{\"a\":1}",
                "{case}"
            );
        }
    }

    /// A home with no marker that still holds an entry of the earlier layout in the CLI's data
    /// directory makes the enhanced launch unavailable with the provider's code, whichever of the
    /// six entries it is, and before anything is made: no launch directory, no data or projects
    /// directory in the home, and the entry left where it was. A marker that is a link leading
    /// nowhere is no marker.
    #[test]
    fn an_earlier_layout_not_yet_migrated_makes_the_enhanced_launch_unavailable_and_builds_nothing()
    {
        let unmigrated = |fixture: &SpecFixture, case: &str| {
            let err = build(&fixture.spec())
                .err()
                .unwrap_or_else(|| panic!("{case}: not refused"));
            let coded = err
                .downcast_ref::<CodedError>()
                .unwrap_or_else(|| panic!("{case}: not coded: {err}"));
            assert_eq!(
                coded.code,
                Code::of_provider(EARLIER_LAYOUT_NOT_MIGRATED),
                "{case}"
            );
            assert!(coded.params.is_empty(), "{case}: {:?}", coded.params);
            assert!(coded.message.contains("agy"), "{case}: {}", coded.message);
            assert!(!fixture.scratch.join("antigravity").exists(), "{case}");
            assert!(!user_config(fixture).join("projects").exists(), "{case}");
        };
        for name in ["rules", "workflows", "skills", "plugins"] {
            let fixture = isolated();
            let entry = user_gemini(&fixture).join("antigravity-cli").join(name);
            std::fs::create_dir_all(&entry).expect("directory");
            unmigrated(&fixture, name);
            assert!(entry.is_dir(), "{name}");
        }
        for name in ["hooks.json", "mcp_config.json"] {
            let fixture = isolated();
            let entry = user_gemini(&fixture).join("antigravity-cli").join(name);
            user_file(&entry, "{}");
            unmigrated(&fixture, name);
            assert_eq!(
                std::fs::read_to_string(&entry).expect("file"),
                "{}",
                "{name}"
            );
        }
        // It comes before a file of the user's is read: one that could not be used is not named.
        let fixture = isolated();
        std::fs::create_dir_all(user_gemini(&fixture).join("antigravity-cli").join("skills"))
            .expect("directory");
        user_file(&user_config(&fixture).join("hooks.json"), "{ not json");
        unmigrated(&fixture, "before the user's files");

        #[cfg(unix)]
        {
            let fixture = isolated();
            user_file(
                &user_gemini(&fixture)
                    .join("antigravity-cli")
                    .join("skills")
                    .join("s.md"),
                "skill",
            );
            std::fs::create_dir_all(user_config(&fixture)).expect("config directory");
            std::os::unix::fs::symlink(
                user_config(&fixture).join("gone-target"),
                user_config(&fixture).join(".migrated"),
            )
            .expect("link");
            unmigrated(&fixture, "a marker that leads nowhere");
        }
    }

    /// One of the two files not yet moved while the configuration directory has a file of its name
    /// that holds something refuses under a code of its own, naming the file that would be
    /// replaced and the earlier one, before the plain refusal and before anything is made, and
    /// both files are left as they were. An empty file in the configuration directory (what a run
    /// of the CLI leaves in a home that had none) is the plain refusal, and so are directories in
    /// both places, which the CLI neither moves nor replaces.
    #[test]
    fn a_file_in_both_places_refuses_under_its_own_code() {
        let code_of = |fixture: &SpecFixture, case: &str| {
            let err = build(&fixture.spec())
                .err()
                .unwrap_or_else(|| panic!("{case}: not refused"));
            let coded = err
                .downcast_ref::<CodedError>()
                .unwrap_or_else(|| panic!("{case}: not coded: {err}"));
            assert!(!fixture.scratch.join("antigravity").exists(), "{case}");
            (coded.code, coded.params.clone())
        };
        for name in ["hooks.json", "mcp_config.json"] {
            let fixture = isolated();
            let earlier = user_gemini(&fixture).join("antigravity-cli").join(name);
            let current = user_config(&fixture).join(name);
            user_file(&earlier, r#"{"earlier":{}}"#);
            user_file(&current, r#"{"current":{}}"#);
            // Another entry not yet moved does not turn it into the plain refusal.
            std::fs::create_dir_all(user_gemini(&fixture).join("antigravity-cli").join("rules"))
                .expect("directory");
            let (code, params) = code_of(&fixture, name);
            assert_eq!(
                code,
                Code::of_provider(EARLIER_LAYOUT_FILE_IN_BOTH_PLACES),
                "{name}"
            );
            assert_eq!(
                params.get("path").map(String::as_str),
                Some(current.to_string_lossy().as_ref()),
                "{name}"
            );
            assert_eq!(
                params.get("earlier_path").map(String::as_str),
                Some(earlier.to_string_lossy().as_ref()),
                "{name}"
            );
            assert_eq!(params.len(), 2, "{name}: {params:?}");
            assert_eq!(
                std::fs::read_to_string(&earlier).expect("file"),
                r#"{"earlier":{}}"#
            );
            assert_eq!(
                std::fs::read_to_string(&current).expect("file"),
                r#"{"current":{}}"#
            );

            // An empty file in the configuration directory holds nothing to lose.
            let fixture = isolated();
            user_file(
                &user_gemini(&fixture).join("antigravity-cli").join(name),
                "{}",
            );
            user_file(&user_config(&fixture).join(name), "");
            let (code, params) = code_of(&fixture, &format!("{name}, empty"));
            assert_eq!(
                code,
                Code::of_provider(EARLIER_LAYOUT_NOT_MIGRATED),
                "{name}"
            );
            assert!(params.is_empty(), "{name}: {params:?}");
        }

        // Directories in both places, each holding something.
        let fixture = isolated();
        user_file(
            &user_gemini(&fixture)
                .join("antigravity-cli")
                .join("skills")
                .join("old")
                .join("SKILL.md"),
            "old",
        );
        user_file(
            &user_config(&fixture)
                .join("skills")
                .join("new")
                .join("SKILL.md"),
            "new",
        );
        let (code, _) = code_of(&fixture, "directories");
        assert_eq!(code, Code::of_provider(EARLIER_LAYOUT_NOT_MIGRATED));

        // With the marker there, neither refusal applies: the CLI will not migrate.
        let fixture = isolated();
        user_file(
            &user_gemini(&fixture)
                .join("antigravity-cli")
                .join("hooks.json"),
            "{}",
        );
        user_file(
            &user_config(&fixture).join("hooks.json"),
            r#"{"current":{}}"#,
        );
        user_file(&user_config(&fixture).join(".migrated"), "");
        build(&fixture.spec()).expect("a home with its marker launches");
    }

    /// A link among the earlier layout's entries that leads anywhere but the entry of its name in
    /// the configuration directory is the user's own (a directory of dotfiles), not what the
    /// migration left: with no marker it makes the enhanced launch unavailable as an entry not yet
    /// moved does, and neither the link nor what it leads to is touched. A link the migration left
    /// whose moved entry is gone still names it, and stands in the way of nothing.
    #[test]
    fn a_link_to_somewhere_else_is_an_entry_not_yet_moved() {
        let fixture = isolated();
        let old = user_gemini(&fixture).join("antigravity-cli");
        let dotfiles = fixture.scratch.join("dotfiles").join("skills");
        user_file(&dotfiles.join("mine").join("SKILL.md"), "skill");
        std::fs::create_dir_all(&old).expect("data directory");
        crate::hostfs::alias(&dotfiles, &old.join("skills"), true).expect("the user's link");
        let err = build(&fixture.spec()).err().expect("refused");
        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, Code::of_provider(EARLIER_LAYOUT_NOT_MIGRATED));
        assert!(!fixture.scratch.join("antigravity").exists());
        assert!(std::fs::symlink_metadata(old.join("skills"))
            .expect("the link")
            .file_type()
            .is_symlink());
        assert!(dotfiles.join("mine").join("SKILL.md").is_file());

        // A link to a like-named directory of another place is the user's own as well.
        let fixture = isolated();
        let old = user_gemini(&fixture).join("antigravity-cli");
        let elsewhere = fixture
            .scratch
            .join("elsewhere")
            .join("config")
            .join("rules");
        std::fs::create_dir_all(&elsewhere).expect("directory");
        std::fs::create_dir_all(user_config(&fixture).join("rules")).expect("the home's own");
        std::fs::create_dir_all(&old).expect("data directory");
        crate::hostfs::alias(&elsewhere, &old.join("rules"), true).expect("the user's link");
        assert!(build(&fixture.spec()).is_err());

        // What the migration left, its moved entry since deleted: the link still names it.
        #[cfg(unix)]
        {
            let fixture = isolated();
            let old = user_gemini(&fixture).join("antigravity-cli");
            std::fs::create_dir_all(&old).expect("data directory");
            std::fs::create_dir_all(user_config(&fixture)).expect("config directory");
            std::os::unix::fs::symlink(
                user_config(&fixture).join("workflows"),
                old.join("workflows"),
            )
            .expect("link");
            build(&fixture.spec()).expect("a leftover link whose entry is gone launches");
        }
    }

    /// The earlier layout refuses nothing where the home has its marker (the CLI has migrated, or
    /// will not), and nothing where the entries are the links the migration leaves in their place,
    /// each to the entry of its name in the configuration directory: such a home with no marker
    /// launches with the launch's own.
    #[test]
    fn a_migrated_home_and_entries_that_are_links_launch() {
        let fixture = isolated();
        let old = user_gemini(&fixture).join("antigravity-cli");
        user_file(&old.join("skills").join("mine").join("SKILL.md"), "skill");
        user_file(&old.join("hooks.json"), r#"{"old-hook":{}}"#);
        user_file(&user_config(&fixture).join(".migrated"), "");
        let launch = build(&fixture.spec()).expect("a home with its marker launches");
        assert!(old.join("skills").join("mine").join("SKILL.md").is_file());
        assert!(old.join("hooks.json").is_file());
        assert!(!hooks_of(&launch)
            .as_object()
            .expect("hooks")
            .contains_key("old-hook"));

        let fixture = isolated();
        let old = user_gemini(&fixture).join("antigravity-cli");
        let moved = user_config(&fixture).join("skills");
        std::fs::create_dir_all(&moved).expect("the moved directory");
        std::fs::create_dir_all(&old).expect("data directory");
        crate::hostfs::alias(&moved, &old.join("skills"), true).expect("a link in its place");
        let mut linked = vec!["skills"];
        if crate::platform::fs::file_links_available() {
            user_file(&user_config(&fixture).join("hooks.json"), "{}");
            crate::hostfs::alias(
                &user_config(&fixture).join("hooks.json"),
                &old.join("hooks.json"),
                false,
            )
            .expect("a link in its place");
            linked.push("hooks.json");
        }
        let launch = build(&fixture.spec())
            .unwrap_or_else(|err| panic!("links only ({linked:?}) refused: {err}"));
        let marker = launch.dir.join("config/.migrated");
        assert!(std::fs::symlink_metadata(&marker)
            .expect("the launch's own marker")
            .file_type()
            .is_file());
        assert!(std::fs::symlink_metadata(user_config(&fixture).join(".migrated")).is_err());
    }

    /// The launch's settings are written whole to one file of the launch's directory, private to
    /// the user, replaced by a second writing, and the path returned is the file's.
    #[test]
    fn the_launch_settings_are_written_to_a_private_file_of_the_launch() {
        let fixture = isolated();
        let launch = build(&fixture.spec()).expect("built");
        let json = r#"{"permissions":{"allow":["Bash(echo secret:*)"]}}"#;
        let path = write_settings(&launch.dir, json).expect("written");
        assert_eq!(path, launch.dir.join("octoboard-launch-settings.json"));
        assert_eq!(std::fs::read_to_string(&path).expect("file"), json);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("entry")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let again = write_settings(&launch.dir, "{}").expect("written again");
        assert_eq!(again, path);
        assert_eq!(std::fs::read_to_string(&path).expect("file"), "{}");
    }
}
