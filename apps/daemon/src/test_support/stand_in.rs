//! Stand-in live sessions on a PTY, the fake agent CLIs they run, and the reads of what reached
//! them.

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use super::{is_running, perl_write, wait_for_pid, within, PATIENCE};
use crate::protocol::Agent;
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

/// A stand-in live session that stays alive on a PTY (`sleep 30`) with nothing reading it.
pub(crate) fn idle_stand_in(id: &str) -> StandIn {
    fake_live_session(id, Agent::Claude, 80, 24, "sleep 30")
}

/// A stand-in session for `id` that is registered with `state` and has its output read, so a test
/// can wait for what was written into it with [`output_containing`].
pub(crate) fn running(state: &Arc<AppState>, id: &str) -> StandIn {
    let live = idle_stand_in(id);
    state.register_live(live.clone());
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    live
}

/// A stand-in process for the stored session `id` whose shell command leaves one process running
/// in a terminal session of its own, as an agent's dev server is; registered and watched as a
/// launched session's is. Answers the stand-in and that process's pid.
pub(crate) fn running_leaving_one(state: &Arc<AppState>, dir: &Path, id: &str) -> (StandIn, i32) {
    let pid_file = dir.join(format!("{id}-child"));
    let live = fake_live_session(
        id,
        Agent::Claude,
        80,
        24,
        &format!(
            "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n\
             exec sleep 300",
            perl_write(&pid_file, "$$")
        ),
    );
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(state, live.clone());
    let child = wait_for_pid(&pid_file);
    (live, child)
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

/// Polls a stand-in session's output until it holds `needle`.
pub(crate) fn output_containing(live: &LiveSession, needle: &str) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let output = String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
        if output.contains(needle) || Instant::now() >= deadline {
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

    pub(crate) fn with_codex_transport(
        mut self,
        transport: crate::adapter::codex::transport::CodexTransport,
    ) -> Self {
        Arc::get_mut(&mut self.0)
            .expect("configure before registering the stand-in")
            .codex_transport = Some(Arc::new(transport));
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

pub(crate) fn fake_live_session(
    id: &str,
    agent: Agent,
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

/// What a stand-in launch was asked for: the session, its owner and its opening prompt.
pub(crate) type Launched = (String, Option<crate::mcp::Owner>, Option<String>);

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
    stand_in_launch_running(started, launches, "sleep 30", during)
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
    let (started, launches, script) = (started.clone(), launches.clone(), script.to_string());
    move |request| {
        during();
        let live = tagged_live_session(
            &request.session_id,
            request.agent,
            &script,
            &request.process_tag,
        );
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
    agent: Agent,
    cols: u16,
    rows: u16,
    script: &str,
    carried_trust: Option<crate::session::trust::CarriedTrust>,
) -> StandIn {
    stand_in(id, agent, cols, rows, script, carried_trust, None)
}

/// [`fake_live_session`] for the launch `tag` names: the stand-in carries the tag in its
/// environment, as an agent `term::launch` starts does, and has its launch id.
pub(crate) fn tagged_live_session(
    id: &str,
    agent: Agent,
    script: &str,
    tag: &crate::processes::ledger::ProcessTag,
) -> StandIn {
    let live = stand_in(id, agent, 80, 24, script, None, Some(tag));
    live.trust.end_watch();
    live
}

fn stand_in(
    id: &str,
    agent: Agent,
    cols: u16,
    rows: u16,
    script: &str,
    carried_trust: Option<crate::session::trust::CarriedTrust>,
    tag: Option<&crate::processes::ledger::ProcessTag>,
) -> StandIn {
    // Bash rather than `/bin/sh`: callers' scripts may use bash builtins and syntax that dash, the
    // `/bin/sh` of Debian and Ubuntu, lacks.
    let mut cmd = portable_pty::CommandBuilder::new("/bin/bash");
    cmd.args(["-c", script]);
    if let Some(tag) = tag {
        cmd.env(crate::processes::ledger::TAG_VARIABLE, tag.value());
    }
    let spawned = crate::terminal::pty::spawn(cmd, rows, cols).expect("the stand-in");
    StandIn(Arc::new(LiveSession::new(NewSession {
        id: id.to_string(),
        agent,
        launch_id: tag.map_or_else(
            || uuid::Uuid::new_v4().to_string(),
            |tag| tag.launch_id().to_string(),
        ),
        spawned,
        checks_composer: false,
        codex_transport: None,
        scratch_dir: None,
        resolves_approvals_itself: false,
        carried_trust,
        before_scratch_removal: None,
    })))
}

/// The one executable every fake agent CLI is a symlink to, written once per test binary. A
/// script of its own per fake would pay macOS's first-`exec` code-signature check each time,
/// serialised across processes; this one is exec'd once and then reused. It sources the
/// companion file next to the symlink (`$0.body`), which is where each fake's behaviour lives.
/// Installed by writing a file of the process's own and renaming it into place, since a test
/// binary of another worktree may be executing it.
fn fake_agent_dispatcher() -> &'static Path {
    static DISPATCHER: OnceLock<PathBuf> = OnceLock::new();
    DISPATCHER.get_or_init(|| {
        let path = std::env::temp_dir().join("octoboardd-fake-agent-dispatcher");
        let staging = path.with_extension(format!("{}", std::process::id()));
        std::fs::write(&staging, "#!/bin/sh\n. \"$0.body\"\n")
            .expect("write fake agent dispatcher");
        let mut perms = std::fs::metadata(&staging).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
        std::fs::set_permissions(&staging, perms).expect("chmod fake agent dispatcher");
        std::fs::rename(&staging, &path).expect("install fake agent dispatcher");
        path
    })
}

/// Makes `dir/<binary>` a fake agent CLI that runs `body` as shell script (sourced, so `exit`
/// ends it and `$@` are its arguments), and returns an environment whose `PATH` is `dir` and the
/// system directories and whose `HOME` is `dir`.
pub(crate) fn fake_agent(dir: &Path, binary: &str, body: &str) -> HashMap<String, String> {
    let path = dir.join(binary);
    std::fs::write(dir.join(format!("{binary}.body")), body).expect("write fake agent body");
    std::fs::remove_file(&path).ok();
    std::os::unix::fs::symlink(fake_agent_dispatcher(), &path).expect("symlink fake agent");
    HashMap::from([
        (
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", dir.display()),
        ),
        ("HOME".to_string(), dir.to_string_lossy().into_owned()),
    ])
}
