//! The per-session `GROK_HOME`: a symlink farm over the source home, with the files Octoboard owns
//! copied in.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::HOOK_EVENTS;
use crate::adapter::files::{canonical_project, LARGE_FILE_LIMIT};
use crate::adapter::{LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::agents::layout::{GROK_CONFIG_FILE, GROK_TRUST_STORE};
use crate::mcp;

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

/// Builds the session's `GROK_HOME` and returns its path.
pub(super) fn build_grok_home(spec: &LaunchSpec<'_>, source_home: &Path) -> Result<PathBuf> {
    let farm = spec.scratch.join("grok-home");
    if farm.exists() {
        std::fs::remove_dir_all(&farm).ok();
    }
    std::fs::create_dir_all(&farm).with_context(|| format!("creating {}", farm.display()))?;

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
            let link = farm.join(&name);
            // The fully resolved target, so a source entry that is itself a link is reached through
            // one link and a save does not depend on following a chain.
            let target = std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            std::os::unix::fs::symlink(&target, &link)
                .with_context(|| format!("linking {}", link.display()))?;
        }
    }

    let config = farm.join(GROK_CONFIG_FILE);
    copy_private_file(&source_home.join(GROK_CONFIG_FILE), &config)?;
    append_mcp_server(&config, spec)?;

    copy_if_present(
        &source_home.join(GROK_TRUST_STORE),
        &farm.join(GROK_TRUST_STORE),
    )?;
    for name in SANDBOX_CHECKED_ENTRIES {
        copy_if_present(&source_home.join(name), &farm.join(name))?;
    }
    write_hooks(&farm, spec.hook_script)?;

    Ok(farm)
}

/// Appends Octoboard's MCP server to the farm's own `config.toml`. Appended rather than written
/// fresh, because this copy carries the user's whole Grok configuration and replacing it would be
/// the "do not override the user's configuration" failure in another form. The append step reads
/// only the copy's last byte. The server's command line carries the daemon's key, so a file
/// created here is owner-only.
fn append_mcp_server(config: &Path, spec: &LaunchSpec<'_>) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::fs::OpenOptionsExt;

    let (command, args) = crate::adapter::mcp_server_command(spec);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o600)
        .open(config)
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
        key = mcp::SERVER_KEY,
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
    use std::os::unix::fs::OpenOptionsExt;

    if !from.is_file() {
        return Ok(());
    }
    let bytes = crate::hostfs::read_regular(from, LARGE_FILE_LIMIT)
        .with_context(|| format!("reading {}", from.display()))?;
    let mut copy = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(to)
        .with_context(|| format!("creating {}", to.display()))?;
    copy.write_all(&bytes)
        .with_context(|| format!("copying {}", from.display()))
}

/// Copies a file, or a directory with everything under it; an absent source is left absent. A
/// link is followed only at the top, so a linked source becomes a real file or directory in the
/// copy; inside a directory a link is recreated as a link rather than followed.
fn copy_if_present(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        copy_tree(from, to)
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
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        let kind = entry
            .file_type()
            .with_context(|| format!("reading {}", source.display()))?;
        if kind.is_symlink() {
            let destination = std::fs::read_link(&source)
                .with_context(|| format!("reading link {}", source.display()))?;
            std::os::unix::fs::symlink(destination, &target)
                .with_context(|| format!("linking {}", target.display()))?;
        } else if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &target)
                .with_context(|| format!("copying {}", source.display()))?;
        }
    }
    Ok(())
}

/// The folder Grok records its trust for: the working directory as Grok resolves it, canonical
/// (`/private/tmp/…`, not `/tmp/…`).
pub(super) fn canonical_cwd(cwd: &Path) -> String {
    canonical_project(cwd).to_string_lossy().into_owned()
}

/// Writes Octoboard's hooks into the farm. Hooks placed in `$GROK_HOME/hooks/*.json` load at
/// `global` scope, which is always trusted — the folder-trust requirement applies to the project's
/// own `<project>/.grok/hooks`, not to these.
fn write_hooks(farm: &Path, hook_script: &Path) -> Result<()> {
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
                    "timeout": HOOK_TIMEOUT_SECS,
                }]
            }]),
        );
    }
    let document = json!({ "hooks": hooks });
    std::fs::write(
        hooks_dir.join("octoboard.json"),
        serde_json::to_vec_pretty(&document)?,
    )?;
    Ok(())
}
