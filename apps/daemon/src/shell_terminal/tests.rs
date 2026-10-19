use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::broadcast;

use super::{close, close_all, create, rename, shell_environment, ShellProcess};
use crate::browse::git::GitEnv;
use crate::protocol::{error_code, CodedError, Event, Request, ShellTerminal, ShellTerminalState};
use crate::pty::PtyCore;
use crate::state::AppState;
use crate::test_support::{
    app_state, assert_gone_by, console, git, insert_project, isolated_git_env, repo_with, within,
    ScratchDir, PATIENCE, SYSTEM_GIT,
};

/// No test here reads a worktree but the one that names one, which passes a `git` of its own.
fn no_git() -> anyhow::Result<GitEnv> {
    anyhow::bail!("this test runs no git")
}

fn test_git() -> anyhow::Result<GitEnv> {
    Ok(GitEnv::new(SYSTEM_GIT.to_string(), isolated_git_env()))
}

/// A state holding project `p1` of console `console-1`, at a directory of its own.
fn with_project(label: &str) -> (Arc<AppState>, ScratchDir, ScratchDir) {
    let (state, dir) = app_state(label);
    state.store.insert_console(&console(&dir)).unwrap();
    let project = ScratchDir::new(&format!("{label}-project"));
    insert_project(&state, &project);
    (state, dir, project)
}

/// Adds project `id` at `path` to console `console_id`.
fn add_project(state: &AppState, id: &str, console_id: &str, path: &std::path::Path) {
    let project = serde_json::from_value(serde_json::json!({
        "id": id, "console_id": console_id, "host_id": crate::store::LOCAL_HOST_ID,
        "name": id, "path": path, "source": "local",
        "trust_consent": false, "pinned": false, "tags": [],
    }))
    .unwrap();
    state.store.insert_project(&project).unwrap();
}

async fn request(state: &Arc<AppState>, json: &str) -> anyhow::Result<Option<Event>> {
    let request: Request = serde_json::from_str(json).expect(json);
    crate::coordinator::handle(state, request.id, request.body).await
}

fn code_of(err: &anyhow::Error) -> &str {
    err.downcast_ref::<CodedError>()
        .map(|coded| coded.code)
        .unwrap_or("uncoded")
}

fn process(state: &AppState, terminal: &ShellTerminal) -> Arc<ShellProcess> {
    state
        .shell_terminals
        .process(&terminal.id)
        .expect("the terminal is held")
}

/// Waits for the terminal's output to hold `needle`, and returns it as it then stands.
async fn output_containing(process: &Arc<ShellProcess>, needle: &str) -> String {
    let (process, needle) = (process.clone(), needle.to_string());
    let found = {
        let (process, needle) = (process.clone(), needle.clone());
        within(PATIENCE, move || screen(&process).contains(&needle)).await
    };
    let output = screen(&process);
    assert!(found, "{needle:?} never appeared in {output:?}");
    output
}

fn screen(pty: &PtyCore) -> String {
    String::from_utf8_lossy(&pty.recent_output(64 * 1024)).into_owned()
}

/// Waits for terminal `id`'s record to satisfy `holds`, and returns it.
async fn record_where(
    state: &Arc<AppState>,
    id: &str,
    holds: impl Fn(&ShellTerminal) -> bool + Send + 'static,
) -> ShellTerminal {
    let current = {
        let (state, id) = (state.clone(), id.to_string());
        move || {
            state
                .shell_terminals
                .list()
                .into_iter()
                .find(|record| record.id == id)
        }
    };
    let met = {
        let current = current.clone();
        within(PATIENCE, move || {
            current().is_some_and(|record| holds(&record))
        })
        .await
    };
    let record = current();
    assert!(met, "the record never got there: {record:?}");
    record.unwrap()
}

/// The shell-terminal events broadcast so far, in order.
fn shell_events(events: &mut broadcast::Receiver<Event>) -> Vec<Event> {
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        if matches!(
            event,
            Event::ShellTerminalUpserted { .. } | Event::ShellTerminalDeleted { .. }
        ) {
            seen.push(event);
        }
    }
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_runs_a_shell_in_its_project_that_takes_input_and_ends_when_closed() {
    let (state, _dir, project) = with_project("shell-lifecycle");
    let mut events = state.subscribe();

    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let canonical = project.canonicalize().unwrap();
    assert_eq!(created.project_id, "p1");
    assert_eq!(created.worktree, None);
    assert_eq!(created.cwd, canonical.to_string_lossy());
    assert_eq!(created.title, "bash", "before the shell sets one, its name");
    assert_eq!(created.state, ShellTerminalState::Running);
    assert!(matches!(
        shell_events(&mut events).first(),
        Some(Event::ShellTerminalUpserted { shell_terminal }) if shell_terminal.id == created.id
    ));
    let shell = process(&state, &created);
    let pid = shell.pid;
    assert!(crate::crash_cleanup::is_registered(pid));

    shell.write(b"pwd; echo $((6*7))\r").unwrap();
    let output = output_containing(&shell, "42").await;
    assert!(output.contains(&*canonical.to_string_lossy()), "{output}");

    shell.resize(100, 40).unwrap();
    shell.write(b"stty size\r").unwrap();
    output_containing(&shell, "40 100").await;

    // A client attaching again is given what scrolled past before it came.
    let (replay, _live) = shell.attach();
    let replay = String::from_utf8_lossy(&replay);
    assert!(
        replay.contains("42") && replay.contains("40 100"),
        "{replay}"
    );

    close(&state, &created.id).await.unwrap();
    assert!(state.shell_terminals.list().is_empty());
    assert!(shell_events(&mut events).iter().any(
        |event| matches!(event, Event::ShellTerminalDeleted { shell_terminal } if *shell_terminal == created.id)
    ));
    assert_gone_by(pid as i32, Instant::now() + PATIENCE);
    assert!(!crate::crash_cleanup::is_registered(pid));
    let again = close(&state, &created.id).await.unwrap_err();
    assert_eq!(code_of(&again), error_code::UNKNOWN_SHELL_TERMINAL);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_title_follows_the_shell_until_the_terminal_is_renamed() {
    let (state, _dir, _project) = with_project("shell-title");
    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let shell = process(&state, &created);

    shell
        .write(b"printf '\\033]2;from the shell\\007'\r")
        .unwrap();
    record_where(&state, &created.id, |record| {
        record.title == "from the shell"
    })
    .await;

    let blank = request(
        &state,
        &format!(
            r#"{{"type":"rename_shell_terminal","shell_terminal":"{}","title":"  "}}"#,
            created.id
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&blank), error_code::FIELD_REQUIRED);
    request(
        &state,
        &format!(
            r#"{{"type":"rename_shell_terminal","shell_terminal":"{}","title":" Mine "}}"#,
            created.id
        ),
    )
    .await
    .unwrap();
    let renamed = record_where(&state, &created.id, |record| record.renamed).await;
    assert_eq!(renamed.title, "Mine");

    shell
        .write(b"printf '\\033]2;later\\007'; echo after-the-title\r")
        .unwrap();
    output_containing(&shell, "after-the-title\r\n").await;
    let kept = record_where(&state, &created.id, |_| true).await;
    assert_eq!(kept.title, "Mine");

    let unknown = rename(&state, "no-such-terminal", "x".to_string()).unwrap_err();
    assert_eq!(code_of(&unknown), error_code::UNKNOWN_SHELL_TERMINAL);
    close(&state, &created.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_is_busy_while_it_runs_something_in_the_foreground_or_the_background() {
    let (state, _dir, _project) = with_project("shell-busy");
    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let shell = process(&state, &created);

    shell.write(b"sleep 30\r").unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"\x03").unwrap();
    record_where(&state, &created.id, |record| !record.busy).await;

    shell.write(b"sleep 30 &\r").unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"kill %1\r").unwrap();
    record_where(&state, &created.id, |record| !record.busy).await;

    close(&state, &created.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_that_exits_leaves_its_terminal_exited_with_its_output_until_closed() {
    let (state, _dir, _project) = with_project("shell-exit");
    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let shell = process(&state, &created);
    let (_, mut attached) = shell.attach();

    shell.write(b"echo last-words; sleep 30 &\r").unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"exit\r").unwrap();
    let exited = record_where(&state, &created.id, |record| {
        record.state == ShellTerminalState::Exited
    })
    .await;
    assert!(!exited.busy, "an exited shell runs nothing");

    // The client attached before the exit is let go once the output has reached it.
    let ended =
        tokio::time::timeout(PATIENCE, async { while attached.recv().await.is_some() {} }).await;
    assert!(ended.is_ok(), "the attached client was never let go");

    let (replay, live) = shell.attach();
    assert!(String::from_utf8_lossy(&replay).contains("last-words"));
    assert!(live.is_closed(), "an exited shell's replay is all there is");

    close(&state, &created.id).await.unwrap();
    assert!(state.shell_terminals.list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_project_or_its_console_ends_its_terminals_and_no_other() {
    let (state, dir, _project) = with_project("shell-removal");
    let mut second = console(&dir);
    second.id = "console-2".to_string();
    second.workdir = dir.join("workdir-2").to_string_lossy().into_owned();
    state.store.insert_console(&second).unwrap();
    let other = ScratchDir::new("shell-removal-other");
    add_project(&state, "p2", "console-2", &other);

    let removed = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let kept = create(&state, "p2".to_string(), None, no_git)
        .await
        .unwrap();
    let (removed_pid, kept_pid) = (process(&state, &removed).pid, process(&state, &kept).pid);

    request(&state, r#"{"type":"delete_project","project":"p1"}"#)
        .await
        .unwrap();
    let left: Vec<String> = state
        .shell_terminals
        .list()
        .into_iter()
        .map(|record| record.id)
        .collect();
    assert_eq!(left, [kept.id.clone()]);
    assert_gone_by(removed_pid as i32, Instant::now() + PATIENCE);

    request(&state, r#"{"type":"delete_console","console":"console-2"}"#)
        .await
        .unwrap();
    assert!(state.shell_terminals.list().is_empty());
    assert_gone_by(kept_pid as i32, Instant::now() + PATIENCE);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemons_exit_ends_every_terminal() {
    let (state, _dir, _project) = with_project("shell-exit-all");
    let mut pids = Vec::new();
    for _ in 0..2 {
        let created = create(&state, "p1".to_string(), None, no_git)
            .await
            .unwrap();
        pids.push(process(&state, &created).pid);
    }
    close_all(&state).await;
    assert!(state.shell_terminals.list().is_empty());
    for pid in pids {
        assert_gone_by(pid as i32, Instant::now() + PATIENCE);
    }
}

#[test]
fn a_shell_starts_in_the_snapshot_or_else_in_the_daemons_environment_without_agent_markers() {
    let inherited = || {
        [
            ("PATH", "/inherited/bin"),
            ("CLAUDECODE", "1"),
            ("OCTOBOARD_SESSION_ID", "s1"),
            ("TERM", "dumb"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
    };
    let snapshot = HashMap::from([("PATH".to_string(), "/snapshot/bin".to_string())]);

    let from_snapshot = shell_environment(Some(snapshot), inherited());
    assert_eq!(
        from_snapshot,
        HashMap::from([
            ("PATH".to_string(), "/snapshot/bin".to_string()),
            ("TERM".to_string(), crate::pty::TERM.to_string()),
        ])
    );

    let inherited_only = shell_environment(None, inherited());
    assert_eq!(
        inherited_only,
        HashMap::from([
            ("PATH".to_string(), "/inherited/bin".to_string()),
            ("TERM".to_string(), crate::pty::TERM.to_string()),
        ])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_starts_when_its_environment_cannot_be_read() {
    let (state, _dir, _project) = with_project("shell-no-env");
    state.host_tools.replace(crate::host_tools::Tools {
        shell_env: || anyhow::bail!("the login shell timed out"),
        ..crate::test_support::host_tools()
    });
    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let shell = process(&state, &created);
    shell.write(b"echo $((6*7))\r").unwrap();
    output_containing(&shell, "42").await;
    close(&state, &created.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_starts_only_in_a_worktree_of_its_own_project() {
    let (state, dir) = app_state("shell-worktree");
    state.store.insert_console(&console(&dir)).unwrap();
    let repo = repo_with("shell-worktree-repo", &[(b"a.txt", b"a")]);
    let linked = ScratchDir::new("shell-worktree-linked");
    std::fs::remove_dir(&*linked).unwrap();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            &linked.to_string_lossy(),
        ],
    );
    insert_project(&state, &repo);
    let roots = crate::browse::project_live_roots(&state, "p1", &test_git).unwrap();
    let worktree = roots
        .iter()
        .find_map(|root| root.worktree.clone())
        .expect("the linked worktree is listed");

    let created = create(&state, "p1".to_string(), Some(worktree.clone()), test_git)
        .await
        .unwrap();
    assert_eq!(created.worktree, Some(worktree));
    assert_eq!(
        created.cwd,
        linked.canonicalize().unwrap().to_string_lossy()
    );
    close(&state, &created.id).await.unwrap();

    let foreign = create(
        &state,
        "p1".to_string(),
        Some("not-a-worktree".to_string()),
        test_git,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&foreign), error_code::WORKTREE_UNAVAILABLE);
    let unknown = create(&state, "p9".to_string(), None, test_git)
        .await
        .unwrap_err();
    assert_eq!(code_of(&unknown), error_code::UNKNOWN_PROJECT);
    assert!(state.shell_terminals.list().is_empty());
}

/// Waits for the shell to print a pid after `marker`, the typed command's own echo aside, and
/// returns it.
async fn printed_pid(shell: &Arc<ShellProcess>, marker: &'static str) -> i32 {
    let pid_in = move |output: &str| {
        output
            .split(marker)
            .map(|rest| {
                rest.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
            })
            .find_map(|pid| pid.parse::<i32>().ok())
    };
    let printed = {
        let shell = shell.clone();
        within(PATIENCE, move || pid_in(&screen(&shell)).is_some()).await
    };
    let output = screen(shell);
    assert!(printed, "no pid after {marker} in {output:?}");
    pid_in(&output).unwrap()
}

/// Whether `pid` is a running process rather than gone or a zombie.
fn is_running(pid: i32) -> bool {
    let ps = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("ps ran");
    let stat = String::from_utf8_lossy(&ps.stdout);
    !stat.trim().is_empty() && !stat.trim_start().starts_with('Z')
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_a_terminal_lets_it_go_whatever_still_holds_it_open() {
    let (state, _dir, _project) = with_project("shell-hang-up");
    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let shell = process(&state, &created);

    // A job the shell leaves behind, as `huponexit` being off lets it, holding the PTY's other side
    // open: the reader would wait on it for as long as the job runs.
    shell
        .write(b"shopt -u huponexit; sleep 300 & echo job=$!\r")
        .unwrap();
    let job = printed_pid(&shell, "job=").await;
    shell.write(b"exit\r").unwrap();
    record_where(&state, &created.id, |record| {
        record.state == ShellTerminalState::Exited
    })
    .await;
    assert!(is_running(job), "the job outlived its shell");

    // Nothing of the terminal is left once it is closed: its PTY is closed, and its reader, its
    // output and the rest go with the last hold on it. On macOS the shell's exit already revoked
    // the terminal, ending the reader; on Linux it does not, and only the hang-up does.
    close(&state, &created.id).await.unwrap();
    assert!(
        shell.is_hung_up(),
        "the closed terminal's PTY is still open"
    );
    assert!(shell.write(b"x").is_err());
    let released = Arc::downgrade(&shell);
    drop(shell);
    let gone = within(PATIENCE, move || released.upgrade().is_none()).await;
    // SAFETY: the test's own job; a test cleaning up after itself.
    unsafe { libc::kill(job, libc::SIGKILL) };
    assert!(
        gone,
        "the closed terminal is still held, its reader still reading"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_is_busy_while_another_group_holds_its_terminals_foreground() {
    let (state, _dir, _project) = with_project("shell-foreground");
    let created = create(&state, "p1".to_string(), None, no_git)
        .await
        .unwrap();
    let shell = process(&state, &created);

    // A process that is not the shell's child (its parent exits at once, leaving the shell with
    // none) takes the terminal's foreground for a group of its own, as a job control program run
    // from elsewhere could, while the shell waits on a FIFO rather than on the terminal: a shell
    // that reads its terminal from the background is told it has gone, and exits. The holder waits
    // past a few polls first, so a poll sees the shell idle in between.
    shell
        .write(
            b"mkfifo hold.fifo; perl -e 'exit if fork; setpgrp(0, 0); $SIG{TTOU} = q(IGNORE); \
              select(undef, undef, undef, 1.5); require POSIX; \
              POSIX::tcsetpgrp(0, getpgrp()) or die; print qq(holder=$$\\n); sleep 30'; \
              read line < hold.fifo\r",
        )
        .unwrap();
    let holder = printed_pid(&shell, "holder=").await;
    let busy = {
        let state = state.clone();
        let id = created.id.clone();
        within(PATIENCE, move || {
            state
                .shell_terminals
                .list()
                .iter()
                .any(|record| record.id == id && record.busy)
        })
        .await
    };
    // SAFETY: the test's own process, whose pid it printed.
    unsafe { libc::kill(holder, libc::SIGKILL) };
    assert!(busy, "the shell read as idle with its foreground held");
    close(&state, &created.id).await.unwrap();
}
