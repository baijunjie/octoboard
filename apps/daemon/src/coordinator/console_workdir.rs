//! A console's working directory: the console session's instruction file written into it,
//! `AGENTS.md` with `CLAUDE.md` a symlink to it, and its removal with the console. The text is
//! built elsewhere; this puts it on disk.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};

use crate::hostfs::replace::{replace_file, Permissions};
use crate::protocol::Console;
use crate::session_tools::console_instructions::{
    console_session_instructions, CLAUDE_INSTRUCTION_FILE, INSTRUCTION_FILE,
};
use crate::session_tools::role::RoleOptions;
use crate::store::Store;

/// Serializes the writes of console instruction files. Each locked refresh takes the console from
/// the store under it, so whichever runs last has read the latest record and leaves its text on
/// disk. It also holds the removal of a deleted console's working directory, so no write lands in
/// a directory being removed. A static rather than a field of `AppState`: what it guards is the
/// files on disk, shared by the whole process, not state tied to one `AppState`.
static INSTRUCTION_FILE_WRITES: Mutex<()> = Mutex::new(());

/// Removes a deleted console's working directory under the lock, so a refresh that read the console
/// before it was deleted finishes first rather than writing into a directory being removed, and one
/// that starts after it finds no console to write for. Called only once the console's record is
/// deleted, which is what leaves a later refresh nothing to write for.
pub fn remove_console_workdir(workdir: &str) -> std::io::Result<()> {
    let _writing = INSTRUCTION_FILE_WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::fs::remove_dir_all(workdir)
}

/// Writes the console's instruction file from the console as it is stored right now, under the
/// lock that keeps concurrent refreshes from leaving an older record's text on disk.
pub fn refresh_console_session_instructions(store: &Store, console_id: &str) -> Result<()> {
    let _writing = INSTRUCTION_FILE_WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let console = store.require_console(console_id)?;
    write_console_session_instructions_from_settings(store, &console)
}

/// Writes the console's instruction file with the guidance the stored settings call for: asking as
/// choices, and replying in the interface's language. No lock: see
/// [`write_console_session_instructions`].
pub fn write_console_session_instructions_from_settings(
    store: &Store,
    console: &Console,
) -> Result<()> {
    let user_language = store.interface_language()?;
    write_console_session_instructions(
        console,
        &RoleOptions {
            ask_as_choices: store.ask_questions_as_choices()?,
            user_language: user_language.as_deref(),
            ..RoleOptions::default()
        },
    )
}

/// Writes the console's instruction file into its working directory: `AGENTS.md`, with
/// `CLAUDE.md` a relative symlink to it, so Claude Code, Codex and Grok all read the one text and
/// Grok, which reads both spellings, loads it once. Both are replaced atomically, so concurrent
/// launches never read a truncated file, including when they run different agents in the same
/// console. Not locked: a console no one has been told of yet has no other writer, and every other
/// caller goes through [`refresh_console_session_instructions`]. The working directory must exist:
/// it is created with the console and removed with it, never recreated by a write.
/// `options` carries the application settings that add to the text; its worktrees are ignored.
fn write_console_session_instructions(console: &Console, options: &RoleOptions) -> Result<()> {
    let workdir = Path::new(&console.workdir);

    let path = workdir.join(INSTRUCTION_FILE);
    let text = console_session_instructions(console.console_session_prompt.as_deref(), options);
    replace_file(
        &path,
        &temp_name_beside(workdir, INSTRUCTION_FILE),
        text.as_bytes(),
        Permissions::KeptOrDefault,
    )
    .with_context(|| format!("writing {}", path.display()))?;

    // A symlink that already points at the file is left alone. Anything else at that name — an
    // earlier build's regular file, or a link elsewhere — is replaced by one made under a
    // temporary name and renamed over it, so the name never goes missing.
    let link = workdir.join(CLAUDE_INSTRUCTION_FILE);
    if std::fs::read_link(&link).is_ok_and(|target| target == Path::new(INSTRUCTION_FILE)) {
        return Ok(());
    }
    let temp_link = temp_name_beside(workdir, CLAUDE_INSTRUCTION_FILE);
    std::os::unix::fs::symlink(INSTRUCTION_FILE, &temp_link)
        .with_context(|| format!("linking {}", temp_link.display()))?;
    rename_over(&temp_link, &link).inspect_err(|_| {
        std::fs::remove_file(&temp_link).ok();
    })
}

/// A unique name beside `name` in `dir`, for a file to be written in full and then renamed over it.
fn temp_name_beside(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.octoboard-tmp-{}", uuid::Uuid::new_v4()))
}

/// Renames `temp_path` over `path`, so a reader sees the old file or the new one and never a
/// partial one.
fn rename_over(temp_path: &Path, path: &Path) -> Result<()> {
    std::fs::rename(temp_path, path)
        .with_context(|| format!("renaming {} to {}", temp_path.display(), path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Agent;
    use crate::test_support::ScratchDir;

    fn console(agent: Agent, workdir: &Path) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: workdir.to_string_lossy().into_owned(),
            default_agent: agent,
            project_session_agent: None,
            accounts: Default::default(),
            launch: Default::default(),
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
            created_at: 0,
        }
    }

    /// Every agent reads the console's one instruction file: `CLAUDE.md` is a relative link to
    /// `AGENTS.md`, an earlier build's regular `CLAUDE.md` or a link elsewhere is replaced, and
    /// the user's text is in the file only while there is some.
    #[test]
    fn a_console_has_one_instruction_file_and_a_link_to_it() {
        let workdir = ScratchDir::new("console-instructions");
        std::fs::write(workdir.join("CLAUDE.md"), "an earlier build's file").unwrap();
        let mut console = console(Agent::Claude, &workdir);
        for (prompt, expected) in [
            (
                None,
                console_session_instructions(None, &RoleOptions::default()),
            ),
            (
                Some("Be brief."),
                console_session_instructions(Some("Be brief."), &RoleOptions::default()),
            ),
            (
                None,
                console_session_instructions(None, &RoleOptions::default()),
            ),
        ] {
            console.console_session_prompt = prompt.map(str::to_string);
            write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
            assert_eq!(
                std::fs::read_to_string(workdir.join("AGENTS.md")).unwrap(),
                expected
            );
            assert_eq!(
                std::fs::read_link(workdir.join("CLAUDE.md")).unwrap(),
                Path::new("AGENTS.md")
            );
            assert_eq!(
                std::fs::read_to_string(workdir.join("CLAUDE.md")).unwrap(),
                expected
            );
            assert_eq!(workdir.read_dir().unwrap().count(), 2);
        }
        std::fs::remove_file(workdir.join("CLAUDE.md")).unwrap();
        std::os::unix::fs::symlink("elsewhere", workdir.join("CLAUDE.md")).unwrap();
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
        assert_eq!(
            std::fs::read_link(workdir.join("CLAUDE.md")).unwrap(),
            Path::new("AGENTS.md")
        );
    }

    /// Rewriting the instruction file keeps the permissions the user gave it.
    #[test]
    fn a_rewritten_instruction_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let workdir = ScratchDir::new("console-instructions-mode");
        let console = console(Agent::Claude, &workdir);
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
        let file = workdir.join("AGENTS.md");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
            0o640
        );
    }

    /// A link that cannot be renamed into place (a directory sits at its name) is not left behind
    /// under its temporary name.
    #[test]
    fn a_link_that_cannot_be_renamed_into_place_is_removed() {
        let workdir = ScratchDir::new("console-instructions-link-failed");
        let console = console(Agent::Claude, &workdir);
        let blocker = workdir.join("CLAUDE.md");
        std::fs::create_dir(&blocker).unwrap();
        std::fs::write(blocker.join("inside"), "kept").unwrap();
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap_err();
        let mut names: Vec<_> = workdir
            .read_dir()
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["AGENTS.md", "CLAUDE.md"]);
    }
}
