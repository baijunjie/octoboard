//! The per-session `GROK_HOME`: a farm over the source home, linking or copying the user's entries
//! (by what the host can link, [`file_links_available`]), with the files Octoboard owns copied in.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::HOOK_EVENTS;
use crate::adapter::files::{canonical_project, LARGE_FILE_LIMIT};
use crate::adapter::{add_guard_group, GuardHook, LaunchSpec, COMMAND_HOOK_TIMEOUT_SECS};
use crate::platform::fs::{self as platform, create_private};
use crate::providers::grok::{GROK_CONFIG_FILE, GROK_SIGN_IN_FILE, GROK_TRUST_STORE};
use crate::session_tools;

/// Entries Octoboard owns in its `GROK_HOME` rather than linking to the user's.
const COPIED_ENTRIES: &[&str] = &[GROK_CONFIG_FILE, GROK_TRUST_STORE];

/// Entries Grok's startup sandbox check wants as real files rather than symlinks; copied from the
/// source when it has them. They are copied on every launch, although a launch that runs under the
/// sandbox is refused: detection cannot see every source (macOS managed preferences, policy synced
/// from the server), and a launch it misses must still get past Grok's startup check rather than
/// fail there. Copying unconditionally also avoids depending on detection.
const SANDBOX_CHECKED_ENTRIES: &[&str] = &["bundled", "bundles", "hooks-paths", "sandbox.toml"];

/// Entries that must never exist in the farm: Grok's deployment sync deletes them at runtime, and
/// an organisation-distributed config is not Octoboard's to reproduce.
const EXCLUDED_ENTRIES: &[&str] = &["managed_config.toml", "requirements.toml", "hooks"];

/// The variable that names Grok's login store to it in place of `$GROK_HOME/auth.json` (Grok Build
/// reads the login from the file it names: measured on 1.0.77).
const LOGIN_STORE_VAR: &str = "GROK_AUTH_PATH";

/// The session's `GROK_HOME`, and what the launch sets for Grok to use it.
pub(super) struct GrokHome {
    pub dir: PathBuf,
    /// [`LOGIN_STORE_VAR`] naming the user's own login, where the farm cannot link it.
    pub env: Vec<(String, String)>,
    /// Whether the copy holds Claude Code's MCP servers, so that Grok's own reading of them is to
    /// be turned off ([`super::compatibility_consumer::CLAUDE_MCPS_VAR`]).
    pub claude_mcps_supplied: bool,
}

#[cfg(test)]
thread_local! {
    /// [`platform::file_links_available`] for a test, on the test's own thread.
    pub(super) static FILE_LINKS_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Whether the host makes file links ([`platform::file_links_available`]), or a test's override of
/// it. Read on the thread that builds the farm.
fn file_links_available() -> bool {
    #[cfg(test)]
    if let Some(available) = FILE_LINKS_OVERRIDE.get() {
        return available;
    }
    platform::file_links_available()
}

/// How the farm carries an entry of the source home that it neither excludes nor owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Carried {
    /// A link to the entry's resolved target, so what Grok writes there is the user's.
    Linked,
    /// A copy, the session's own: what Grok writes there is lost with the session.
    Copied,
    /// Not in the farm: Grok creates its own if it needs one.
    Left,
}

/// How the farm carries the source entry `name`.
///
/// A directory is always linked: a directory alias needs no file links on any host.
///
/// Where the host makes file links, every file is linked too, its lock files with it: a lock beside
/// a linked file (`auth.json.lock`, `active_sessions.lock`) is what keeps the session's Grok and the
/// user's own from writing that file at once, so it must be the user's; a lock beside a file the
/// farm owns only makes the two wait on each other for a moment. The exception is a SQLite database
/// where the host's SQLite names its log after the path it was opened by
/// ([`platform::SQLITE_FOLLOWS_FILE_ALIASES`]): opened through a link, it would keep a log of its own
/// beside the user's over the one database, which corrupts it, so it is copied, and its
/// shared-memory index left for SQLite to rebuild.
///
/// Where the host makes none, every file is a copy except what no session may share in a copy:
///
/// - `auth.json`, the login. Grok refreshes it, and a refresh replaces the refresh token, so a
///   copy would leave the user's own `grok` holding a spent one; Grok is pointed at the user's own
///   file instead ([`LOGIN_STORE_VAR`]).
/// - Lock files: no file here is shared, so no lock needs to be, and a copied lock could carry its
///   holder's mark over into a home that holder never ran in. Grok creates its own.
/// - Files that exist only while something writes them: Grok's temporary files, sockets, SQLite's
///   shared-memory index.
pub(super) fn carried(name: &str, is_dir: bool, file_links: bool) -> Carried {
    if is_dir {
        Carried::Linked
    } else if file_links {
        if platform::SQLITE_FOLLOWS_FILE_ALIASES || !is_sqlite_file(name) {
            Carried::Linked
        } else if is_transient(name) {
            Carried::Left
        } else {
            Carried::Copied
        }
    } else if name == GROK_SIGN_IN_FILE || is_lock(name) || is_transient(name) {
        Carried::Left
    } else {
        Carried::Copied
    }
}

/// A lock file: `auth.json.lock`, `.config-init.lock`, `bundled/.lock`.
fn is_lock(name: &str) -> bool {
    name.ends_with(".lock")
}

/// A file that exists only while something writes or serves it: a temporary file a write left
/// behind (`.59335.0.tmp`, `.tmp2tPTzx`), a socket, a SQLite database's shared-memory index.
fn is_transient(name: &str) -> bool {
    name.ends_with(".tmp")
        || name.starts_with(".tmp")
        || name.ends_with(".sock")
        || name.ends_with("-shm")
}

/// A SQLite database (`worktrees.db`) or one of the files SQLite keeps beside it.
fn is_sqlite_file(name: &str) -> bool {
    let database = ["-wal", "-shm", "-journal"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name);
    [".db", ".sqlite", ".sqlite3"]
        .iter()
        .any(|extension| database.ends_with(extension))
}

/// Builds the session's `GROK_HOME`.
pub(super) fn build_grok_home(spec: &LaunchSpec<'_>, source_home: &Path) -> Result<GrokHome> {
    let farm = spec.scratch.join("grok-home");
    if farm.exists() {
        std::fs::remove_dir_all(&farm).ok();
    }
    std::fs::create_dir_all(&farm).with_context(|| format!("creating {}", farm.display()))?;
    let file_links = file_links_available();

    if source_home.is_dir() {
        for entry in std::fs::read_dir(source_home)
            .with_context(|| format!("reading {}", source_home.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if EXCLUDED_ENTRIES.contains(&name_str.as_ref())
                || COPIED_ENTRIES.contains(&name_str.as_ref())
                || SANDBOX_CHECKED_ENTRIES.contains(&name_str.as_ref())
            {
                continue;
            }
            let at = farm.join(&name);
            // The fully resolved target, so a source entry that is itself a link is reached through
            // one link and a save does not depend on following a chain.
            let target = crate::hostfs::canonical(entry.path()).unwrap_or_else(|_| entry.path());
            match carried(&name_str, target.is_dir(), file_links) {
                Carried::Linked => crate::hostfs::alias(&target, &at, target.is_dir())
                    .with_context(|| format!("linking {}", at.display()))?,
                Carried::Copied => copy_carried(&target, &at),
                Carried::Left => {}
            }
        }
    }

    let config = farm.join(GROK_CONFIG_FILE);
    copy_private_file(&source_home.join(GROK_CONFIG_FILE), &config)?;
    append_mcp_server(&config, spec)?;
    let added = super::compatibility_consumer::add_to_config(&config, source_home, spec);
    let mut env = added.env;

    copy_if_present(
        &source_home.join(GROK_TRUST_STORE),
        &farm.join(GROK_TRUST_STORE),
        file_links,
    )?;
    for name in SANDBOX_CHECKED_ENTRIES {
        copy_if_present(&source_home.join(name), &farm.join(name), file_links)?;
    }
    write_hooks(&farm, spec.hook_script, spec.guard.as_ref())?;

    // A login store the user's shell names already reaches Grok through the launch environment.
    if !file_links && !spec.shell_env.contains_key(LOGIN_STORE_VAR) {
        let login = source_home.join(GROK_SIGN_IN_FILE);
        // Resolved, so that Grok's save replaces the file rather than a link to it.
        let login = crate::hostfs::canonical(&login).unwrap_or(login);
        env.push((
            LOGIN_STORE_VAR.to_string(),
            login.to_string_lossy().into_owned(),
        ));
    }
    Ok(GrokHome {
        dir: farm,
        env,
        claude_mcps_supplied: added.claude_mcps_supplied,
    })
}

/// Copies a source file the farm carries as a copy. A file that cannot be copied (gone meanwhile,
/// a dangling link, held by another program) is left out, for Grok to start its own: what is
/// copied this way is state and caches that a session can do without, unlike the files the farm
/// owns.
fn copy_carried(from: &Path, to: &Path) {
    if !from.is_file() {
        return;
    }
    if let Err(err) = std::fs::copy(from, to) {
        tracing::warn!(path = %from.display(), %err, "copying an entry of the Grok home into a session's failed; it is left out");
    }
}

/// Appends Octoboard's MCP server to the farm's own `config.toml`. Appended rather than written
/// fresh, because this copy carries the user's whole Grok configuration and replacing it would be
/// the "do not override the user's configuration" failure in another form. The append step reads
/// only the copy's last byte. A file created here is owner-only, since it holds the user's whole
/// Grok configuration.
fn append_mcp_server(config: &Path, spec: &LaunchSpec<'_>) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};

    let (command, args) = crate::adapter::mcp_server_command(spec);
    let mut file = create_private(
        config,
        std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true),
    )
    .with_context(|| format!("opening {}", config.display()))?;
    let mut content = String::new();
    let reading = || format!("reading {}", config.display());
    if file.metadata().with_context(reading)?.len() > 0 {
        let mut last = [0u8];
        file.seek(SeekFrom::End(-1))
            .and_then(|_| file.read_exact(&mut last))
            .with_context(reading)?;
        if last[0] != b'\n' {
            content.push('\n');
        }
    }
    // Grok abandons a tool call at a limit of its own, and tells the server with a
    // `notifications/cancelled` — which withdraws a waiting session request, ending
    // the wait before the user can answer and leaving a later approval nothing to act on. That
    // makes giving up destructive here, unlike Codex, which loses only the call's result, and
    // Grok's own default is documented far above a call Octoboard holds for the user
    // (`session_request::ANSWER_TIME_LIMIT`), but its exact value was never measured, so where it
    // truly falls is unknown (the "Grok Build" section of `docs/agent-cli-reference.md` has what is
    // and is not established). The limit is therefore pinned to a known value rather than left to
    // that default. The reasoning for the value is at `crate::adapter::MCP_TOOL_TIMEOUT_SECS`.
    content.push_str(&format!(
        "\n[mcp_servers.{key}]\ncommand = {command}\nargs = [{args}]\n\
         tool_timeout_sec = {tool_timeout}\n",
        key = session_tools::catalogue::SERVER_KEY,
        tool_timeout = crate::adapter::MCP_TOOL_TIMEOUT_SECS,
        command = json!(command),
        args = args
            .iter()
            .map(|arg| json!(arg).to_string())
            .collect::<Vec<_>>()
            .join(", "),
    ));
    file.write_all(content.as_bytes())
        .with_context(|| format!("writing {}", config.display()))?;
    Ok(())
}

/// Copies the file `from` to `to` readable by its owner only, whatever the source's mode, and never
/// with a wider one in between; an absent source is left absent. A linked source is followed. The
/// source is read whole within [`LARGE_FILE_LIMIT`] before `to` is created, so a source that is too
/// large, or stops being a regular file before it is read, fails the copy rather than leaving a
/// truncated one.
fn copy_private_file(from: &Path, to: &Path) -> Result<()> {
    use std::io::Write;

    if !from.is_file() {
        return Ok(());
    }
    let bytes = crate::hostfs::read_regular(from, LARGE_FILE_LIMIT)
        .with_context(|| format!("reading {}", from.display()))?;
    let mut copy = create_private(
        to,
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .with_context(|| format!("creating {}", to.display()))?;
    copy.write_all(&bytes)
        .with_context(|| format!("copying {}", from.display()))
}

/// Copies a file, or a directory with everything under it; an absent source is left absent. A
/// link is followed only at the top, so a linked source becomes a real file or directory in the
/// copy; inside a directory a link is recreated as a link rather than followed, where the host
/// makes one (`file_links`).
fn copy_if_present(from: &Path, to: &Path, file_links: bool) -> Result<()> {
    if from.is_dir() {
        copy_tree(from, to, file_links)
    } else if from.is_file() {
        std::fs::copy(from, to)
            .map(drop)
            .with_context(|| format!("copying {}", from.display()))
    } else {
        Ok(())
    }
}

/// The inside of `copy_if_present` for a directory: files are copied, links recreated as they are,
/// anything else (a FIFO, a socket) skipped, since opening one could block or fail the launch.
///
/// Where the host makes no file links, Grok's own lock and temporary files are left out
/// ([`is_grok_scratch`]): the copy shares nothing that would need the lock, and Grok creates its
/// own. A link to a file is copied as the file it leads to, and a link to a directory is made
/// again to the directory it resolves to, which needs no file links; one that leads nowhere is
/// left out.
fn copy_tree(from: &Path, to: &Path, file_links: bool) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if !file_links && is_grok_scratch(from, &name.to_string_lossy()) {
            continue;
        }
        let (source, target) = (entry.path(), to.join(&name));
        let kind = entry
            .file_type()
            .with_context(|| format!("reading {}", source.display()))?;
        if kind.is_symlink() && !file_links {
            if source.is_dir() {
                let resolved = crate::hostfs::canonical(&source)
                    .with_context(|| format!("resolving {}", source.display()))?;
                crate::hostfs::alias(&resolved, &target, true)
                    .with_context(|| format!("linking {}", target.display()))?;
            } else if source.is_file() {
                std::fs::copy(&source, &target)
                    .with_context(|| format!("copying {}", source.display()))?;
            }
        } else if kind.is_symlink() {
            let destination = platform::read_alias(&source)
                .with_context(|| format!("reading link {}", source.display()))?
                .ok_or_else(|| anyhow::anyhow!("{} is not a link any more", source.display()))?;
            crate::hostfs::alias(&destination, &target, source.is_dir())
                .with_context(|| format!("linking {}", target.display()))?;
        } else if kind.is_dir() {
            copy_tree(&source, &target, file_links)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &target)
                .with_context(|| format!("copying {}", source.display()))?;
        }
    }
    Ok(())
}

/// A lock or temporary file of Grok's own in its home's directory `dir`: a bare `.lock`, a
/// `<file>.lock` beside the `<file>` it guards (`auth.json.lock`), and what Grok's writes leave
/// behind, `.tmp` and six letters or digits (`.tmp2tPTzx`) or `.<number>.<number>.tmp`
/// (`.59335.0.tmp`). Names that only look alike are a user's files and kept: `uv.lock`,
/// `Cargo.lock`, `yarn.lock` and `bun.lock` guard no file of their own name, and a `.tmpl…` name is
/// a template.
fn is_grok_scratch(dir: &Path, name: &str) -> bool {
    if let Some(guarded) = name.strip_suffix(".lock") {
        return guarded.is_empty() || std::fs::symlink_metadata(dir.join(guarded)).is_ok();
    }
    if let Some(random) = name.strip_prefix(".tmp") {
        return random.len() == 6
            && !random.starts_with('l')
            && random.bytes().all(|byte| byte.is_ascii_alphanumeric());
    }
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .and_then(|numbers| numbers.split_once('.'))
        .is_some_and(|(first, second)| {
            [first, second]
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

/// The folder Grok records its trust for: the working directory as Grok resolves it, canonical
/// (`/private/tmp/…`, not `/tmp/…`).
pub(super) fn canonical_cwd(cwd: &Path) -> String {
    canonical_project(cwd).to_string_lossy().into_owned()
}

/// The tools the cross-project guard decides: the edit tools under Claude's names, which Grok
/// maps to `search_replace`, and under its own and the Cursor preset's.
const GUARD_MATCHER: &str =
    "Edit|Write|MultiEdit|NotebookEdit|search_replace|write|hashline_edit|StrReplace|EditNotebook";

/// Writes Octoboard's hooks into the farm. Hooks placed in `$GROK_HOME/hooks/*.json` load at
/// `global` scope, which is always trusted — the folder-trust requirement applies to the project's
/// own `<project>/.grok/hooks`, not to these.
fn write_hooks(farm: &Path, hook_script: &Path, guard: Option<&GuardHook<'_>>) -> Result<()> {
    let hooks_dir = farm.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let command = hook_script.to_string_lossy().into_owned();
    let mut hooks = serde_json::Map::new();
    for event in HOOK_EVENTS {
        hooks.insert(
            (*event).to_string(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "timeout": COMMAND_HOOK_TIMEOUT_SECS,
                }]
            }]),
        );
    }
    // The cross-project guard decides an edit before it runs, so it is a group of its own beside
    // the status hook, and without `failClosed`: a guard that cannot decide stays silent.
    if let Some(guard) = guard {
        add_guard_group(&mut hooks, GUARD_MATCHER, &guard.script.to_string_lossy());
    }
    let document = json!({ "hooks": hooks });
    std::fs::write(
        hooks_dir.join("octoboard.json"),
        serde_json::to_vec_pretty(&document)?,
    )?;
    Ok(())
}
