use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::broadcast;

use super::{close, close_all, close_all_ending, create, rename, shell_environment, ShellProcess};
use crate::protocol::{error_code, Event, Request, ShellTerminal, ShellTerminalState};
use crate::state::AppState;
use crate::terminal::display::{DisplayFacts, DisplaySeed};
use crate::terminal::pty::PtyCore;
use crate::test_support::code_of;
use crate::test_support::{
    app_state, assert_gone_by, console, git, insert_project, is_running, repo_with, typed, within,
    ScratchDir, PATIENCE,
};

/// Makes `state` run no `git`: no test here reads a worktree but the one that names one.
fn without_git(state: &AppState) {
    state.host_tools.replace(crate::host_tools::Tools {
        git: || anyhow::bail!("this test runs no git"),
        ..state.host_tools.get()
    });
}

/// A state holding project `p1` of console `console-1`, at a directory of its own.
fn with_project(label: &str) -> (Arc<AppState>, ScratchDir, ScratchDir) {
    let (state, dir) = app_state(label);
    without_git(&state);
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

/// A command that runs a program in the foreground for about `seconds`: `sleep`, or in PowerShell
/// `ping` (`Start-Sleep` is the shell's own and starts no process), started on the shell's console
/// and waited for, where `&` can give it a console of its own and not wait: Windows PowerShell
/// does when its console's window is hidden, as a pseudoconsole's is to it.
fn sleeping(seconds: u32) -> String {
    if cfg!(windows) {
        format!(
            "Start-Process -NoNewWindow -Wait -FilePath \
             \"$([Environment]::SystemDirectory)\\PING.EXE\" -ArgumentList '-n','{}','127.0.0.1'",
            seconds + 1
        )
    } else {
        format!("sleep {seconds}")
    }
}

/// The name a shell terminal shows before its shell sets a title: its shell's file name.
fn shell_name() -> String {
    if cfg!(windows) {
        let program = (crate::test_support::host_tools().shell)().program;
        program.file_name().unwrap().to_string_lossy().into_owned()
    } else {
        "bash".to_string()
    }
}

/// `path` as the daemon names a folder it resolved: the real path, in the plain form on Windows.
fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    crate::platform::names::canonical(path).unwrap()
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

    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let canonical = canonical(&project);
    assert_eq!(created.project_id, "p1");
    assert_eq!(created.worktree, None);
    assert_eq!(created.cwd, canonical.to_string_lossy());
    assert_eq!(
        created.title,
        shell_name(),
        "before the shell sets one, its name"
    );
    assert_eq!(created.state, ShellTerminalState::Running);
    assert!(matches!(
        shell_events(&mut events).first(),
        Some(Event::ShellTerminalUpserted { shell_terminal }) if shell_terminal.id == created.id
    ));
    let shell = process(&state, &created);
    let pid = shell.pid;
    assert!(crate::platform::proc::crash::is_registered(
        crate::platform::proc::ProcessId(pid)
    ));

    shell
        .write(&typed("pwd; echo $((6*7))", "(Get-Location).Path; 6*7"))
        .unwrap();
    let output = output_containing(&shell, "42").await;
    assert!(output.contains(&*canonical.to_string_lossy()), "{output}");

    shell.resize(100, 40).unwrap();
    shell
        .write(&typed(
            "stty size",
            "\"$($Host.UI.RawUI.WindowSize.Height) $($Host.UI.RawUI.WindowSize.Width)\"",
        ))
        .unwrap();
    output_containing(&shell, "40 100").await;

    // A client attaching again is given what scrolled past before it came.
    let replay = shell.attach(10_000).opening;
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
    assert!(!crate::platform::proc::crash::is_registered(
        crate::platform::proc::ProcessId(pid)
    ));
    let again = close(&state, &created.id).await.unwrap_err();
    assert_eq!(code_of(&again), error_code::UNKNOWN_SHELL_TERMINAL);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_display_a_client_reports_on_a_shell_becomes_the_daemons_fallback_for_every_terminal() {
    let (state, _dir, _project) = with_project("shell-display-fallback");
    let before = DisplaySeed::new(state.display_fallback.clone(), None).fallback();
    assert!(before.dark(), "no report yet: the dark theme");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);
    let attached = shell.attach(0);
    let client = attached.answering.as_ref().expect("a live client").client;
    let light = DisplayFacts::reported("#383a42", "#fafafa", "#808080", Some((9, 20))).unwrap();
    shell.report_display(client, light);

    let after = DisplaySeed::new(state.display_fallback.clone(), None).fallback();
    assert!(!after.dark(), "the reported colours are the daemon's last");
    assert_eq!(after.background, light.background);
    assert_eq!(after.cell, DisplayFacts::default().cell, "never the cell");
    close(&state, &created.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_title_follows_the_shell_until_the_terminal_is_renamed() {
    let (state, _dir, _project) = with_project("shell-title");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);

    shell
        .write(&typed(
            "printf '\\033]2;from the shell\\007'",
            "[Console]::Write([char]27 + ']2;from the shell' + [char]7)",
        ))
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
        .write(&typed(
            "printf '\\033]2;later\\007'; echo after-the-title",
            "[Console]::Write([char]27 + ']2;later' + [char]7); 'after-the-title'",
        ))
        .unwrap();
    output_containing(&shell, "after-the-title\r\n").await;
    let kept = record_where(&state, &created.id, |_| true).await;
    assert_eq!(kept.title, "Mine");

    let unknown = rename(&state, "no-such-terminal", "x".to_string()).unwrap_err();
    assert_eq!(code_of(&unknown), error_code::UNKNOWN_SHELL_TERMINAL);
    close(&state, &created.id).await.unwrap();
}

/// Read off the parsed screen's titles, as a host that renders what the program draws has them
/// read, the shell's own title still names the terminal.
#[tokio::test(flavor = "multi_thread")]
async fn the_title_follows_the_shell_off_the_parsed_screen() {
    let (state, _dir, _project) = with_project("shell-title-rendered");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);
    shell.read_as_rendered();

    shell
        .write(&typed(
            "printf '\\033]2;from the parsed screen\\007'",
            "[Console]::Write([char]27 + ']2;from the parsed screen' + [char]7)",
        ))
        .unwrap();
    record_where(&state, &created.id, |record| {
        record.title == "from the parsed screen"
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_is_busy_while_it_runs_something_in_the_foreground() {
    let (state, _dir, _project) = with_project("shell-busy");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);

    shell.write(&typed(&sleeping(30), &sleeping(30))).unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"\x03").unwrap();
    record_where(&state, &created.id, |record| !record.busy).await;

    close(&state, &created.id).await.unwrap();
}

// Unix only: where the terminal cannot name its foreground job, as on Windows, the shell is busy
// while its job lists any process besides the shell and the console hosts, wherever that process's
// streams go (`busy`), which `a_background_process_keeps_a_shell_busy_until_it_ends` tests.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_background_child_off_the_terminal_does_not_make_a_shell_busy() {
    let (state, _dir, _project) = with_project("shell-detached-child");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);

    // A helper the prompt keeps (a git status daemon) has its streams off the terminal, so the
    // shell stays idle with it running; a second job that keeps the terminal makes it busy, and
    // idle again once that one is gone while the helper still runs.
    shell
        .write(b"sleep 30 </dev/null >/dev/null 2>&1 &\r")
        .unwrap();
    shell.write(b"sleep 31 &\r").unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"kill %2\r").unwrap();
    record_where(&state, &created.id, |record| !record.busy).await;

    shell.write(b"kill %1\r").unwrap();
    close(&state, &created.id).await.unwrap();
}

/// Where the terminal cannot name its foreground job, a process the shell started in the
/// background keeps it busy until it ends, though its streams are off the terminal: the shell's job
/// lists it, beside the shell and the console host of its hidden console.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn a_background_process_keeps_a_shell_busy_until_it_ends() {
    let (state, _dir, _project) = with_project("shell-background");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);

    shell
        .write(
            b"$job = Start-Process -WindowStyle Hidden -PassThru -FilePath \
              \"$([Environment]::SystemDirectory)\\PING.EXE\" -ArgumentList '-n','31','127.0.0.1'\r",
        )
        .unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"Stop-Process -Id $job.Id\r").unwrap();
    record_where(&state, &created.id, |record| !record.busy).await;

    close(&state, &created.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_that_exits_leaves_its_terminal_exited_with_its_output_until_closed() {
    let (state, _dir, _project) = with_project("shell-exit");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);
    let attached = shell.attach(0);
    let client = attached.answering.as_ref().expect("a live client").client;
    let wake = attached.live.clone().expect("a live client");

    shell
        .write(&typed(
            "echo last-words; sleep 30 &",
            "'last-words'; Start-Process -NoNewWindow -FilePath \
             \"$([Environment]::SystemDirectory)\\PING.EXE\" -ArgumentList '-n','31','127.0.0.1'",
        ))
        .unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    shell.write(b"exit\r").unwrap();
    let exited = record_where(&state, &created.id, |record| {
        record.state == ShellTerminalState::Exited
    })
    .await;
    assert!(!exited.busy, "an exited shell runs nothing");

    // The client attached before the exit is let go once the output has reached it.
    let ended = tokio::time::timeout(PATIENCE, async {
        loop {
            match shell.take_output(client) {
                crate::terminal::fan::Delivery::Send(frames) => {
                    let sent: usize = frames.iter().map(|frame| frame.size()).sum();
                    shell.acknowledge(client, sent as u64);
                }
                crate::terminal::fan::Delivery::Wait => wake.notified().await,
                crate::terminal::fan::Delivery::Over => break,
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the attached client was never let go");

    let crate::terminal::fan::Attached {
        opening: paint,
        live,
        ..
    } = shell.attach(0);
    assert!(paint.starts_with(b"\x1bc"), "an exited shell is painted");
    assert!(String::from_utf8_lossy(&paint).contains("last-words"));
    assert!(live.is_none(), "an exited shell's paint is all there is");

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

    let removed = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let kept = create(&state, "p2".to_string(), None, String::new())
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
    assert_eq!(left, std::slice::from_ref(&kept.id));
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
        let created = create(&state, "p1".to_string(), None, String::new())
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

/// The stop that escalates a planned one forgets every terminal, asks about each whether it is
/// to be ended here, and ends only those it picks: the others end with their holder.
#[tokio::test(flavor = "multi_thread")]
async fn an_escalated_stop_forgets_every_terminal_and_ends_only_those_it_picks() {
    let (state, _dir, _project) = with_project("shell-exit-picked");
    let ended = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let left = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let (ended_pid, left_pid) = (process(&state, &ended).pid, process(&state, &left).pid);
    let asked = std::sync::Mutex::new(Vec::new());
    close_all_ending(&state, |shell| {
        asked.lock().unwrap().push(shell.id.clone());
        shell.id != left.id
    })
    .await;
    assert!(state.shell_terminals.list().is_empty());
    let mut asked = asked.into_inner().unwrap();
    asked.sort();
    let mut both = vec![ended.id.clone(), left.id.clone()];
    both.sort();
    assert_eq!(asked, both);
    assert_gone_by(ended_pid as i32, Instant::now() + PATIENCE);
    crate::test_support::kill(left_pid as i32);
}

#[test]
fn a_shell_starts_in_the_snapshot_or_else_in_the_daemons_environment_without_agent_markers() {
    let inherited = || {
        [
            ("PATH", "/inherited/bin"),
            ("CLAUDECODE", "1"),
            ("OCTOBOARD_SESSION_ID", "s1"),
            ("OCTOBOARD_DAEMON_PORT", "7"),
            ("OCTOBOARD_DAEMON_KEY", "k"),
            ("OCTOBOARD_DAEMON_KEY_FILE", "/run/s1/key"),
            ("OCTOBOARD_DAEMON_CREDENTIAL_PATH", "/run/s1/key"),
            ("OCTOBOARD_PROCESS_TAG", "instance:launch"),
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
            ("TERM".to_string(), crate::terminal::pty::TERM.to_string()),
        ])
    );

    let inherited_only = shell_environment(None, inherited());
    assert_eq!(
        inherited_only,
        HashMap::from([
            ("PATH".to_string(), "/inherited/bin".to_string()),
            ("TERM".to_string(), crate::terminal::pty::TERM.to_string()),
        ])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_starts_when_its_environment_cannot_be_read() {
    let (state, _dir, _project) = with_project("shell-no-env");
    state.host_tools.replace(crate::host_tools::Tools {
        shell_env: || anyhow::bail!("the login shell timed out"),
        ..state.host_tools.get()
    });
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);
    shell.write(&typed("echo $((6*7))", "6*7")).unwrap();
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
    let roots =
        crate::browse::scope::project_live_roots(&state, "p1", &|| state.host_tools.isolated_git())
            .unwrap();
    let worktree = roots
        .iter()
        .find_map(|root| root.worktree.clone())
        .expect("the linked worktree is listed");

    let created = create(
        &state,
        "p1".to_string(),
        Some(worktree.clone()),
        String::new(),
    )
    .await
    .unwrap();
    assert_eq!(created.worktree, Some(worktree));
    assert_eq!(created.cwd, canonical(&linked).to_string_lossy());
    close(&state, &created.id).await.unwrap();

    let foreign = create(
        &state,
        "p1".to_string(),
        Some("not-a-worktree".to_string()),
        String::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&foreign), error_code::WORKTREE_UNAVAILABLE);
    let unknown = create(&state, "p9".to_string(), None, String::new())
        .await
        .unwrap_err();
    assert_eq!(code_of(&unknown), error_code::UNKNOWN_PROJECT);
    assert!(state.shell_terminals.list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_starts_in_a_folder_below_the_project_and_refuses_what_is_not_one() {
    let (state, _dir, project) = with_project("shell-path");
    std::fs::create_dir_all(project.join("src/a b")).unwrap();
    std::fs::write(project.join("file.txt"), "a").unwrap();

    let created = create(&state, "p1".to_string(), None, "src/a b".to_string())
        .await
        .unwrap();
    assert_eq!(
        created.cwd,
        canonical(&project.join("src/a b")).to_string_lossy()
    );
    close(&state, &created.id).await.unwrap();

    for (path, code) in [
        ("file.txt", error_code::PATH_NOT_A_DIRECTORY),
        ("missing", error_code::FILE_NOT_FOUND),
    ] {
        let refused = create(&state, "p1".to_string(), None, path.to_string())
            .await
            .unwrap_err();
        assert_eq!(code_of(&refused), code, "{path}");
    }
    assert!(state.shell_terminals.list().is_empty());

    crate::test_support::symlink(project.join("src/a b"), project.join("link")).unwrap();
    let linked = create(&state, "p1".to_string(), None, "link".to_string())
        .await
        .unwrap();
    assert_eq!(
        linked.cwd,
        canonical(&project.join("src/a b")).to_string_lossy(),
        "a link is followed to its folder"
    );
    close(&state, &linked.id).await.unwrap();
}

/// A folder whose name is not valid Unicode is named on the wire by its name's bytes (WTF-8 on
/// Windows, the raw bytes on Linux), and the terminal starts in exactly that folder. macOS stores
/// only UTF-8 names, so there is no such folder to start in there.
#[cfg(any(windows, target_os = "linux"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_starts_in_a_folder_whose_name_is_not_valid_unicode() {
    let (state, _dir, project) = with_project("shell-not-unicode");
    #[cfg(windows)]
    let (name, wire) = {
        use std::os::windows::ffi::OsStringExt;
        let mut units: Vec<u16> = "surrogate-".encode_utf16().collect();
        units.push(0xD800);
        (std::ffi::OsString::from_wide(&units), "surrogate-%ED%A0%80")
    };
    #[cfg(target_os = "linux")]
    let (name, wire) = {
        use std::os::unix::ffi::OsStringExt;
        (std::ffi::OsString::from_vec(b"caf\xff".to_vec()), "caf%FF")
    };
    let folder = project.join(&name);
    std::fs::create_dir(&folder).unwrap();

    let created = create(&state, "p1".to_string(), None, wire.to_string())
        .await
        .unwrap();
    assert_eq!(created.cwd, canonical(&folder).to_string_lossy());
    close(&state, &created.id).await.unwrap();
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

#[tokio::test(flavor = "multi_thread")]
async fn closing_a_terminal_lets_it_go_whatever_still_holds_it_open() {
    let (state, _dir, _project) = with_project("shell-hang-up");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);

    // A job the shell leaves behind, as `huponexit` being off lets it, holding the PTY's other side
    // open: the reader would wait on it for as long as the job runs. On Windows the console is
    // closed once the shell has exited, which ends what is on it; there the job holds on through
    // the close, as long as the host lets it, and the console's output stays open meanwhile.
    #[cfg(unix)]
    let job = b"shopt -u huponexit; sleep 300 & echo job=$!\r".to_vec();
    #[cfg(windows)]
    let job = format!("{}\r", job_holding_the_console()).into_bytes();
    shell.write(&job).unwrap();
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
    crate::test_support::kill(job);
    assert!(
        gone,
        "the closed terminal is still held, its reader still reading"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_is_busy_while_another_group_holds_its_terminals_foreground() {
    let (state, _dir, _project) = with_project("shell-foreground");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);

    // A process that is not the shell's child (its parent exits at once, leaving the shell with
    // none) takes the terminal's foreground for a group of its own, as a job control program run
    // from elsewhere could, while the shell waits on a FIFO rather than on the terminal: a shell
    // that reads its terminal from the background is told it has gone, and exits. The holder waits
    // past a few polls first, so a poll sees the shell idle in between. Windows has no foreground
    // group: there the holder is on the shell's console, started by a Windows PowerShell that
    // exits at once, while the shell is back at its prompt.
    shell
        .write(&typed(
            "mkfifo hold.fifo; perl -e 'exit if fork; setpgrp(0, 0); $SIG{TTOU} = q(IGNORE); \
              select(undef, undef, undef, 1.5); require POSIX; \
              POSIX::tcsetpgrp(0, getpgrp()) or die; print qq(holder=$$\\n); sleep 30'; \
              read line < hold.fifo",
            &holder_started_elsewhere(),
        ))
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
    crate::test_support::kill(holder);
    assert!(busy, "the shell read as idle with its foreground held");
    close(&state, &created.id).await.unwrap();
}

/// What PowerShell is typed to start a job on its console that holds on when the console closes
/// (`test_support::fixture`), and print `job=` and its pid.
#[cfg(windows)]
fn job_holding_the_console() -> String {
    let (program, args) = crate::test_support::fixture::Steps::new()
        .hold_on_close()
        .sleep(300)
        .program("job")
        .command_line_args();
    format!(
        "$job = Start-Process -NoNewWindow -PassThru -FilePath '{}' -ArgumentList '{}'; \
         \"job=$($job.Id)\"",
        program.replace('\'', "''"),
        args.replace('\'', "''")
    )
}

/// What PowerShell is typed for a process on its console that is not its child to print
/// `holder=` and its pid: a Windows PowerShell that starts it and exits at once.
fn holder_started_elsewhere() -> String {
    let starter = "$holder = Start-Process -NoNewWindow -PassThru -FilePath \
        \"$([Environment]::SystemDirectory)\\PING.EXE\" -ArgumentList '-n','31','127.0.0.1'; \
        [Console]::Out.Write(\"holder=$($holder.Id)`n\")";
    format!(
        "Start-Process -NoNewWindow -FilePath \
         \"$([Environment]::SystemDirectory)\\WindowsPowerShell\\v1.0\\powershell.exe\" \
         -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{}'",
        crate::test_support::programs::encoded_powershell(starter)
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_only_close_checks_the_process_even_when_the_record_says_idle() {
    let (state, _dir, _project) = with_project("shell-close-idle-busy");
    let created = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &created);
    shell.write(&typed(&sleeping(30), &sleeping(30))).unwrap();
    record_where(&state, &created.id, |record| record.busy).await;
    super::update(&state, &created.id, |record| record.busy = false);
    let body = serde_json::json!({
        "type": "close_shell_terminal", "shell_terminal": created.id, "only_if_idle": true,
    });
    request(&state, &body.to_string()).await.unwrap();
    assert!(state.shell_terminals.process(&created.id).is_some());
    close(&state, &created.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_only_close_removes_idle_and_exited_terminals_and_is_idempotent() {
    let (state, _dir, _project) = with_project("shell-close-idle");
    let idle = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    let shell = process(&state, &idle);
    shell.write(&typed("printf 'ready\\n'", "'ready'")).unwrap();
    output_containing(&shell, "ready\r\n").await;
    record_where(&state, &idle.id, |record| !record.busy).await;
    super::close_if_idle(&state, &idle.id).await;
    assert!(state.shell_terminals.process(&idle.id).is_none());
    super::close_if_idle(&state, &idle.id).await;

    let exited = create(&state, "p1".to_string(), None, String::new())
        .await
        .unwrap();
    process(&state, &exited).write(b"exit\r").unwrap();
    record_where(&state, &exited.id, |record| {
        record.state == ShellTerminalState::Exited
    })
    .await;
    super::close_if_idle(&state, &exited.id).await;
    assert!(state.shell_terminals.process(&exited.id).is_none());
    request(
        &state,
        r#"{"type":"close_shell_terminal","shell_terminal":"already-gone","only_if_idle":true}"#,
    )
    .await
    .unwrap();
}
