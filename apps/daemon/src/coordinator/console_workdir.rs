//! A console's working directory: its canonical instruction content exposed under each
//! provider's declared filename by a link, an import or a copy, and its removal with the console.
//! The text is built elsewhere; this puts it on disk.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};

use crate::hostfs::replace::{replace_file, Permissions};
use crate::platform::fs as platform;
use crate::protocol::Console;
use crate::session_tools::console_instructions::{console_session_instructions, INSTRUCTION_FILE};
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
/// lock that keeps concurrent refreshes from leaving an older record's text on disk, with the
/// memory text the stored `memory_enabled` calls for.
pub fn refresh_console_session_instructions(store: &Store, console_id: &str) -> Result<()> {
    refresh_console_session_instructions_with(store, console_id, store.memory_enabled()?)
}

/// [`refresh_console_session_instructions`] with the memory text `memory_enabled` calls for: a
/// launch reads the setting once and passes the same value here and to the agent, so the file,
/// the tools offered and the agent's own memory never follow two different values of it.
pub fn refresh_console_session_instructions_with(
    store: &Store,
    console_id: &str,
    memory_enabled: bool,
) -> Result<()> {
    let _writing = INSTRUCTION_FILE_WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let console = store.require_console(console_id)?;
    write_console_session_instructions_from_settings(store, &console, memory_enabled)
}

/// Writes the console's instruction file with the guidance the stored settings call for (asking
/// as choices, and replying in the interface's language) and, with `memory_enabled`, the index of
/// the console's memory as it is stored now. No lock: see [`write_console_session_instructions`].
pub fn write_console_session_instructions_from_settings(
    store: &Store,
    console: &Console,
    memory_enabled: bool,
) -> Result<()> {
    let user_language = store.interface_language()?;
    let memory = if memory_enabled {
        store.memory_index(&crate::protocol::MemoryScope::Console {
            console_id: console.id.clone(),
        })?
    } else {
        Vec::new()
    };
    write_console_session_instructions(
        console,
        &RoleOptions {
            ask_as_choices: store.ask_questions_as_choices()?,
            user_language: user_language.as_deref(),
            memory: &memory,
            memory_enabled,
            ..RoleOptions::default()
        },
    )
}

/// Writes the canonical instruction content, then exposes it under each provider's declared
/// filename. Links let a provider that reads several filenames recognise one file. Where the
/// host makes no file links, a provider with an import syntax gets a reference to the canonical
/// file; a provider with none gets a copy refreshed on every write. Each form replaces what the
/// other left, so a host that gains or loses file links leaves no stale name. Atomic replacements
/// ensure concurrent
/// launches never read a truncated file, including when they run different agents in the same
/// console. Not locked: a console no one has been told of yet has no other writer, and every other
/// caller goes through [`refresh_console_session_instructions`]. The working directory must exist:
/// it is created with the console and removed with it, never recreated by a write. `options`
/// carries the application settings that add to the text; its worktrees are ignored.
fn write_console_session_instructions(console: &Console, options: &RoleOptions) -> Result<()> {
    let workdir = Path::new(&console.workdir);
    let text = console_session_instructions(console.console_session_prompt.as_deref(), options);
    write_instruction_file(workdir, INSTRUCTION_FILE, &text)?;

    let file_links = file_links_available();
    let mut written: Vec<&str> = Vec::new();
    for descriptor in crate::providers::descriptors() {
        let name = descriptor.instruction_file;
        if name == INSTRUCTION_FILE || written.contains(&name) {
            continue;
        }
        written.push(name);
        if file_links {
            link_instruction_file(workdir, name)?;
        } else if let Some(prefix) = descriptor.instruction_import_prefix {
            write_instruction_file(workdir, name, &format!("{prefix}{INSTRUCTION_FILE}\n"))?;
        } else {
            write_instruction_file(workdir, name, &text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// [`platform::file_links_available`] for a test, on the test's own thread.
    pub(super) static FILE_LINKS_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Whether the host makes file links ([`platform::file_links_available`]), or a test's override of
/// it. Read on the thread that writes the files.
fn file_links_available() -> bool {
    #[cfg(test)]
    if let Some(available) = FILE_LINKS_OVERRIDE.get() {
        return available;
    }
    platform::file_links_available()
}

/// Writes `text` to `name` in `workdir`, replacing whatever is there (a link included) in one
/// rename, and keeping the permissions the file had.
fn write_instruction_file(workdir: &Path, name: &str, text: &str) -> Result<()> {
    let path = workdir.join(name);
    replace_file(
        &path,
        &temp_name_beside(workdir, name),
        text.as_bytes(),
        Permissions::KeptOrDefault,
    )
    .with_context(|| format!("writing {}", path.display()))
}

/// Points `name` in `workdir` at [`INSTRUCTION_FILE`]. A symlink that already points at the file
/// is left alone. Anything else at that name — an earlier build's regular file, or a link
/// elsewhere — is replaced by one made under a temporary name and renamed over it, so the name
/// never goes missing.
fn link_instruction_file(workdir: &Path, name: &str) -> Result<()> {
    let link = workdir.join(name);
    if platform::read_alias(&link)
        .ok()
        .flatten()
        .is_some_and(|target| target == Path::new(INSTRUCTION_FILE))
    {
        return Ok(());
    }
    let temp_link = temp_name_beside(workdir, name);
    platform::alias_file(Path::new(INSTRUCTION_FILE), &temp_link)
        .map(drop)
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
    use crate::protocol::AgentId;
    use crate::test_support::ScratchDir;

    fn console(agent: AgentId, workdir: &Path) -> Console {
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
            versions: Default::default(),
        }
    }

    /// Every agent reads the console's one instruction file: `CLAUDE.md` is a relative link to
    /// `AGENTS.md`, an earlier build's regular `CLAUDE.md` or a link elsewhere is replaced, and
    /// the user's text is in the file only while there is some.
    #[test]
    fn a_console_has_one_instruction_file_and_a_link_to_it() {
        let workdir = ScratchDir::new("console-instructions");
        std::fs::write(workdir.join("CLAUDE.md"), "an earlier build's file").unwrap();
        let mut console = console(AgentId::CLAUDE, &workdir);
        // The file itself and one link per other name a registered provider reads.
        let names: std::collections::HashSet<_> = crate::providers::descriptors()
            .map(|descriptor| descriptor.instruction_file)
            .chain([INSTRUCTION_FILE])
            .collect();
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
                std::fs::read_to_string(workdir.join(INSTRUCTION_FILE)).unwrap(),
                expected
            );
            for descriptor in crate::providers::descriptors() {
                assert_eq!(
                    std::fs::read_link(workdir.join(descriptor.instruction_file)).unwrap(),
                    Path::new(INSTRUCTION_FILE)
                );
                assert_eq!(
                    std::fs::read_to_string(workdir.join(descriptor.instruction_file)).unwrap(),
                    expected
                );
            }
            assert_eq!(
                std::fs::read_link(workdir.join("CLAUDE.md")).unwrap(),
                Path::new(INSTRUCTION_FILE)
            );
            assert_eq!(
                std::fs::read_to_string(workdir.join("CLAUDE.md")).unwrap(),
                expected
            );
            assert_eq!(workdir.read_dir().unwrap().count(), names.len());
        }
        std::fs::remove_file(workdir.join("CLAUDE.md")).unwrap();
        crate::test_support::symlink("elsewhere", workdir.join("CLAUDE.md")).unwrap();
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
        assert_eq!(
            std::fs::read_link(workdir.join("CLAUDE.md")).unwrap(),
            Path::new(INSTRUCTION_FILE)
        );
    }

    /// Where the host makes no file links, every instruction file is a regular file, in place of
    /// the links a run that made them left: `CLAUDE.md` is Claude Code's import line for
    /// `AGENTS.md`, and the test provider's file, a provider with no import line, a copy of the
    /// text rewritten with it. Once the host makes file links again, links replace them.
    #[test]
    fn a_console_without_file_links_writes_regular_instruction_files() {
        let workdir = ScratchDir::new("console-instructions-copied");
        let mut console = console(AgentId::CLAUDE, &workdir);
        let others = ["CLAUDE.md", "TEST_AGENT.md"];
        for name in others {
            crate::test_support::symlink("AGENTS.md", workdir.join(name)).unwrap();
        }
        let regular_file = |name: &str| {
            let path = workdir.join(name);
            assert!(
                std::fs::symlink_metadata(&path).unwrap().is_file(),
                "{name}"
            );
            std::fs::read_to_string(path).unwrap()
        };
        let names: std::collections::HashSet<_> = crate::providers::descriptors()
            .map(|descriptor| descriptor.instruction_file)
            .chain([INSTRUCTION_FILE])
            .collect();

        FILE_LINKS_OVERRIDE.set(Some(false));
        for prompt in [None, Some("Be brief.")] {
            console.console_session_prompt = prompt.map(str::to_string);
            write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
            let expected = console_session_instructions(prompt, &RoleOptions::default());
            assert_eq!(regular_file(INSTRUCTION_FILE), expected);
            assert_eq!(regular_file("AGENTS.md"), expected);
            assert_eq!(regular_file("CLAUDE.md"), format!("@{INSTRUCTION_FILE}\n"));
            assert_eq!(regular_file("TEST_AGENT.md"), expected);
            assert_eq!(workdir.read_dir().unwrap().count(), names.len());
        }

        FILE_LINKS_OVERRIDE.set(Some(true));
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
        for name in others {
            assert_eq!(
                std::fs::read_link(workdir.join(name)).unwrap(),
                Path::new(INSTRUCTION_FILE)
            );
        }
        assert_eq!(workdir.read_dir().unwrap().count(), names.len());
    }

    /// A refresh writes the console's memory index as it is stored at that moment, so a console
    /// session launched after another saved an entry starts with it in its index.
    #[test]
    fn a_refresh_writes_the_stored_memory_index() {
        use crate::protocol::{MemoryDraft, MemoryScope, MemoryWriter};

        let (state, dir) = crate::test_support::app_state("console-instructions-memory");
        let console = crate::test_support::console(&dir);
        std::fs::create_dir_all(&console.workdir).unwrap();
        state.store.insert_console(&console).unwrap();
        // The index is written only while Octoboard's memory is on, which it is not by default.
        state.store.set_memory_enabled(true).unwrap();
        refresh_console_session_instructions(&state.store, &console.id).unwrap();
        let file = Path::new(&console.workdir).join(INSTRUCTION_FILE);
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .contains("(no entries yet)"));

        let draft = MemoryDraft {
            summary: "Web requests go to site".to_string(),
            content: "the entry's own body".to_string(),
            local: None,
        };
        let writer = MemoryWriter::User;
        let saved = crate::coordinator::memory::save_memory(
            &state.store,
            &MemoryScope::Console {
                console_id: console.id.clone(),
            },
            &crate::protocol::MemoryTarget::Create,
            draft,
            &writer,
        )
        .unwrap()
        .unwrap();
        refresh_console_session_instructions(&state.store, &console.id).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains(&format!(
                "- `{}` (updated {}): Web requests go to site",
                saved.id,
                crate::time::utc_date(saved.updated_at)
            )),
            "{text}"
        );
        assert!(!text.contains("(no entries yet)"));
        assert!(!text.contains("the entry's own body"));
    }

    /// A refresh follows the memory setting as it is stored at that moment: while it is off, the
    /// file says nothing of memory, not even of an entry the console holds; turned on, the index is
    /// in it; turned off again, it is gone and the entry is still held.
    #[test]
    fn a_refresh_writes_the_memory_text_only_while_memory_is_on() {
        use crate::protocol::{MemoryDraft, MemoryScope, MemoryTarget, MemoryWriter};

        let (state, dir) = crate::test_support::app_state("console-instructions-memory-switch");
        let console = crate::test_support::console(&dir);
        std::fs::create_dir_all(&console.workdir).unwrap();
        state.store.insert_console(&console).unwrap();
        let scope = MemoryScope::Console {
            console_id: console.id.clone(),
        };
        let saved = crate::coordinator::memory::save_memory(
            &state.store,
            &scope,
            &MemoryTarget::Create,
            MemoryDraft {
                summary: "Web requests go to site".to_string(),
                content: "the entry's own body".to_string(),
                local: None,
            },
            &MemoryWriter::User,
        )
        .unwrap()
        .unwrap();
        let file = Path::new(&console.workdir).join(INSTRUCTION_FILE);
        let refreshed = || {
            refresh_console_session_instructions(&state.store, &console.id).unwrap();
            std::fs::read_to_string(&file).unwrap()
        };
        let says_nothing_of_memory = |text: &str| {
            for absent in [
                "## Memory",
                "console_memory",
                saved.id.as_str(),
                "Web requests go to site",
            ] {
                assert!(!text.contains(absent), "{absent}");
            }
            for tool in crate::session_tools::catalogue::MEMORY_TOOLS {
                assert!(!text.contains(tool), "{tool}");
            }
        };

        says_nothing_of_memory(&refreshed());
        state.store.set_memory_enabled(true).unwrap();
        let on = refreshed();
        assert!(on.contains("<console_memory>"), "{on}");
        assert!(on.contains(&saved.id), "{on}");
        assert!(on.contains("Web requests go to site"), "{on}");
        state.store.set_memory_enabled(false).unwrap();
        says_nothing_of_memory(&refreshed());
        assert_eq!(state.store.memories(&scope).unwrap().len(), 1);
    }

    /// A refresh given the setting's value follows that value, not the one stored meanwhile, so a
    /// launch's file agrees with the tools and the agent's own memory it read the setting for.
    #[test]
    fn a_refresh_given_the_setting_follows_it_over_the_stored_one() {
        let (state, dir) = crate::test_support::app_state("console-instructions-memory-given");
        let console = crate::test_support::console(&dir);
        std::fs::create_dir_all(&console.workdir).unwrap();
        state.store.insert_console(&console).unwrap();
        let file = Path::new(&console.workdir).join(INSTRUCTION_FILE);
        let read = || std::fs::read_to_string(&file).unwrap();

        state.store.set_memory_enabled(true).unwrap();
        refresh_console_session_instructions_with(&state.store, &console.id, false).unwrap();
        assert!(!read().contains("save_memory"), "{}", read());

        state.store.set_memory_enabled(false).unwrap();
        refresh_console_session_instructions_with(&state.store, &console.id, true).unwrap();
        assert!(read().contains("(no entries yet)"), "{}", read());
    }

    /// Rewriting the instruction file keeps the permissions the user gave it.
    #[cfg(unix)]
    #[test]
    fn a_rewritten_instruction_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let workdir = ScratchDir::new("console-instructions-mode");
        let console = console(AgentId::CLAUDE, &workdir);
        write_console_session_instructions(&console, &RoleOptions::default()).unwrap();
        let file = workdir.join(INSTRUCTION_FILE);
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
        let console = console(AgentId::CLAUDE, &workdir);
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
        assert_eq!(names, ["CLAUDE.md", INSTRUCTION_FILE]);
    }
}
