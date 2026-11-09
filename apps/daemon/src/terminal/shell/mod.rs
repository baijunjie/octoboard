//! Plain shell terminals the daemon runs for a project, beside its agent sessions: each is the
//! user's login shell on a PTY of its own (`crate::terminal::pty`), held in memory with its record
//! until a client closes it, its project is removed or the daemon exits. A shell that exits on its
//! own leaves its record behind, exited, with its last screen still painted for a client that
//! attaches.
//!
//! A shell terminal runs in a holder, so it outlives the daemon: the daemon that starts next puts
//! it back from what its meta holds ([`ShellMeta`], [`restore`]), with the name the user gave it,
//! which the store keeps by the terminal's id (`Store::shell_terminal_title`), or else the title
//! the shell sets next. A planned stop leaves the terminals and their records alone: it hands each
//! terminal over to its holder with its screen saved, which the next daemon restores.
//!
//! The records are live state, carried in `snapshot` and kept current by
//! `shell_terminal_upserted` and `shell_terminal_deleted` (`apps/daemon/PROTOCOL.md`, "Shell
//! terminals"); only the user's name for a terminal, and which holder runs it, are stored, and
//! forgotten with the terminal.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::holder::client::HeldTerminal;
use crate::platform::proc::{Containment, Polite};
use crate::platform::shell::ShellCommand;
use crate::platform::term;
use crate::protocol::{
    error_code, CodedError, Event, HolderLink, ShellTerminal, ShellTerminalState,
};
use crate::state::{AppState, ExactScreen};
use crate::terminal::backend::{self, Launching, Started};
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

    /// Every terminal's process, for a planned stop to hand over.
    pub fn processes(&self) -> Vec<Arc<ShellProcess>> {
        self.lock()
            .values()
            .map(|entry| entry.process.clone())
            .collect()
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

/// What a shell terminal's holder keeps of it, which is what a daemon that adopts the terminal
/// puts its record back from; the name the user gave it is the store's, not the holder's.
/// It crosses builds: a daemon of an older build in the holder's compatible range writes it and a
/// newer one reads it, so a field added later needs `#[serde(default)]`, and it is kept readable
/// by every build in that range: `crate::holder::compat::FLOOR` only moves by the
/// previous-stable release rule.
#[derive(Serialize, Deserialize)]
pub struct ShellMeta {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub worktree: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub created_at: i64,
    /// The shell's own name, which its title shows before it sets one.
    #[serde(default)]
    pub shell_name: Option<String>,
}

impl ShellMeta {
    /// The `kind` of a shell terminal's meta.
    pub const KIND: &'static str = "shell_terminal";

    pub fn to_value(&self) -> serde_json::Value {
        let mut meta = serde_json::to_value(self).unwrap_or_default();
        if let Some(fields) = meta.as_object_mut() {
            fields.insert("kind".to_string(), Self::KIND.into());
        }
        meta
    }
}

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
    let created_at = now_millis();
    let tools = state.host_tools.get();
    let process = {
        let (id, cwd) = (id.clone(), cwd.clone());
        let (project, worktree) = (project.clone(), worktree.clone());
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
                ShellMeta {
                    id: id.clone(),
                    project_id: project,
                    worktree,
                    cwd: cwd.to_string_lossy().into_owned(),
                    created_at,
                    shell_name: None,
                },
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
        created_at,
        screen_inexact: false,
        holder_link: None,
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

/// Starts `shell` in `cwd` on a PTY of its own, over exactly `env`, for the terminal `meta`
/// describes. **Blocks** on the fork.
fn launch(
    mut meta: ShellMeta,
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
    let id = meta.id.clone();
    let containment = Containment::create(CONTAINMENT_INSTANCE, &id)
        .context("creating the shell's process containment")?;
    let shell_name = shell.program.file_name().map_or_else(
        || shell.program.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().trim_start_matches('-').to_string(),
    );
    meta.shell_name = Some(shell_name.clone());
    let spawned = backend::start(Launching {
        command,
        size: pty::INITIAL_SIZE,
        containment: &containment,
        instance: CONTAINMENT_INSTANCE,
        launch: &id,
        kill_session_leftovers: false,
        meta: meta.to_value(),
        helpers: None,
        companion: false,
    })
    .with_context(|| format!("starting the shell `{}`", shell.program.display()))?;
    Ok(run(&id, spawned, containment, shell_name, Seed::Nothing))
}

/// Puts a shell terminal the holders kept running back among the terminals: `meta` is what its
/// holder kept of it, `held` the terminal, `exited` whether its shell has exited already, and
/// `tail` the output the holder kept. With `exact`, the screen the previous daemon saved at its
/// planned stop, the screen is restored from it and the tail, which is then only what was written
/// since, is replayed over it as the shell's output; without, the screen and replay are rebuilt
/// from the tail (and so marked inexact). A running shell is made to draw again over a rebuilt
/// screen, and over a saved one marked inexact.
/// A name the user gave it is read back from the store before it is registered, so no title the
/// shell sets, in the tail or after, takes its place. Called by the daemon at start, or once a
/// holder it could not link to then is, once the terminal's project is known to be on record.
pub fn restore(
    state: &Arc<AppState>,
    meta: &ShellMeta,
    held: HeldTerminal,
    exited: bool,
    tail: Vec<u8>,
    exact: Option<ExactScreen>,
) {
    // The shell was put in the containment when it started; the host that lists a containment's
    // members still has it, and one that does not has nothing to list.
    let containment = Containment::reopen(CONTAINMENT_INSTANCE, &meta.id)
        .map_or_else(|| Containment::create(CONTAINMENT_INSTANCE, &meta.id), Ok);
    let containment = match containment {
        Ok(containment) => containment,
        Err(err) => {
            tracing::warn!(terminal = %meta.id, %err, "the shell's process containment could not be made; ending the terminal");
            held.end();
            return;
        }
    };
    let shell_name = meta
        .shell_name
        .clone()
        .unwrap_or_else(|| "shell".to_string());
    let user_title = state
        .store
        .shell_terminal_title(&meta.id)
        .unwrap_or_else(|err| {
            tracing::warn!(terminal = %meta.id, %err, "reading the name the user gave the shell terminal failed");
            None
        });
    // A saved screen marked inexact is drawn again over its restored content, as a rebuilt one is.
    let exact_screen = exact.as_ref().is_some_and(|exact| !exact.saved.inexact);
    let seed = match exact {
        Some(exact) => {
            // Read before any output, as what the shell wrote since the screen was saved.
            held.replay_first(tail);
            Seed::Exact(exact)
        }
        None => Seed::Tail(tail),
    };
    let process = run(&meta.id, Started::Held(held), containment, shell_name, seed);
    let record = ShellTerminal {
        id: meta.id.clone(),
        project_id: meta.project_id.clone(),
        worktree: meta.worktree.clone(),
        cwd: meta.cwd.clone(),
        renamed: user_title.is_some(),
        title: user_title.unwrap_or_default(),
        state: ShellTerminalState::Running,
        busy: false,
        created_at: meta.created_at,
        screen_inexact: process.screen_inexact(),
        holder_link: None,
    };
    register(state, &process, record);
    watch(state.clone(), process.clone());
    // Once the terminal is registered and watched, so what the shell draws in answer reaches
    // everything that follows its output. A screen restored exact is the shell's own already.
    if !exited && !exact_screen {
        if let Err(err) = process.nudge_repaint() {
            tracing::warn!(terminal = %meta.id, %err, "asking the shell to redraw its screen failed");
        }
    }
}

/// What the screen of a shell process starts from, before its reader runs.
enum Seed {
    /// Nothing: a shell this daemon starts.
    Nothing,
    /// The output a holder kept, which the screen and the replay are rebuilt from, inexact.
    Tail(Vec<u8>),
    /// The screen the previous daemon saved, restored as it was, with the rule the output its
    /// reader replays first is taken under.
    Exact(ExactScreen),
}

/// The shell process around the terminal `spawned`, its reader started once its screen is set up
/// from `seed`.
fn run(
    id: &str,
    spawned: Started,
    containment: Containment,
    shell_name: String,
    seed: Seed,
) -> Arc<ShellProcess> {
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
    match seed {
        Seed::Nothing => {}
        Seed::Tail(tail) => {
            pty.seed_output(&tail);
            pty.mark_screen_inexact();
        }
        Seed::Exact(ExactScreen { saved, rule }) => {
            pty.restore_screen(&saved);
            pty.begin_replay(rule);
        }
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
    process
}

/// Holds the terminal and broadcasts it, with the title the shell has already set, if any, unless
/// the user named it. Records which holder runs it.
fn register(state: &Arc<AppState>, process: &Arc<ShellProcess>, mut record: ShellTerminal) {
    process.enter_crash_cleanup();
    if let Some(held_at) = process.held_at() {
        let held = crate::store::HeldTerminalRecord {
            kind: crate::store::HeldKind::ShellTerminal,
            owner_id: record.id.clone(),
            holder_id: held_at.holder_id,
            terminal_id: held_at.terminal_id,
            launch_id: record.id.clone(),
        };
        if let Err(err) = state.store.record_held_terminal(&held) {
            tracing::warn!(terminal = %record.id, %err, "recording where the shell terminal runs failed");
        }
    }
    // Its holder can be out of reach for a while, the shell going on: clients are told. Read when
    // the record is changed, under its lock, so the last change leaves what holds now; asked for
    // before the record is held, which reads it under the same lock, so no change goes unseen.
    let (weak_state, weak_process) = (Arc::downgrade(state), Arc::downgrade(process));
    process.on_link_changed(move || {
        let (Some(state), Some(process)) = (weak_state.upgrade(), weak_process.upgrade()) else {
            return;
        };
        update(&state, &process.id, |record| {
            record.holder_link = process.holder_unlinked().then_some(HolderLink::Pending);
        });
    });
    let mut titles = process.titles.lock().expect("title watch lock poisoned");
    titles.state = Arc::downgrade(state);
    if !record.renamed {
        record.title = shown_title(&process.shell_name, titles.latest.as_deref());
    }
    let mut entries = state.shell_terminals.lock();
    record.holder_link = process.holder_unlinked().then_some(HolderLink::Pending);
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
    // Its holder's link dropping loses what the shell prints meanwhile: the screen is no longer
    // exact, which clients are told, and the shell is made to draw it again.
    let (weak_state, weak_process) = (Arc::downgrade(state), Arc::downgrade(process));
    process.on_resumed(move || {
        let (Some(state), Some(process)) = (weak_state.upgrade(), weak_process.upgrade()) else {
            return;
        };
        process.mark_screen_inexact();
        update(&state, &process.id, |record| record.screen_inexact = true);
        if let Err(err) = process.nudge_repaint() {
            tracing::warn!(terminal = %process.id, %err, "asking the shell to redraw its screen failed");
        }
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
/// titles no longer change. The name is stored before it is published, under the terminals'
/// lock, so a daemon that puts the terminal back from its holder shows it again, and a close
/// cannot slip in between and leave it stored for a terminal that is gone.
pub fn rename(state: &AppState, id: &str, title: String) -> Result<()> {
    let mut entries = state.shell_terminals.lock();
    let Some(entry) = entries.get_mut(id) else {
        return Err(CodedError::unknown_shell_terminal(id));
    };
    state.store.set_shell_terminal_title(id, &title)?;
    let before = entry.record.clone();
    entry.record.title = title;
    entry.record.renamed = true;
    if entry.record != before {
        state.broadcast(Event::ShellTerminalUpserted {
            shell_terminal: entry.record.clone(),
        });
    }
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
    end_all(state, ids, |_| true).await;
}

/// Ends every terminal, as the daemon exits. Returns once their shells are gone.
pub async fn close_all(state: &AppState) {
    close_all_ending(state, |_| true).await;
}

/// [`close_all`] for an ordinary stop that escalates a planned stop already begun: every terminal
/// is taken out and forgotten, but only those `ends` picks are ended here. The others are held by
/// a holder the planned stop handed terminals over from or parted from, which nothing is sent to
/// terminal by terminal any more: their shells end with that holder, which the caller ends.
pub async fn close_all_ending(state: &AppState, ends: impl Fn(&ShellProcess) -> bool) {
    let ids: Vec<String> = state.shell_terminals.lock().keys().cloned().collect();
    end_all(state, ids, ends).await;
}

/// Takes each terminal of `ids` out of the terminals and forgets it, and ends those `ends` picks.
/// Returns once their shells are gone.
async fn end_all(state: &AppState, ids: Vec<String>, ends: impl Fn(&ShellProcess) -> bool) {
    let ending: Vec<_> = ids
        .iter()
        .filter_map(|id| remove(state, id))
        .filter(|process| ends(process))
        .map(|process| tokio::spawn(end(process)))
        .collect();
    for ended in ending {
        ended.await.ok();
    }
}

/// Takes terminal `id` out of the terminals, and forgets its stored name and where it runs.
fn remove(state: &AppState, id: &str) -> Option<Arc<ShellProcess>> {
    let mut entries = state.shell_terminals.lock();
    let entry = entries.remove(id)?;
    if let Err(err) = state.store.forget_shell_terminal(id) {
        tracing::warn!(terminal = %id, %err, "forgetting the shell terminal's name failed");
    }
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
            // A terminal handed over is the next daemon's to watch: an exit seen from here on is
            // its to find.
            if !state.shell_terminals.holds(&process) || state.is_handing_over() {
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
