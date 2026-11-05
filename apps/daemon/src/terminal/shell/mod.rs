//! Plain shell terminals the daemon runs for a project, beside its agent sessions: each is the
//! user's login shell on a PTY of its own (`crate::terminal::pty`), held in memory with its record
//! until a client closes it, its project is removed or the daemon exits. A shell that exits on its
//! own leaves its record behind, exited, with its output still replayed to a client that attaches.
//!
//! The records are live state, never stored: carried in `snapshot` and kept current by
//! `shell_terminal_upserted` and `shell_terminal_deleted` (`apps/daemon/PROTOCOL.md`, "Shell
//! terminals").

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::platform::proc::{Containment, Polite};
use crate::platform::shell::ShellCommand;
use crate::platform::term;
use crate::protocol::{error_code, CodedError, Event, ShellTerminal, ShellTerminalState};
use crate::state::AppState;
use crate::terminal::pty::{self, NewPty, PtyCore, PtyOwner};
use crate::time::now_millis;

/// How often a running shell is looked at for having exited and for whether it is busy. Neither
/// has a signal of its own: the daemon waits on nothing a shell's exit closes, and a program
/// running quietly in the foreground prints nothing.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// The shell terminals the daemon holds, on `AppState`.
#[derive(Default)]
pub struct ShellTerminals {
    /// Every change to a record is broadcast while this is held, so no client is sent an older
    /// record of a terminal after a newer one, or after its deletion.
    entries: Mutex<HashMap<String, Entry>>,
}

struct Entry {
    process: Arc<ShellProcess>,
    record: ShellTerminal,
}

impl ShellTerminals {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().expect("shell terminals lock poisoned")
    }

    /// Every record, oldest first, for `snapshot`.
    pub fn list(&self) -> Vec<ShellTerminal> {
        let mut records: Vec<ShellTerminal> = self
            .lock()
            .values()
            .map(|entry| entry.record.clone())
            .collect();
        records.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        records
    }

    /// The terminal's process, for a terminal client to attach to.
    pub fn process(&self, id: &str) -> Option<Arc<ShellProcess>> {
        self.lock().get(id).map(|entry| entry.process.clone())
    }

    /// Whether `process` is still the one held under its id: not closed in the meantime.
    fn holds(&self, process: &Arc<ShellProcess>) -> bool {
        self.lock()
            .get(&process.id)
            .is_some_and(|entry| Arc::ptr_eq(&entry.process, process))
    }
}

/// The instance a shell's containment is made for. Not the process ledger's, so a shell's jobs are
/// never taken for an agent launch's. The containment only adopts the shell (what the busy check
/// reads where the terminal cannot name its foreground job); it is not `prepare`d, so the shell's
/// environment carries no process tag and a user's shell sees the environment it always did.
const CONTAINMENT_INSTANCE: &str = "shell-terminal";

/// A shell running on its PTY. Everything about its process and PTY is its [`PtyCore`]'s, reached
/// through it directly (it dereferences to it).
pub struct ShellProcess {
    pub id: String,
    pty: PtyCore,
    /// What the shell's jobs are told apart from other processes by.
    containment: Containment,
    /// The shell's own name, what its title shows before it sets one.
    shell_name: String,
    titles: Mutex<TitleWatch>,
    /// Whether the shell's titles are the parsed screen's title changes, as where the host renders
    /// what the program draws (`TermFacts::host_renders`), rather than what the output's own title
    /// sequences say.
    host_renders: AtomicBool,
}

#[derive(Default)]
struct TitleWatch {
    parser: crate::terminal::title::Parser,
    /// The last non-blank title the shell set; `None` before it sets one or once it clears it.
    latest: Option<String>,
    /// Where a new title goes, once the terminal is registered there.
    state: Weak<AppState>,
}

impl std::ops::Deref for ShellProcess {
    type Target = PtyCore;

    fn deref(&self) -> &PtyCore {
        &self.pty
    }
}

impl PtyOwner for ShellProcess {
    fn pty(&self) -> &PtyCore {
        &self.pty
    }

    fn on_output(&self, data: &[u8]) {
        let rendered = self.host_renders.load(Ordering::Relaxed);
        if !rendered {
            self.observe_title(|parser| parser.feed(data));
        }
        self.pty.push_output(data, |_, titles| {
            if rendered {
                self.observe_title(|parser| parser.take(&titles));
            }
        });
    }
}

impl ShellProcess {
    /// Takes the title `read` completes, if any, as the shell's latest, and shows it on the
    /// terminal's record unless the terminal has been renamed.
    fn observe_title(
        &self,
        read: impl FnOnce(&mut crate::terminal::title::Parser) -> Option<Option<String>>,
    ) {
        let mut titles = self.titles.lock().expect("title watch lock poisoned");
        if let Some(title) = read(&mut titles.parser) {
            let title = title.filter(|title| !title.trim().is_empty());
            if titles.latest != title {
                titles.latest = title;
                // Under the watch's lock, which registering takes too, so a title read before the
                // terminal was registered cannot overwrite a later one.
                if let Some(state) = titles.state.upgrade() {
                    let shown = shown_title(&self.shell_name, titles.latest.as_deref());
                    update(&state, &self.id, |record| {
                        if !record.renamed {
                            record.title = shown;
                        }
                    });
                }
            }
        }
    }

    /// Reads the shell's titles from now on as a host that renders what the program draws has
    /// them read, for a test of that reading on a host that does not.
    #[cfg(test)]
    pub(crate) fn read_as_rendered(&self) {
        self.pty.report_titles();
        self.host_renders.store(true, Ordering::Relaxed);
    }
}

fn shown_title(shell_name: &str, latest: Option<&str>) -> String {
    latest.unwrap_or(shell_name).to_string()
}

/// Starts a shell terminal for `project`: in its directory, or with `worktree` in its place in that
/// worktree of its repository, resolved as browsing resolves it. A non-empty wire `path` moves the
/// start to that folder below it, and a path that is not a folder is refused. Broadcasts the new
/// record and returns it.
pub async fn create(
    state: &Arc<AppState>,
    project: String,
    worktree: Option<String>,
    path: String,
) -> Result<ShellTerminal> {
    let cwd = {
        let (state, project, worktree) = (state.clone(), project.clone(), worktree.clone());
        tokio::task::spawn_blocking(move || {
            crate::browse::scope::resolve_project_directory(
                &state,
                &project,
                worktree.as_deref(),
                &path,
                &|| state.host_tools.isolated_git(),
            )
        })
        .await??
    };

    let id = uuid::Uuid::new_v4().to_string();
    let tools = state.host_tools.get();
    let process = {
        let (id, cwd) = (id.clone(), cwd.clone());
        tokio::task::spawn_blocking(move || {
            // The agents' policy for the snapshot, but a shell does not need it to be of use: it
            // reads the user's profile itself as it starts.
            let snapshot = match (tools.shell_env)() {
                Ok(env) => Some(env),
                Err(err) => {
                    tracing::warn!(terminal = %id, %err, "the shell environment could not be read; the shell starts with the daemon's own");
                    None
                }
            };
            let inherited = std::env::vars_os().filter_map(|(key, value)| {
                Some((key.into_string().ok()?, value.into_string().ok()?))
            });
            launch(
                &id,
                &cwd,
                &(tools.shell)(),
                shell_environment(snapshot, inherited),
            )
        })
        .await??
    };

    let record = ShellTerminal {
        id: id.clone(),
        project_id: project.clone(),
        worktree,
        cwd: cwd.to_string_lossy().into_owned(),
        title: String::new(),
        renamed: false,
        state: ShellTerminalState::Running,
        busy: false,
        created_at: now_millis(),
    };
    register(state, &process, record);

    // The project may have been removed while the shell started, after its removal ended the
    // project's terminals; this one would then outlive it. A store error says nothing either way,
    // so the terminal is kept on one.
    if let Ok(None) = state.store.get_project(&project) {
        close(state, &id).await.ok();
        return Err(CodedError::unknown_project(&project));
    }
    watch(state.clone(), process);
    state
        .shell_terminals
        .lock()
        .get(&id)
        .map(|entry| entry.record.clone())
        .ok_or_else(|| CodedError::unknown_shell_terminal(&id))
}

/// The environment a shell starts in: the snapshot of the user's shell as it is, or, when none
/// could be taken, `inherited` (the daemon's own) without the variables that mark an agent
/// session, as the snapshot is filtered. `TERM` is the terminal the UI renders with either way.
fn shell_environment(
    snapshot: Option<HashMap<String, String>>,
    inherited: impl Iterator<Item = (String, String)>,
) -> HashMap<String, String> {
    let mut env = snapshot.unwrap_or_else(|| {
        inherited
            .filter(|(key, _)| !crate::env_shell::is_filtered(key))
            .collect()
    });
    env.insert("TERM".to_string(), pty::TERM.to_string());
    env
}

/// Starts `shell` in `cwd` on a PTY of its own, over exactly `env`. **Blocks** on the fork.
fn launch(
    id: &str,
    cwd: &Path,
    shell: &ShellCommand,
    env: HashMap<String, String>,
) -> Result<Arc<ShellProcess>> {
    if !cwd.is_dir() {
        // The terminal backend may fall back to the home directory for a cwd that does not exist.
        return Err(CodedError::raised(
            error_code::DIRECTORY_UNREACHABLE,
            format!(
                "`{}` is not a directory the daemon can reach",
                cwd.display()
            ),
            &[("path", &cwd.to_string_lossy())],
        ));
    }
    let mut command = term::Command::new(&shell.program);
    command.args(&shell.args);
    command.env_clear();
    for (key, value) in &env {
        command.env(key, value);
    }
    command.cwd(cwd);
    let containment = Containment::create(CONTAINMENT_INSTANCE, id)
        .context("creating the shell's process containment")?;
    let spawned = term::spawn(command, pty::INITIAL_SIZE, Some(&containment))
        .with_context(|| format!("starting the shell `{}`", shell.program.display()))?;
    let shell_name = shell.program.file_name().map_or_else(
        || shell.program.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().trim_start_matches('-').to_string(),
    );
    let pty = PtyCore::new(NewPty {
        label: id.to_string(),
        spawned,
        scratch_dir: None,
        kill_session_leftovers: false,
        before_scratch_removal: None,
    });
    let host_renders = term::facts().host_renders;
    if host_renders {
        pty.report_titles();
    }
    let process = Arc::new(ShellProcess {
        id: id.to_string(),
        containment,
        pty,
        shell_name,
        titles: Mutex::new(TitleWatch::default()),
        host_renders: AtomicBool::new(host_renders),
    });
    pty::spawn_reader_thread(process.clone(), pty::READ_BUF);
    Ok(process)
}

/// Holds the terminal and broadcasts it, with the title the shell has already set, if any.
fn register(state: &Arc<AppState>, process: &Arc<ShellProcess>, mut record: ShellTerminal) {
    process.enter_crash_cleanup();
    let mut titles = process.titles.lock().expect("title watch lock poisoned");
    titles.state = Arc::downgrade(state);
    record.title = shown_title(&process.shell_name, titles.latest.as_deref());
    let mut entries = state.shell_terminals.lock();
    entries.insert(
        record.id.clone(),
        Entry {
            process: process.clone(),
            record: record.clone(),
        },
    );
    state.broadcast(Event::ShellTerminalUpserted {
        shell_terminal: record,
    });
}

/// Changes the record of terminal `id`, if it is still held, and broadcasts it when that changed
/// anything.
fn update(state: &AppState, id: &str, change: impl FnOnce(&mut ShellTerminal)) {
    let mut entries = state.shell_terminals.lock();
    let Some(entry) = entries.get_mut(id) else {
        return;
    };
    let before = entry.record.clone();
    change(&mut entry.record);
    if entry.record != before {
        state.broadcast(Event::ShellTerminalUpserted {
            shell_terminal: entry.record.clone(),
        });
    }
}

/// Names terminal `id` `title` (already trimmed and checked not blank), which the shell's own
/// titles no longer change.
pub fn rename(state: &AppState, id: &str, title: String) -> Result<()> {
    if state.shell_terminals.process(id).is_none() {
        return Err(CodedError::unknown_shell_terminal(id));
    }
    update(state, id, |record| {
        record.title = title;
        record.renamed = true;
    });
    Ok(())
}

/// Removes terminal `id` and ends its shell, if it still runs. Returns once the shell is gone.
pub async fn close(state: &AppState, id: &str) -> Result<()> {
    let process = remove(state, id).ok_or_else(|| CodedError::unknown_shell_terminal(id))?;
    end(process).await;
    Ok(())
}

/// Checks the process now, rather than trusting the periodically refreshed record. A missing
/// terminal or a failed check is left alone, so multiple clients can safely clean up together.
pub async fn close_if_idle(state: &AppState, id: &str) {
    let Some(process) = state.shell_terminals.process(id) else {
        return;
    };
    let checked = process.clone();
    let idle =
        tokio::task::spawn_blocking(move || checked.poll_exit() || !busy::is_busy(&checked)).await;
    if !matches!(idle, Ok(true)) {
        return;
    }
    if let Some(process) = remove(state, id) {
        end(process).await;
    }
}

/// Ends every terminal of `project`, which is being removed. Returns once their shells are gone.
pub async fn close_project(state: &AppState, project: &str) {
    close_projects(state, &[project]).await;
}

/// Ends every terminal of each of `projects`, which are being removed, all at once. Returns once
/// their shells are gone.
pub async fn close_projects(state: &AppState, projects: &[&str]) {
    let ids: Vec<String> = state
        .shell_terminals
        .lock()
        .values()
        .filter(|entry| projects.contains(&entry.record.project_id.as_str()))
        .map(|entry| entry.record.id.clone())
        .collect();
    end_all(state, ids).await;
}

/// Ends every terminal, as the daemon exits. Returns once their shells are gone.
pub async fn close_all(state: &AppState) {
    let ids: Vec<String> = state.shell_terminals.lock().keys().cloned().collect();
    end_all(state, ids).await;
}

async fn end_all(state: &AppState, ids: Vec<String>) {
    let ending: Vec<_> = ids
        .iter()
        .filter_map(|id| remove(state, id))
        .map(|process| tokio::spawn(end(process)))
        .collect();
    for ended in ending {
        ended.await.ok();
    }
}

fn remove(state: &AppState, id: &str) -> Option<Arc<ShellProcess>> {
    let mut entries = state.shell_terminals.lock();
    let entry = entries.remove(id)?;
    state.broadcast(Event::ShellTerminalDeleted {
        shell_terminal: id.to_string(),
    });
    Some(entry.process)
}

/// Ends the shell, then hangs its terminal up, which lets every attached client go and leaves
/// nothing of the terminal behind, whatever still holds the PTY's other side open: a job the
/// shell left running, one sent to the background with `nohup` or `disown`, or one that ignores
/// the hangup. Waits out the shell's grace period on a blocking thread.
async fn end(process: Arc<ShellProcess>) {
    let ending = tokio::task::spawn_blocking(move || {
        // A hangup, which is what a terminal closing sends: an interactive shell ignores a request
        // to terminate, and on a hangup it passes the signal on to its jobs before it exits.
        process.stop(Polite::HangUp);
        process.hang_up();
    });
    if let Err(err) = ending.await {
        tracing::warn!(%err, "ending a shell terminal failed");
    }
}

/// Watches a registered shell until it exits or is closed: whether it is busy, kept current on
/// its record, and its exit, which leaves the record exited and lets every attached client go
/// once the shell's last output has reached them.
fn watch(state: Arc<AppState>, process: Arc<ShellProcess>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            if !state.shell_terminals.holds(&process) {
                return;
            }
            if process.poll_exit() {
                break;
            }
            // Off the runtime's workers: the check reads each descendant's descriptors, and on a
            // Linux kernel that keeps no list of children, every process's record.
            let busy = {
                let process = process.clone();
                tokio::task::spawn_blocking(move || busy::is_busy(&process)).await
            };
            if let Ok(busy) = busy {
                update(&state, &process.id, |record| record.busy = busy);
            }
        }
        let drained = {
            let process = process.clone();
            tokio::task::spawn_blocking(move || process.wait_for_reader(pty::FINAL_OUTPUT_DRAIN))
        };
        drained.await.ok();
        update(&state, &process.id, |record| {
            record.state = ShellTerminalState::Exited;
            record.busy = false;
        });
        process.close_output();
    });
}

mod busy;
#[cfg(test)]
mod tests;
