//! Stand-in live sessions on a PTY, the fake agent CLIs they run, and the reads of what reached
//! them.

use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::background::Background;
#[cfg(unix)]
use super::perl_write;
use super::{is_running, wait_for_pid, within, PATIENCE};
use crate::protocol::AgentId;
use crate::session::{LiveSession, NewSession};
use crate::state::AppState;

/// A real Claude Code interrupt entry, from a turn interrupted with Esc while the model was writing
/// on 2.1.303, with only its `cwd` replaced. Stamped as it was captured: long before any watch a
/// test arms.
pub(crate) fn captured_interrupt() -> &'static str {
    include_str!("../../testdata/transcript_interrupt.jsonl")
}

/// [`captured_interrupt`] stamped later than any watch a test arms, since a transcript watch counts
/// an interrupt only from the moment it was armed.
pub(crate) fn fresh_interrupt() -> String {
    const CAPTURED_AT: &str = "\"timestamp\":\"2026-10-19T05:22:20.382Z\"";
    assert!(captured_interrupt().contains(CAPTURED_AT));
    captured_interrupt().replace(CAPTURED_AT, "\"timestamp\":\"2100-01-01T00:00:00.000Z\"")
}

/// Claude Code's view of a background subagent as drawn on the screen.
pub(crate) const SUBAGENT_VIEW: &str = "\u{276F} Message @general-purpose\u{2026}\n\
    \x20 \u{25EF} main\n  \u{23FA} general-purpose  Slow probe agent  8s\n";

/// Puts [`SUBAGENT_VIEW`] on `live`'s screen and asserts it is recognised as a view of the
/// background agents, which shows the session dormant.
pub(crate) fn show_agents_view(live: &LiveSession) {
    live.on_output(SUBAGENT_VIEW.as_bytes());
    assert!(live
        .look_for_dialog()
        .current
        .is_some_and(|dialog| dialog.is_agents_view()));
}

/// The idle stand-in's process: one process and nothing it started, as an agent at rest is, so
/// that what a restart lists as the processes the agent started is only what a test started. It
/// lives until its guard terminates it: a fixed lifetime can expire while a process-heavy test is
/// still preparing the messages it expects the stand-in to receive.
///
/// On Windows what is written into a console shows only once a process reads it, where a Unix
/// terminal echoes it as it arrives, so the stand-in reads its input. It is a native process
/// rather than a Git Bash script: Git Bash starting under load can fail to set up its mount table
/// and end at once, which a test reads as a session that never took its message.
fn idle_command() -> crate::platform::term::Command {
    #[cfg(unix)]
    {
        let mut cmd = crate::platform::term::Command::new(super::bash());
        cmd.args(["-c", "exec /usr/bin/perl -e 'sleep 3600 while 1'"]);
        cmd
    }
    #[cfg(windows)]
    super::fixture::Steps::new()
        .read_input()
        .program("idle")
        .term_command()
}

/// A stand-in live session that stays idle on a PTY until its guard drops, with nothing reading it.
pub(crate) fn idle_stand_in(id: &str) -> StandIn {
    idle_stand_in_of(id, AgentId::CLAUDE)
}

/// [`idle_stand_in`] standing in for `agent`.
pub(crate) fn idle_stand_in_of(id: &str, agent: AgentId) -> StandIn {
    let live = stand_in_of(id, agent, 80, 24, idle_command(), None, None);
    live.trust.end_watch();
    live
}

/// A stand-in session for `id` that is registered with `state` and has its output read, so a test
/// can wait for what was written into it with [`output_containing`].
pub(crate) fn running(state: &Arc<AppState>, id: &str) -> StandIn {
    let live = idle_stand_in(id);
    state.register_live(live.clone());
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    live
}

/// A stand-in agent for session `id` that starts `commands` in the background and then stays up
/// until its guard ends it.
pub(crate) fn stand_in_starting(id: &str, commands: Vec<Background>) -> StandIn {
    #[cfg(unix)]
    {
        let lines: String = commands.iter().map(Background::line).collect();
        fake_live_session(
            id,
            AgentId::CLAUDE,
            80,
            24,
            &format!("{lines}exec sleep 300"),
        )
    }
    #[cfg(windows)]
    {
        let steps = commands
            .into_iter()
            .fold(super::fixture::Steps::new(), |steps, command| {
                command.start(steps)
            });
        stand_in_running_steps(id, steps.read_input())
    }
}

/// A stand-in agent for session `id` that runs `steps`, as a Windows process of its own.
#[cfg(windows)]
pub(crate) fn stand_in_running_steps(id: &str, steps: super::fixture::Steps) -> StandIn {
    fake_live_session_running(id, AgentId::CLAUDE, 80, 24, &steps.program("agent"))
}

/// A stand-in process for the stored session `id` whose shell command leaves one process running
/// in a terminal session of its own, as an agent's dev server is; registered and watched as a
/// launched session's is. Answers the stand-in and that process's pid.
pub(crate) fn running_leaving_one(state: &Arc<AppState>, dir: &Path, id: &str) -> (StandIn, i32) {
    let pid_file = dir.join(format!("{id}-child"));
    let live = stand_in_starting(id, vec![Background::sleeper(&pid_file)]);
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(state, live.clone());
    let child = wait_for_pid(&pid_file);
    (live, child)
}

/// What the restart's list of the processes it ended starts with after "started: ", for the
/// process [`running_leaving_two`] has it end.
pub(crate) fn doomed_command_starts() -> String {
    if cfg!(windows) {
        fixture_command_starts("doomed")
    } else {
        "/usr/bin/perl".to_string()
    }
}

/// What the command of the process [`running_leaving_two`] leaves running starts with, which the
/// restart's list of what it ended does not hold.
pub(crate) fn survivor_command_starts() -> String {
    if cfg!(windows) {
        fixture_command_starts("survivor")
    } else {
        "/bin/sleep".to_string()
    }
}

/// What the command of a Windows fixture process called `name` starts with: the test binary and
/// the name, which a command cut to the length a notice quotes still holds.
fn fixture_command_starts(name: &str) -> String {
    let exe = std::env::current_exe().expect("the test binary");
    format!("{} {name}", exe.display())
}

/// [`running_leaving_one`] with a second process, in a terminal session of its own as well, that
/// the stand-in ends as it shuts down, as an agent's app server closes its background terminals.
/// Answers the stand-in, the process that outlives it and the one that does not.
///
/// The stand-in takes `SIGTERM`, the signal a restart stops it with, by ending that process and
/// reaping it before it exits itself, so the process is gone by the time the stand-in's end is
/// recorded (on Windows, the close of its console). A process left to notice the end by itself can
/// still be running when the stand-in's end is observed. It is ready only once its handler is in
/// place, which it says by writing a file. The process still exits by itself once the stand-in is
/// gone some other way, so it never outlives the test.
pub(crate) fn running_leaving_two(
    state: &Arc<AppState>,
    dir: &Path,
    id: &str,
) -> (StandIn, i32, i32) {
    let survivor_file = dir.join(format!("{id}-survivor"));
    let doomed_file = dir.join(format!("{id}-doomed"));
    let ready_file = dir.join(format!("{id}-ready"));
    #[cfg(unix)]
    let live = fake_live_session(
        id,
        AgentId::CLAUDE,
        80,
        24,
        &format!(
            "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n\
             /usr/bin/perl -MPOSIX -e 'my $agent = getppid(); POSIX::setsid(); {} \
             select(undef, undef, undef, 0.05) while kill 0, $agent;' &\n\
             exec /usr/bin/perl -e 'my $doomed = $ARGV[0]; \
             $SIG{{TERM}} = sub {{ kill \"TERM\", $doomed; waitpid($doomed, 0); exit 0 }}; \
             {} sleep 300 while 1;' \"$!\"",
            perl_write(&survivor_file, "$$"),
            perl_write(&doomed_file, "$$"),
            perl_write(&ready_file, "$$"),
        ),
    );
    #[cfg(windows)]
    let live = {
        use super::fixture::Steps;
        let steps = Steps::new()
            .spawn(
                "survivor",
                Steps::new().write_pid(&survivor_file).sleep(300),
            )
            .spawn(
                "doomed",
                Steps::new()
                    .end_with_parent()
                    .write_pid(&doomed_file)
                    .sleep(300),
            )
            .end_child_on_close()
            .write_pid(&ready_file)
            .read_input();
        stand_in_running_steps(id, steps)
    };
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(state, live.clone());
    let survivor = wait_for_pid(&survivor_file);
    let doomed = wait_for_pid(&doomed_file);
    wait_for_pid(&ready_file);
    (live, survivor, doomed)
}

/// Waits for `pid` to stop running, off the runtime, ticking the ledger's scanner in place of its
/// thread, which a test's ledger does not start.
pub(crate) async fn stops_running(state: &Arc<AppState>, pid: i32) -> bool {
    let ledger = Arc::clone(&state.process_ledger);
    within(PATIENCE, move || {
        ledger.tick_now();
        !is_running(pid)
    })
    .await
}

/// Waits until the exit watcher is done with the end of `live`'s process, so that whatever it
/// would send the session's owner is sent: what an absence asserted behind
/// [`heard_after_marker`] needs, since the watcher tells the owner only after recording the end.
pub(crate) async fn exit_settled(live: &Arc<LiveSession>) {
    let live = live.clone();
    assert!(
        within(PATIENCE, move || live.is_exit_settled()).await,
        "the exit watcher was done with the end"
    );
}

/// What a stand-in has printed, from the last `max_bytes` of its output: without what the host
/// wrote of its own accord before the stand-in printed anything, while that is still in them.
pub(crate) fn printed(live: &LiveSession, max_bytes: usize) -> Vec<u8> {
    let output = live.recent_output(max_bytes);
    match output.strip_prefix(crate::platform::term::host_preamble()) {
        Some(printed) => printed.to_vec(),
        None => output,
    }
}

/// How long [`output_containing`] waits, once its needle is there, for the rest of what came with
/// it.
const REST_OF_THE_ECHO: Duration = Duration::from_secs(2);

/// Polls a stand-in session's output until it holds `needle`, and then until it has gone quiet,
/// so that the rest of what was written with the needle is there too: on a Windows console what is
/// written shows only as the process reads it, in pieces, where a Unix terminal echoes a write
/// whole as it arrives.
pub(crate) fn output_containing(live: &LiveSession, needle: &str) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let output = String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
        if output.contains(needle) {
            crate::session::wait::wait_quiet(live, REST_OF_THE_ECHO);
            return String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
        }
        if Instant::now() >= deadline {
            return output;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What a stand-in session has heard once a marker written to it now has arrived, so that an
/// absence asserted in it is one of a reader that has caught up (its reader thread fills the output
/// some time after a write). The session must have a process that takes messages.
///
/// Each call writes a marker of its own: one a call before left in the output would be found at
/// once, before the reader has caught up with what was written since.
pub(crate) fn heard_after_marker(state: &Arc<AppState>, live: &LiveSession) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let marker = format!(
        "marker-after-the-call-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    crate::messaging::delivery::write_message(
        state,
        &live.id,
        &marker,
        crate::messaging::delivery::WhenBlocked::Queue,
    )
    .unwrap();
    let heard = output_containing(live, &marker);
    assert!(heard.contains(&marker), "{heard}");
    heard
}

/// A stand-in live session on a real PTY, with `/bin/bash` running `script` behind it instead of a
/// real agent: enough for whatever a test needs a `LiveSession` to write into, read from, or just
/// stay alive on. `LiveSession` has no `Drop` that ends its process, so this guard does: the
/// process is terminated when the guard drops, which a test that fails part-way reaches too.
/// Dereferences to the `Arc<LiveSession>`, so `live.clone()` registers it with an `AppState`.
/// Declare it after any `ScratchDir` its script uses, so the process ends before the directory
/// goes.
pub(crate) struct StandIn(Arc<LiveSession>);

impl StandIn {
    pub(crate) fn with_composer(mut self) -> Self {
        Arc::get_mut(&mut self.0)
            .expect("configure before registering the stand-in")
            .checks_composer = true;
        self
    }

    pub(crate) fn with_transport(
        mut self,
        transport: Arc<dyn crate::session::term::MessageTransport>,
    ) -> Self {
        Arc::get_mut(&mut self.0)
            .expect("configure before registering the stand-in")
            .transport = Some(transport);
        self
    }

    pub(crate) fn with_auto_review(mut self) -> Self {
        Arc::get_mut(&mut self.0)
            .expect("configure before registering the stand-in")
            .resolves_approvals_itself = true;
        self
    }
}

impl Deref for StandIn {
    type Target = Arc<LiveSession>;

    fn deref(&self) -> &Arc<LiveSession> {
        &self.0
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.0.terminate();
    }
}

/// A server of the session's own standing in for Codex's that declines every message, as one does
/// that cannot take it: nothing of the message reached it. It runs `during_send` while the
/// request is under way, which is where a test raises a planned stop's handover barrier, and
/// keeps what it was offered.
pub(crate) struct DecliningServer {
    during_send: Box<dyn Fn() + Send + Sync>,
    offered: std::sync::Mutex<Vec<String>>,
}

impl DecliningServer {
    pub(crate) fn new(during_send: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            during_send: Box::new(during_send),
            offered: Default::default(),
        })
    }

    /// The texts the server was offered, in order.
    pub(crate) fn offered(&self) -> Vec<String> {
        self.offered.lock().unwrap().clone()
    }
}

impl crate::session::term::MessageTransport for DecliningServer {
    fn send(
        &self,
        text: &str,
        _: &str,
    ) -> Result<crate::session::term::Sent, crate::platform::term::PartialWrite> {
        self.offered.lock().unwrap().push(text.to_string());
        (self.during_send)();
        Ok(crate::session::term::Sent::Declined(
            "the server is busy".to_string(),
        ))
    }
    fn attach(&self, _: crate::session::term::TransportEvents) {}
    fn start_queue(&self) -> crate::session::term::QueueStart {
        crate::session::term::QueueStart::Nothing
    }
    fn stop(&self) {}
}

/// A server of the session's own that takes every message, as one does that is ready for it:
/// nothing is written into the terminal, and the texts it was offered are kept in order, for a
/// test to read what was delivered and when.
pub(crate) struct AcceptingServer {
    offered: std::sync::Mutex<Vec<String>>,
}

impl AcceptingServer {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            offered: Default::default(),
        })
    }

    /// The texts the server was offered, in order.
    pub(crate) fn offered(&self) -> Vec<String> {
        self.offered.lock().unwrap().clone()
    }
}

impl crate::session::term::MessageTransport for AcceptingServer {
    fn send(
        &self,
        text: &str,
        _: &str,
    ) -> Result<crate::session::term::Sent, crate::platform::term::PartialWrite> {
        self.offered.lock().unwrap().push(text.to_string());
        Ok(crate::session::term::Sent::Queued)
    }
    fn attach(&self, _: crate::session::term::TransportEvents) {}
    fn start_queue(&self) -> crate::session::term::QueueStart {
        crate::session::term::QueueStart::Nothing
    }
    fn stop(&self) {}
}

pub(crate) fn fake_live_session(
    id: &str,
    agent: AgentId,
    cols: u16,
    rows: u16,
    script: &str,
) -> StandIn {
    let live = watched_live_session(id, agent, cols, rows, script, None);
    // Past its trust screen, as a session that has run a hook is, so that messages are written
    // into it rather than held.
    live.trust.end_watch();
    live
}

/// [`fake_live_session`] running `program` rather than a Bash script.
pub(crate) fn fake_live_session_running(
    id: &str,
    agent: AgentId,
    cols: u16,
    rows: u16,
    program: &crate::test_support::programs::Program,
) -> StandIn {
    let live = stand_in_of(id, agent, cols, rows, program.term_command(), None, None);
    live.trust.end_watch();
    live
}

/// What a stand-in launch was asked for: the session, its owner and its opening prompt.
pub(crate) type Launched = (String, Option<crate::session_tools::Owner>, Option<String>);

/// A launch that starts a stand-in process for the session asked for, for the operations that
/// start an agent (`term::launch` cannot in a test). The process is kept in `started` and the
/// request recorded in `launches`, for the test to read; `during` runs first, as what happens
/// while the agent is being started.
///
/// The stand-in is the launch the ledger was told of: it carries the request's process tag in its
/// environment and as its launch id, as an agent `term::launch` starts does.
pub(crate) fn stand_in_launch(
    started: &Arc<std::sync::Mutex<Vec<StandIn>>>,
    launches: &Arc<std::sync::Mutex<Vec<Launched>>>,
    during: impl FnOnce() + Send + 'static,
) -> impl FnOnce(crate::session::term::LaunchRequest) -> anyhow::Result<crate::session::term::Launch>
       + Send
       + 'static {
    launch_with(started, launches, idle_command, during)
}

/// [`stand_in_launch`] with a stand-in that runs `script`.
pub(crate) fn stand_in_launch_running(
    started: &Arc<std::sync::Mutex<Vec<StandIn>>>,
    launches: &Arc<std::sync::Mutex<Vec<Launched>>>,
    script: &str,
    during: impl FnOnce() + Send + 'static,
) -> impl FnOnce(crate::session::term::LaunchRequest) -> anyhow::Result<crate::session::term::Launch>
       + Send
       + 'static {
    let script = script.to_string();
    launch_with(started, launches, move || bash_command(&script), during)
}

/// [`stand_in_launch`] with a stand-in that says `ready` and then reads everything written to it,
/// which shows in its output as it is read: one process that starts nothing, so what the ledger
/// lists for its launch is only what a test started. On Unix it reads raw, as an agent drains a
/// prompt longer than the cooked terminal's echo buffer; on Windows it is a native process, since
/// under Git Bash a program a script runs, `exec`'d or not, is a process of its own, as is the
/// shell Git's `bin\bash.exe` launcher starts.
pub(crate) fn stand_in_launch_echoing(
    started: &Arc<std::sync::Mutex<Vec<StandIn>>>,
    launches: &Arc<std::sync::Mutex<Vec<Launched>>>,
    ready: &str,
    during: impl FnOnce() + Send + 'static,
) -> impl FnOnce(crate::session::term::LaunchRequest) -> anyhow::Result<crate::session::term::Launch>
       + Send
       + 'static {
    #[cfg(unix)]
    let command = {
        let script = format!("stty raw -echo; printf '{ready}\\n'; exec cat");
        move || bash_command(&script)
    };
    #[cfg(windows)]
    let command = {
        let program = super::fixture::Steps::new()
            .say(ready)
            .read_input()
            .program("echoing");
        move || program.term_command()
    };
    launch_with(started, launches, command, during)
}

/// The launch [`stand_in_launch`] and [`stand_in_launch_running`] make, of a stand-in running
/// `command`.
fn launch_with(
    started: &Arc<std::sync::Mutex<Vec<StandIn>>>,
    launches: &Arc<std::sync::Mutex<Vec<Launched>>>,
    command: impl FnOnce() -> crate::platform::term::Command + Send + 'static,
    during: impl FnOnce() + Send + 'static,
) -> impl FnOnce(crate::session::term::LaunchRequest) -> anyhow::Result<crate::session::term::Launch>
       + Send
       + 'static {
    let (started, launches) = (started.clone(), launches.clone());
    move |request| {
        during();
        let live = stand_in_of(
            &request.session_id,
            request.agent,
            80,
            24,
            command(),
            None,
            Some(&request.process_tag),
        );
        live.trust.end_watch();
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        let process = (*live).clone();
        started.lock().unwrap().push(live);
        launches
            .lock()
            .unwrap()
            .push((request.session_id, request.owner, request.task));
        Ok(crate::session::term::Launch {
            session: process,
            agent_session_id: None,
            notice: None,
        })
    }
}

/// [`fake_live_session`] with its trust watch still looking for the agent's screen, as a freshly
/// launched session's is, and, for a Grok one, where its trust record is carried after a press.
pub(crate) fn watched_live_session(
    id: &str,
    agent: AgentId,
    cols: u16,
    rows: u16,
    script: &str,
    carried_trust: Option<std::sync::Arc<dyn crate::session::trust::TrustCarrier>>,
) -> StandIn {
    stand_in(id, agent, cols, rows, script, carried_trust, None)
}

/// [`fake_live_session`] for the launch `tag` names: the stand-in carries the tag in its
/// environment, as an agent `term::launch` starts does, and has its launch id.
pub(crate) fn tagged_live_session(
    id: &str,
    agent: AgentId,
    script: &str,
    tag: &crate::processes::ledger::ProcessTag,
) -> StandIn {
    let live = stand_in(id, agent, 80, 24, script, None, Some(tag));
    live.trust.end_watch();
    live
}

/// [`tagged_live_session`] that stays idle, as [`idle_stand_in`] does.
pub(crate) fn tagged_idle_live_session(
    id: &str,
    agent: AgentId,
    tag: &crate::processes::ledger::ProcessTag,
) -> StandIn {
    let live = stand_in_of(id, agent, 80, 24, idle_command(), None, Some(tag));
    live.trust.end_watch();
    live
}

fn stand_in(
    id: &str,
    agent: AgentId,
    cols: u16,
    rows: u16,
    script: &str,
    carried_trust: Option<std::sync::Arc<dyn crate::session::trust::TrustCarrier>>,
    tag: Option<&crate::processes::ledger::ProcessTag>,
) -> StandIn {
    stand_in_of(
        id,
        agent,
        cols,
        rows,
        bash_command(script),
        carried_trust,
        tag,
    )
}

/// [`super::bash`] running `script`.
fn bash_command(script: &str) -> crate::platform::term::Command {
    // Bash rather than `/bin/sh`: callers' scripts may use bash builtins and syntax that dash, the
    // `/bin/sh` of Debian and Ubuntu, lacks.
    let mut cmd = crate::platform::term::Command::new(super::bash());
    cmd.args(["-c", script]);
    cmd
}

fn stand_in_of(
    id: &str,
    agent: AgentId,
    cols: u16,
    rows: u16,
    mut cmd: crate::platform::term::Command,
    carried_trust: Option<std::sync::Arc<dyn crate::session::trust::TrustCarrier>>,
    tag: Option<&crate::processes::ledger::ProcessTag>,
) -> StandIn {
    if let Some(tag) = tag {
        tag.containment().prepare(&mut cmd);
    }
    let spawned = crate::platform::term::spawn(
        cmd,
        crate::platform::term::Size { rows, cols },
        tag.map(|tag| tag.containment().as_ref()),
    )
    .expect("the stand-in");
    StandIn(Arc::new(LiveSession::new(NewSession {
        id: id.to_string(),
        agent,
        launch_id: tag.map_or_else(
            || uuid::Uuid::new_v4().to_string(),
            |tag| tag.launch_id().to_string(),
        ),
        spawned: spawned.into(),
        checks_composer: false,
        transport: None,
        scratch_dir: None,
        resolves_approvals_itself: false,
        carried_trust,
        before_scratch_removal: None,
        display: Default::default(),
    })))
}

/// The one executable every fake agent CLI is a symlink to, written once per test binary. A
/// script of its own per fake would pay macOS's first-`exec` code-signature check each time,
/// serialised across processes; this one is exec'd once and then reused. It sources the
/// companion file next to the symlink (`$0.body`), which is where each fake's behaviour lives.
/// Installed by writing a file of the process's own and renaming it into place, since a test
/// binary of another worktree may be executing it.
#[cfg(unix)]
fn fake_agent_dispatcher() -> &'static Path {
    static DISPATCHER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DISPATCHER.get_or_init(|| {
        let path = std::env::temp_dir().join("octoboardd-fake-agent-dispatcher");
        let staging = path.with_extension(format!("{}", std::process::id()));
        std::fs::write(&staging, "#!/bin/sh\n. \"$0.body\"\n")
            .expect("write fake agent dispatcher");
        #[cfg(unix)]
        {
            let mut perms = std::fs::metadata(&staging).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
            std::fs::set_permissions(&staging, perms).expect("chmod fake agent dispatcher");
        }
        std::fs::rename(&staging, &path).expect("install fake agent dispatcher");
        path
    })
}

/// Makes `dir/<binary>` a fake agent CLI that runs `body` as shell script (sourced, so `exit`
/// ends it and `$@` are its arguments), and returns an environment whose `PATH` is `dir` and the
/// system directories and whose `HOME` is `dir`.
#[cfg(unix)]
pub(crate) fn fake_agent(dir: &Path, binary: &str, body: &str) -> HashMap<String, String> {
    let path = dir.join(binary);
    std::fs::write(dir.join(format!("{binary}.body")), body).expect("write fake agent body");
    std::fs::remove_file(&path).ok();
    crate::test_support::symlink(fake_agent_dispatcher(), &path).expect("symlink fake agent");
    HashMap::from([
        (
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", dir.display()),
        ),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ])
}

/// On Windows the fake is `dir/<binary>.cmd`, which runs the body with Git Bash, and the `PATH`
/// is `dir` and the system directories.
#[cfg(windows)]
pub(crate) fn fake_agent(dir: &Path, binary: &str, body: &str) -> HashMap<String, String> {
    std::fs::write(dir.join(format!("{binary}.body")), body).expect("write fake agent body");
    std::fs::write(
        dir.join(format!("{binary}.cmd")),
        format!(
            "@\"{}\" \"%~dp0{binary}.body\" %*\r\n",
            crate::test_support::bash().display()
        ),
    )
    .expect("write fake agent");
    let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"));
    let path =
        std::env::join_paths([dir.to_path_buf(), system.join("System32"), system]).expect("a PATH");
    HashMap::from([
        ("PATH".to_string(), path.to_string_lossy().into_owned()),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ])
}
