//! The holder run in this process against a registry directory of its own, driven over its daemon
//! link by a plain WebSocket client, with real programs on real terminals.

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as Wire;

use super::*;
use crate::holder::protocol::{decode_bulk, BulkKind, ContainmentSpec, RelayRoute};
use crate::holder::registry;
use crate::platform::term::Command;
use crate::test_support::{is_running, programs, ScratchDir, PATIENCE};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const SIZE: host_term::Size = host_term::Size { rows: 24, cols: 80 };

/// What the holder sent.
#[derive(Debug)]
enum Frame {
    Reply(FromHolder),
    Bulk(BulkKind, String, Vec<u8>),
}

/// A daemon's end of a link.
struct Daemon {
    socket: Socket,
    welcome: FromHolder,
    /// The terminals this link had started, for [`Self::end_terminals`].
    started: Vec<String>,
}

/// The terminals the welcome of `daemon` lists.
fn listed(daemon: &Daemon) -> &[crate::holder::protocol::TerminalInfo] {
    match &daemon.welcome {
        FromHolder::Welcome { terminals, .. } => terminals,
        other => panic!("{other:?}"),
    }
}

impl Daemon {
    async fn send(&mut self, message: ToHolder) {
        let text = serde_json::to_string(&message).unwrap();
        self.socket.send(Wire::Text(text)).await.expect("sent");
    }

    /// The next frame, or `None` once the link is closed.
    async fn next(&mut self) -> Option<Frame> {
        loop {
            match tokio::time::timeout(PATIENCE, self.socket.next())
                .await
                .expect("the holder went silent")?
            {
                Ok(Wire::Text(text)) => {
                    return Some(Frame::Reply(serde_json::from_str(&text).expect("a reply")));
                }
                Ok(Wire::Binary(bytes)) => {
                    let (kind, id, payload) = decode_bulk(&bytes).expect("a bulk frame");
                    return Some(Frame::Bulk(kind, id.to_string(), payload.to_vec()));
                }
                Ok(Wire::Close(_)) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }

    /// Frames up to and including the first that `done` accepts.
    async fn until(&mut self, mut done: impl FnMut(&Frame) -> bool) -> Vec<Frame> {
        let mut seen = Vec::new();
        while let Some(frame) = self.next().await {
            let finished = done(&frame);
            seen.push(frame);
            if finished {
                return seen;
            }
        }
        panic!("the link closed before the frame expected: {seen:?}");
    }

    /// The reply to the request `req`, whatever else arrives meanwhile.
    async fn reply_to(&mut self, req: u64) -> FromHolder {
        let frames = self
            .until(|frame| match frame {
                Frame::Reply(
                    FromHolder::Written { req: got, .. }
                    | FromHolder::Done { req: got, .. }
                    | FromHolder::Answer { req: got, .. },
                ) => *got == req,
                _ => false,
            })
            .await;
        match frames.into_iter().last() {
            Some(Frame::Reply(reply)) => reply,
            other => panic!("{other:?}"),
        }
    }

    /// The key the holder issues for the launch of terminal `id`.
    async fn issue_launch(&mut self, id: &str, session: &str) -> String {
        self.send(ToHolder::IssueLaunch {
            id: id.to_string(),
            req: 77,
            session: session.to_string(),
            mcp_token: None,
        })
        .await;
        let frames = self
            .until(|frame| {
                matches!(
                    frame,
                    Frame::Reply(FromHolder::LaunchIssued { req: 77, .. })
                )
            })
            .await;
        match frames.into_iter().last() {
            Some(Frame::Reply(FromHolder::LaunchIssued { key: Some(key), .. })) => key,
            other => panic!("{other:?}"),
        }
    }

    /// The next relayed request, whatever else arrives meanwhile.
    async fn next_relay(&mut self) -> FromHolder {
        let frames = self
            .until(|frame| matches!(frame, Frame::Reply(FromHolder::Relay { .. })))
            .await;
        match frames.into_iter().last() {
            Some(Frame::Reply(relay)) => relay,
            other => panic!("{other:?}"),
        }
    }

    async fn spawn(&mut self, id: &str, command: Command) -> FromHolder {
        self.spawn_spec(spec(id, command)).await
    }

    async fn spawn_spec(&mut self, spec: SpawnSpec) -> FromHolder {
        let id = spec.id.clone();
        self.send(ToHolder::Spawn(Box::new(spec))).await;
        let frames = self
            .until(|frame| {
                matches!(frame, Frame::Reply(
                    FromHolder::Spawned { id: got, .. } | FromHolder::SpawnFailed { id: got, .. }
                ) if *got == id)
            })
            .await;
        match frames.into_iter().last() {
            Some(Frame::Reply(reply)) => {
                if matches!(reply, FromHolder::Spawned { .. }) {
                    self.started.push(id);
                }
                reply
            }
            other => panic!("{other:?}"),
        }
    }

    /// Ends the terminals this link started, as the daemon's ordinary stop does, so that the
    /// holder is left with nothing to run and exits once the link is gone.
    async fn end_terminals(&mut self) {
        let ids = std::mem::take(&mut self.started);
        self.end_ids(&ids.iter().map(String::as_str).collect::<Vec<_>>())
            .await;
    }

    /// Kills, reaps and releases each of the terminals `ids`.
    async fn end_ids(&mut self, ids: &[&str]) {
        for (index, id) in ids.iter().enumerate() {
            let req = 9000 + index as u64;
            self.send(ToHolder::Signal {
                id: id.to_string(),
                signal: crate::holder::protocol::SignalKind::Kill,
            })
            .await;
            tokio::time::timeout(PATIENCE, async {
                loop {
                    self.send(ToHolder::Reap {
                        id: id.to_string(),
                        req,
                    })
                    .await;
                    match self.reply_to(req).await {
                        FromHolder::Answer {
                            answer: Answer::Reaped { reaped: false, .. },
                            ..
                        } => tokio::time::sleep(Duration::from_millis(20)).await,
                        _ => break,
                    }
                }
            })
            .await
            .expect("the terminal was not reaped");
            self.send(ToHolder::Release { id: id.to_string() }).await;
        }
    }

    /// Everything the terminal `id` printed, until its exit is reported.
    async fn output_until_exit(&mut self, id: &str) -> (String, Vec<Frame>) {
        let frames = self
            .until(|frame| matches!(frame, Frame::Reply(FromHolder::Exited { id: got, .. }) if got == id))
            .await;
        (output_of(&frames, id), frames)
    }

    /// Output of the terminal `id` until it has printed `needle`.
    async fn output_until(&mut self, id: &str, needle: &str) -> String {
        let mut printed = String::new();
        let mut frames = Vec::new();
        while !printed.contains(needle) {
            frames.extend(self.until(|_| true).await);
            printed = output_of(&frames, id);
        }
        printed
    }
}

fn output_of(frames: &[Frame], id: &str) -> String {
    let bytes: Vec<u8> = frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Bulk(BulkKind::Output, got, payload) if got == id => Some(payload.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn spec(id: &str, command: Command) -> SpawnSpec {
    SpawnSpec {
        id: id.to_string(),
        command,
        size: SIZE,
        containment: None,
        kill_session_leftovers: false,
        meta: serde_json::json!({ "kind": "test", "id": id }),
        companion: false,
    }
}

/// A holder running in this process, and the directory it registers itself in.
struct Running {
    dir: ScratchDir,
    holder_id: String,
    task: JoinHandle<Result<()>>,
}

impl Running {
    async fn start() -> Self {
        let dir = ScratchDir::new("holder-server");
        let holder_id = uuid::Uuid::new_v4().to_string();
        let task = tokio::spawn(run(HolderConfig {
            registry_dir: dir.join("holders"),
            holder_id: holder_id.clone(),
            version: "test".to_string(),
            build_id: "test-build".to_string(),
            exe: std::env::current_exe().unwrap(),
            first_link_deadline: PATIENCE,
            relay_deadline: PATIENCE,
        }));
        let running = Self {
            dir,
            holder_id,
            task,
        };
        running.entry().await;
        running
    }

    async fn entry(&self) -> RegistryEntry {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(entry) = registry::read(&self.dir.join("holders"), &self.holder_id) {
                return entry;
            }
            assert!(Instant::now() < deadline, "the holder never registered");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn link(&self) -> Daemon {
        self.link_as(None).await
    }

    /// Reads fresh welcomes until the holder has recorded `done`; process exits reach its
    /// snapshot asynchronously, so a welcome taken just after a kill can still list them alive.
    async fn link_when(&self, mut done: impl FnMut(&Daemon) -> bool) -> Daemon {
        let mut last = None;
        let result = tokio::time::timeout(PATIENCE, async {
            loop {
                let daemon = self.link().await;
                if done(&daemon) {
                    return daemon;
                }
                last = Some(listed(&daemon).to_vec());
                drop(daemon);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        result.unwrap_or_else(|_| {
            panic!("the holder never listed the expected state; last listed: {last:?}")
        })
    }

    /// As [`Self::link`], or `None` when the holder closes the link before its welcome.
    #[cfg(unix)]
    async fn try_link(&self) -> Option<Daemon> {
        self.try_link_as(None).await
    }

    /// Links as the daemon process `run_id`; a holder that refuses it answers the hello with
    /// [`FromHolder::Superseded`] where the welcome would be.
    async fn link_as(&self, run_id: Option<&str>) -> Daemon {
        self.try_link_as(run_id)
            .await
            .expect("closed before the welcome")
    }

    /// As [`Self::link_as`], or `None` when the holder closes the link before its welcome.
    async fn try_link_as(&self, run_id: Option<&str>) -> Option<Daemon> {
        let entry = self.entry().await;
        let url = format!(
            "ws://127.0.0.1:{}/daemon?secret={}",
            entry.port, entry.secret
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("connected");
        let hello = ToHolder::Hello {
            version: "test".to_string(),
            build_id: "test-build".to_string(),
            run_id: run_id.map(str::to_string),
            handed_over: Vec::new(),
        };
        socket
            .send(Wire::Text(serde_json::to_string(&hello).unwrap()))
            .await
            .unwrap();
        let welcome = loop {
            match tokio::time::timeout(PATIENCE, socket.next()).await.unwrap() {
                Some(Ok(Wire::Text(text))) => break serde_json::from_str(&text).unwrap(),
                Some(Ok(Wire::Close(_)) | Err(_)) | None => return None,
                Some(Ok(_)) => {}
            }
        };
        Some(Daemon {
            socket,
            welcome,
            started: Vec::new(),
        })
    }

    /// Waits for the holder to have ended and removed its registry file.
    async fn ended(self) {
        tokio::time::timeout(PATIENCE, self.task)
            .await
            .expect("the holder did not exit")
            .expect("the holder panicked")
            .expect("the holder failed");
        assert!(registry::read(&self.dir.join("holders"), &self.holder_id).is_none());
    }
}

fn helper(role: &str) -> Command {
    let mut command = Command::new(crate::platform::test_helper::program());
    command
        .args(crate::platform::test_helper::args(
            "platform::term::contract_tests::a_terminal_helper",
        ))
        .env(crate::platform::test_helper::ROLE, role);
    command
}

fn pid_of(reply: &FromHolder) -> u32 {
    match reply {
        FromHolder::Spawned { pid, .. } => *pid,
        other => panic!("{other:?}"),
    }
}

/// A daemon that was taken over from, and so superseded, cannot take the holder back: its hello is
/// refused, and the newer daemon's link is left as it is.
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_the_holder_was_taken_from_is_refused_when_it_returns() {
    let holder = Running::start().await;
    let mut old = holder.link_as(Some("old-run")).await;
    assert!(matches!(old.welcome, FromHolder::Welcome { .. }));
    let mut new = holder.link_as(Some("new-run")).await;
    assert!(matches!(new.welcome, FromHolder::Welcome { .. }));
    // The link taken over is told which run took it.
    let mut told = None;
    while let Some(frame) = old.next().await {
        if let Frame::Reply(FromHolder::Superseded { by }) = frame {
            told = Some(by);
        }
    }
    assert_eq!(told, Some(Some("new-run".to_string())));

    let back = holder.link_as(Some("old-run")).await;
    assert!(matches!(
        back.welcome,
        FromHolder::Superseded { by: Some(ref by) } if by == "new-run"
    ));
    // A hello from an older build without a run id is accepted, and takes the holder over.
    let unmarked = holder.link().await;
    assert!(matches!(unmarked.welcome, FromHolder::Welcome { .. }));
    while new.next().await.is_some() {}
    drop(unmarked);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_welcome_names_the_holder_and_lists_no_terminals_before_any_is_started() {
    let holder = Running::start().await;
    let daemon = holder.link().await;
    let FromHolder::Welcome {
        holder_id,
        version,
        build_id,
        last_parting,
        terminals,
        facts,
        answers_offline,
        ..
    } = &daemon.welcome
    else {
        panic!("{:?}", daemon.welcome);
    };
    assert_eq!(holder_id, &holder.holder_id);
    // The queries it answers while no daemon is linked: none where the host answers them.
    let expected = if host_term::facts().host_answers_queries {
        Vec::new()
    } else {
        vec![OfflineAnswer::PrimaryDeviceAttributes]
    };
    assert_eq!(*answers_offline, expected);
    assert_eq!(
        (version.as_str(), build_id.as_str()),
        ("test", "test-build")
    );
    assert_eq!(*last_parting, LastParting::None);
    assert!(terminals.is_empty());
    assert_eq!(*facts, host_term::facts());
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_without_the_secret_is_refused() {
    let holder = Running::start().await;
    let entry = holder.entry().await;
    let wrong = format!("ws://127.0.0.1:{}/daemon?secret=nope", entry.port);
    assert!(tokio_tungstenite::connect_async(wrong).await.is_err());
    let none = format!("ws://127.0.0.1:{}/daemon", entry.port);
    assert!(tokio_tungstenite::connect_async(none).await.is_err());
    drop(holder.link().await);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_prints_and_its_exit_is_reported_only_after_its_output() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let reply = daemon
        .spawn("t1", programs::echo("hello from the holder").term_command())
        .await;
    assert!(pid_of(&reply) > 0);
    let (printed, frames) = daemon.output_until_exit("t1").await;
    assert!(printed.contains("hello from the holder"), "{printed:?}");
    // Nothing the program printed comes after the exit report that ended the wait.
    assert!(matches!(
        frames.last(),
        Some(Frame::Reply(FromHolder::Exited { .. }))
    ));
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_spawn_of_a_known_id_is_refused_and_not_run() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let first = daemon.spawn("t1", programs::sleep(60).term_command()).await;
    let pid = pid_of(&first) as i32;
    let second = daemon.spawn("t1", programs::sleep(60).term_command()).await;
    assert!(
        matches!(second, FromHolder::SpawnFailed { .. }),
        "{second:?}"
    );
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
    assert!(!is_running(pid));
}

#[tokio::test(flavor = "multi_thread")]
async fn input_reaches_the_program_and_a_resize_reaches_its_terminal() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon.spawn("t1", helper("size")).await;
    daemon.output_until("t1", "helper-ready").await;

    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 1,
            bytes: b"\n".to_vec(),
        })
        .await;
    assert!(matches!(
        daemon.reply_to(1).await,
        FromHolder::Written {
            written: 1,
            error: None,
            ..
        }
    ));
    daemon.output_until("t1", "size:24x80").await;

    daemon
        .send(ToHolder::Resize {
            id: "t1".into(),
            req: 2,
            size: host_term::Size { rows: 10, cols: 40 },
        })
        .await;
    assert!(matches!(
        daemon.reply_to(2).await,
        FromHolder::Done { error: None, .. }
    ));
    daemon
        .send(ToHolder::Query {
            id: "t1".into(),
            req: 3,
            what: QueryKind::Size,
        })
        .await;
    match daemon.reply_to(3).await {
        FromHolder::Answer {
            answer: Answer::Size { size: Some(size) },
            ..
        } => assert_eq!(size, host_term::Size { rows: 10, cols: 40 }),
        other => panic!("{other:?}"),
    }
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 4,
            bytes: b"\n".to_vec(),
        })
        .await;
    daemon.reply_to(4).await;
    daemon.output_until("t1", "size:10x40").await;
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn semantic_input_is_encoded_by_the_holder() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon.spawn("t1", helper("echo")).await;
    daemon.output_until("t1", "helper-ready").await;
    daemon
        .send(ToHolder::WriteInput {
            id: "t1".into(),
            req: 1,
            inputs: vec![
                crate::platform::term::Input::Text("typed".into()),
                crate::platform::term::Input::Enter,
            ],
        })
        .await;
    assert!(matches!(
        daemon.reply_to(1).await,
        FromHolder::Written { error: None, .. }
    ));
    daemon.output_until("t1", "got:\"typed\"").await;
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

/// A program that never reads its input takes only what the terminal buffers, and the write that
/// does not fit says how far it got.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_write_to_a_program_that_reads_nothing_reports_how_far_it_got() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    // In raw mode without echo, so that nothing the program is sent is dropped or echoed away.
    let mut deaf = Command::new("/bin/sh");
    deaf.args(["-c", "stty raw -echo; echo ready; exec sleep 60"]);
    daemon.spawn("t1", deaf).await;
    daemon.output_until("t1", "ready").await;
    let bytes = vec![b'x'; 4 * 1024 * 1024];
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 1,
            bytes: bytes.clone(),
        })
        .await;
    match daemon.reply_to(1).await {
        FromHolder::Written { written, error, .. } => {
            assert!(error.is_some(), "the whole write went in");
            assert!(written < bytes.len());
        }
        other => panic!("{other:?}"),
    }
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exited_program_is_reaped_once_and_forgotten_on_release() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon
        .spawn("t1", programs::echo("bye").term_command())
        .await;
    daemon.output_until_exit("t1").await;

    daemon
        .send(ToHolder::Query {
            id: "t1".into(),
            req: 1,
            what: QueryKind::ExitedUnreaped,
        })
        .await;
    assert!(matches!(
        daemon.reply_to(1).await,
        FromHolder::Answer {
            answer: Answer::Flag { value: Some(true) },
            ..
        }
    ));
    daemon
        .send(ToHolder::Reap {
            id: "t1".into(),
            req: 2,
        })
        .await;
    assert!(matches!(
        daemon.reply_to(2).await,
        FromHolder::Answer {
            answer: Answer::Reaped {
                reaped: true,
                error: None
            },
            ..
        }
    ));
    daemon.send(ToHolder::Release { id: "t1".into() }).await;
    daemon
        .send(ToHolder::Query {
            id: "t1".into(),
            req: 3,
            what: QueryKind::Size,
        })
        .await;
    assert!(matches!(
        daemon.reply_to(3).await,
        FromHolder::Answer {
            answer: Answer::UnknownTerminal,
            ..
        }
    ));
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_signal_ends_the_program_and_the_holder_keeps_the_terminal_until_it_is_reaped() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let pid = pid_of(&daemon.spawn("t1", programs::sleep(60).term_command()).await) as i32;
    assert!(is_running(pid));
    daemon
        .send(ToHolder::Signal {
            id: "t1".into(),
            signal: crate::holder::protocol::SignalKind::Kill,
        })
        .await;
    daemon.output_until_exit("t1").await;
    daemon
        .send(ToHolder::Reap {
            id: "t1".into(),
            req: 1,
        })
        .await;
    assert!(matches!(
        daemon.reply_to(1).await,
        FromHolder::Answer {
            answer: Answer::Reaped { reaped: true, .. },
            ..
        }
    ));
    assert!(!is_running(pid));
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_newer_link_takes_the_holder_over_and_is_handed_each_terminals_tail() {
    let holder = Running::start().await;
    let mut first = holder.link().await;
    let pid = pid_of(
        &first
            .spawn("t1", {
                let mut command = helper("echo");
                command.env("UNUSED", "1");
                command
            })
            .await,
    ) as i32;
    first.output_until("t1", "helper-ready").await;

    let mut second = holder.link().await;
    let FromHolder::Welcome {
        terminals,
        last_parting,
        ..
    } = &second.welcome
    else {
        panic!("{:?}", second.welcome);
    };
    assert_eq!(terminals.len(), 1);
    assert_eq!(
        (terminals[0].id.as_str(), terminals[0].pid as i32),
        ("t1", pid)
    );
    assert_eq!(terminals[0].meta["kind"], "test");
    assert!(!terminals[0].exited);
    // The first link was taken over, which is not a crash.
    assert_eq!(*last_parting, LastParting::None);

    // The older link is told another daemon took the holder over, and closed.
    let mut told = false;
    while let Some(frame) = first.next().await {
        told |= matches!(frame, Frame::Reply(FromHolder::Superseded { .. }));
    }
    assert!(told, "the older link was not told it was taken over");

    let Some(Frame::Bulk(BulkKind::Tail, id, tail)) = second.next().await else {
        panic!("no tail after the welcome");
    };
    assert_eq!(id, "t1");
    assert!(String::from_utf8_lossy(&tail).contains("helper-ready"));

    // The program runs on, and its output goes to the newer link.
    assert!(is_running(pid));
    second
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 1,
            bytes: b"after\n".to_vec(),
        })
        .await;
    second.output_until("t1", "got:\"after\"").await;
    second.end_ids(&["t1"]).await;
    drop(second);
    holder.ended().await;
    assert!(!is_running(pid));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_holder_keeps_its_programs_when_the_daemon_link_drops_and_hands_them_to_the_next() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let pid = pid_of(&daemon.spawn("t1", helper("echo")).await) as i32;
    daemon.output_until("t1", "helper-ready").await;
    drop(daemon);

    // Nothing ends it: the program runs on and the holder stays.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(is_running(pid));
    assert!(!holder.task.is_finished());

    let mut next = holder.link().await;
    let FromHolder::Welcome {
        terminals,
        last_parting,
        ..
    } = &next.welcome
    else {
        panic!("{:?}", next.welcome);
    };
    // The link dropped without the daemon having parted.
    assert_eq!(*last_parting, LastParting::Crash);
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].pid as i32, pid);
    let Some(Frame::Bulk(BulkKind::Tail, id, tail)) = next.next().await else {
        panic!("no tail after the welcome");
    };
    assert_eq!(id, "t1");
    assert!(String::from_utf8_lossy(&tail).contains("helper-ready"));
    next.end_ids(&["t1"]).await;
    drop(next);
    holder.ended().await;
    assert!(!is_running(pid));
}

#[tokio::test(flavor = "multi_thread")]
async fn output_printed_while_no_daemon_is_linked_is_kept_in_the_tail() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon.spawn("t1", helper("echo")).await;
    daemon.output_until("t1", "helper-ready").await;
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 1,
            bytes: b"unattended\n".to_vec(),
        })
        .await;
    daemon.output_until("t1", "got:\"unattended\"").await;
    drop(daemon);

    let mut next = holder.link().await;
    let Some(Frame::Bulk(BulkKind::Tail, _, tail)) = next.next().await else {
        panic!("no tail after the welcome");
    };
    assert!(String::from_utf8_lossy(&tail).contains("got:\"unattended\""));
    next.end_ids(&["t1"]).await;
    drop(next);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_holder_with_no_terminal_exits_when_its_daemon_is_gone_but_not_while_one_runs() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon.spawn("t1", programs::sleep(60).term_command()).await;
    daemon.end_terminals().await;
    // A linked daemon that has not asked to drain keeps the holder, whatever it runs.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!holder.task.is_finished());
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_draining_holder_exits_with_its_last_terminal_while_the_daemon_stays_linked() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let pid = pid_of(&daemon.spawn("t1", programs::sleep(60).term_command()).await) as i32;
    daemon.send(ToHolder::Drain).await;
    let refused = daemon.spawn("t2", programs::sleep(60).term_command()).await;
    assert!(
        matches!(refused, FromHolder::SpawnFailed { .. }),
        "{refused:?}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!holder.task.is_finished(), "a terminal is still running");
    daemon.end_ids(&["t1"]).await;
    holder.ended().await;
    assert!(!is_running(pid));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_planned_parting_is_what_the_next_welcome_reports_instead_of_a_crash() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon.spawn("t1", programs::sleep(60).term_command()).await;
    daemon.send(ToHolder::Parting { handover: true }).await;
    daemon.socket.close(None).await.ok();
    drop(daemon);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!holder.task.is_finished(), "the program is still running");

    let mut next = holder.link().await;
    assert!(matches!(
        &next.welcome,
        FromHolder::Welcome {
            last_parting: LastParting::Handover,
            ..
        }
    ));
    next.send(ToHolder::Parting { handover: false }).await;
    next.end_ids(&["t1"]).await;
    next.socket.close(None).await.ok();
    drop(next);
    holder.ended().await;
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_parting_ends_a_holder_with_an_unreleased_exited_terminal() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon
        .spawn("t1", programs::echo("last screen").term_command())
        .await;
    daemon.output_until_exit("t1").await;
    // The process is gone, but its final reap and release never reached the holder.
    daemon.send(ToHolder::Parting { handover: false }).await;
    daemon.socket.close(None).await.ok();
    drop(daemon);
    holder.ended().await;
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn a_planned_parting_keeps_an_unreleased_exited_terminal_for_the_next_daemon() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    daemon
        .spawn("t1", programs::echo("kept screen").term_command())
        .await;
    daemon.output_until_exit("t1").await;
    daemon.send(ToHolder::Parting { handover: true }).await;
    daemon.socket.close(None).await.ok();
    while daemon.next().await.is_some() {}
    drop(daemon);

    let mut next = holder.link().await;
    assert!(listed(&next)
        .iter()
        .any(|terminal| terminal.id == "t1" && terminal.exited));
    assert!(matches!(
        &next.welcome,
        FromHolder::Welcome {
            last_parting: LastParting::Handover,
            ..
        }
    ));
    next.end_ids(&["t1"]).await;
    drop(next);
    holder.ended().await;
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn an_older_links_ordinary_parting_does_not_end_the_newer_daemons_holder() {
    let holder = Running::start().await;
    let mut older = holder.link_as(Some("older")).await;
    let command = crate::test_support::fixture::Steps::new()
        .read_input()
        .program("held-terminal")
        .term_command();
    let pid = pid_of(&older.spawn("t1", command).await) as i32;
    let mut guard = crate::test_support::KillOnDrop(vec![pid]);
    older.send(ToHolder::Parting { handover: false }).await;
    // A reply behind the parting proves the holder read it before the takeover. The old link
    // has not closed yet, so its final frames remain valid until the newer generation is in place.
    older
        .send(ToHolder::Query {
            id: "t1".into(),
            req: 1,
            what: QueryKind::Size,
        })
        .await;
    assert!(matches!(
        older.reply_to(1).await,
        FromHolder::Answer {
            answer: Answer::Size { size: Some(_) },
            ..
        }
    ));
    let mut newer = holder.link_as(Some("newer")).await;
    while older.next().await.is_some() {}
    drop(older);
    assert!(listed(&newer)
        .iter()
        .any(|terminal| terminal.id == "t1" && !terminal.exited));
    newer
        .send(ToHolder::Query {
            id: "t1".into(),
            req: 2,
            what: QueryKind::Size,
        })
        .await;
    assert!(matches!(
        newer.reply_to(2).await,
        FromHolder::Answer {
            answer: Answer::Size { size: Some(_) },
            ..
        }
    ));
    assert!(is_running(pid), "the newer daemon's program was ended");
    newer.end_ids(&["t1"]).await;
    drop(newer);
    holder.ended().await;
    assert!(!is_running(pid));
    guard.0.clear();
}

/// A handover marks each terminal the link follows behind the output it was sent and sends it no
/// more output: the next welcome says where the mark falls in the tail with the token, and the
/// tail keeps the history before the mark and holds what was printed after it from there on, so
/// that each byte reaches one daemon.
#[tokio::test(flavor = "multi_thread")]
async fn a_handover_marks_each_followed_terminal_after_its_output_and_keeps_its_history() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    assert!(matches!(
        daemon.welcome,
        FromHolder::Welcome { handover: true, .. }
    ));
    // Spawning discards the frames it waits through, so t1's first output is read before t2
    // starts.
    daemon.spawn("t1", helper("echo")).await;
    daemon.output_until("t1", "helper-ready").await;
    daemon.spawn("t2", helper("echo")).await;
    daemon
        .send(ToHolder::Write {
            id: "t2".into(),
            req: 5,
            bytes: b"two\n".to_vec(),
        })
        .await;
    daemon.output_until("t2", "got:\"two\"").await;
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 1,
            bytes: b"before\n".to_vec(),
        })
        .await;
    daemon.output_until("t1", "got:\"before\"").await;

    // Printed while the handover is under way: on one side of the mark or the other.
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 2,
            bytes: b"pending\n".to_vec(),
        })
        .await;
    daemon
        .send(ToHolder::Handover {
            req: 3,
            token: "tok".into(),
        })
        .await;
    let frames = daemon
        .until(|frame| matches!(frame, Frame::Reply(FromHolder::Done { req: 3, .. })))
        .await;
    for id in ["t1", "t2"] {
        let mark = frames
            .iter()
            .position(|frame| {
                matches!(frame, Frame::Reply(FromHolder::HandoverMark { id: got }) if got == id)
            })
            .unwrap_or_else(|| panic!("{id} was not marked: {frames:?}"));
        assert!(
            output_of(&frames[mark..], id).is_empty(),
            "output of {id} came after its mark"
        );
    }
    let sent_before_the_mark = output_of(&frames, "t1");

    // The link is sent no more of its output.
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 4,
            bytes: b"after\n".to_vec(),
        })
        .await;
    let mut later = daemon
        .until(|frame| matches!(frame, Frame::Reply(FromHolder::Written { req: 4, .. })))
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(Some(frame)) =
        tokio::time::timeout(Duration::from_millis(200), daemon.next()).await
    {
        later.push(frame);
    }
    assert!(output_of(&later, "t1").is_empty(), "{later:?}");
    daemon.send(ToHolder::Parting { handover: true }).await;
    daemon.socket.close(None).await.ok();
    drop(daemon);

    let mut next = holder.link().await;
    let FromHolder::Welcome {
        terminals,
        last_parting,
        ..
    } = &next.welcome
    else {
        panic!("{:?}", next.welcome);
    };
    assert_eq!(*last_parting, LastParting::Handover);
    let told = |id: &str| {
        terminals
            .iter()
            .find(|info| info.id == id)
            .and_then(|info| info.handover.clone())
            .unwrap_or_else(|| panic!("the tail of {id} was not marked"))
    };
    let (first, second) = (told("t1"), told("t2"));
    assert_eq!((first.token.as_str(), first.truncated), ("tok", false));
    assert_eq!(first.ages.first().map(|age| age.offset), Some(0));
    assert_eq!((second.token.as_str(), second.truncated), ("tok", false));
    let mut tails = HashMap::new();
    while tails.len() < 2 {
        match next.next().await {
            Some(Frame::Bulk(BulkKind::Tail, id, tail)) => {
                tails.insert(id, tail);
            }
            Some(_) => {}
            None => panic!("the link closed before the tails"),
        }
    }
    let split = |id: &str, since: u64| {
        let tail = &tails[id];
        let (history, since_mark) = tail.split_at(since as usize);
        (
            String::from_utf8_lossy(history).into_owned(),
            String::from_utf8_lossy(since_mark).into_owned(),
        )
    };
    let (history, since_mark) = split("t1", first.since);
    assert!(
        history.contains("helper-ready") && history.contains("got:\"before\""),
        "{history:?}"
    );
    assert!(since_mark.contains("got:\"after\""), "{since_mark:?}");
    assert!(
        !since_mark.contains("helper-ready") && !since_mark.contains("got:\"before\""),
        "{since_mark:?}"
    );
    let pending = format!("{sent_before_the_mark}{since_mark}")
        .matches("got:\"pending\"")
        .count();
    assert_eq!(pending, 1, "{sent_before_the_mark:?} then {since_mark:?}");
    // Nothing was printed after t2's mark: it holds only history.
    let (history, since_mark) = split("t2", second.since);
    assert!(history.contains("got:\"two\""), "{history:?}");
    assert!(since_mark.is_empty(), "{since_mark:?}");
    next.end_ids(&["t1", "t2"]).await;
    drop(next);
    holder.ended().await;
}

/// Output printed after a handover that overflows the tail is told as cut: the tail no longer
/// holds all of it.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_tail_that_overflows_after_a_handover_is_told_as_truncated() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let mut flood = Command::new("/bin/sh");
    flood.args([
        "-c",
        concat!(
            "echo ready; read go; head -c 1200000 /dev/zero | tr '\\000' x; ",
            "echo; echo flooded; exec sleep 60"
        ),
    ]);
    daemon.spawn("t1", flood).await;
    daemon.output_until("t1", "ready").await;
    daemon
        .send(ToHolder::Handover {
            req: 1,
            token: "tok".into(),
        })
        .await;
    daemon
        .until(|frame| matches!(frame, Frame::Reply(FromHolder::Done { req: 1, .. })))
        .await;
    daemon
        .send(ToHolder::Write {
            id: "t1".into(),
            req: 2,
            bytes: b"go\n".to_vec(),
        })
        .await;
    daemon.reply_to(2).await;
    daemon.send(ToHolder::Parting { handover: true }).await;
    daemon.socket.close(None).await.ok();
    drop(daemon);

    // The flood goes to the tail alone; it is looked at once all of it is in. A link made while it
    // is still printing is told a tail that holds all of it (its front is held for the
    // handshake), and is ended and tried again.
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(mut next) = holder.try_link().await {
            let FromHolder::Welcome { terminals, .. } = &next.welcome else {
                panic!("{:?}", next.welcome);
            };
            let told = terminals[0].handover.clone().expect("the tail was marked");
            if told.truncated {
                if let Some(Frame::Bulk(BulkKind::Tail, _, tail)) = next.next().await {
                    if String::from_utf8_lossy(&tail).contains("flooded") {
                        assert_eq!(told.token, "tok");
                        assert_eq!(tail.len(), super::terminal::TAIL_BYTES);
                        next.end_ids(&["t1"]).await;
                        drop(next);
                        break;
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "the flood never reached the tail"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_is_started_in_the_containment_it_names() {
    let holder = Running::start().await;
    let mut daemon = holder.link().await;
    let mut request = spec("t1", programs::sleep(60).term_command());
    request.containment = Some(ContainmentSpec {
        instance: "holder-test".to_string(),
        launch: uuid::Uuid::new_v4().to_string(),
    });
    let reply = daemon.spawn_spec(request).await;
    assert!(pid_of(&reply) > 0);
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_holder_no_daemon_links_to_gives_up() {
    let dir = ScratchDir::new("holder-first-link");
    let task = tokio::spawn(run(HolderConfig {
        registry_dir: dir.join("holders"),
        holder_id: "lonely".to_string(),
        version: "test".to_string(),
        build_id: "test-build".to_string(),
        exe: std::env::current_exe().unwrap(),
        first_link_deadline: Duration::from_millis(200),
        relay_deadline: PATIENCE,
    }));
    tokio::time::timeout(PATIENCE, task)
        .await
        .expect("the holder kept waiting")
        .unwrap()
        .unwrap();
    assert!(registry::read(&dir.join("holders"), "lonely").is_none());
}

async fn post(
    port: u16,
    key: String,
    path: &'static str,
    body: &'static [u8],
) -> std::io::Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || crate::loopback::post(port, &key, path, body, PATIENCE))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helper_of_an_issued_launch_reaches_the_daemon_through_the_holder() {
    let holder = Running::start().await;
    let port = holder.entry().await.port;
    let mut daemon = holder.link().await;
    let key = daemon.issue_launch("t1", "s1").await;

    // A report is answered at once and relayed.
    assert!(post(port, key.clone(), "/hook/s1", b"{\"a\":1}")
        .await
        .unwrap()
        .is_empty());
    let FromHolder::Relay {
        route,
        param,
        body,
        waits,
        ..
    } = daemon.next_relay().await
    else {
        panic!("not a relay");
    };
    assert_eq!(
        (route, param.as_str(), body.as_slice(), waits),
        (RelayRoute::Hook, "s1", &b"{\"a\":1}"[..], false)
    );

    // A question is answered with what the daemon replies.
    let asking = tokio::spawn(post(port, key.clone(), "/revert-gate/s1", b"{}"));
    let FromHolder::Relay {
        req, route, waits, ..
    } = daemon.next_relay().await
    else {
        panic!("not a relay");
    };
    assert_eq!((route, waits), (RelayRoute::RevertGate, true));
    daemon
        .send(ToHolder::RelayReply {
            req,
            status: 200,
            body: "null".to_string(),
        })
        .await;
    assert_eq!(asking.await.unwrap().unwrap(), b"null");

    // A revoked key is refused; so is a released terminal's.
    let revoked = daemon.issue_launch("t2", "s2").await;
    daemon
        .send(ToHolder::RevokeLaunch { id: "t2".into() })
        .await;
    let released = daemon.issue_launch("t3", "s3").await;
    daemon.spawn("t3", programs::sleep(60).term_command()).await;
    assert!(post(port, released.clone(), "/hook/s3", b"{}")
        .await
        .is_ok());
    daemon.send(ToHolder::Release { id: "t3".into() }).await;
    // The requests above are handled in order on the link, so by now both are in effect.
    for (key, path) in [(revoked, "/hook/s2"), (released, "/hook/s3")] {
        let err = post(port, key, path, b"{}").await.expect_err("refused");
        assert_eq!(crate::loopback::error_status(&err), Some(401), "{path}");
    }
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_key_of_a_launch_whose_start_failed_is_revoked() {
    let holder = Running::start().await;
    let port = holder.entry().await.port;
    let mut daemon = holder.link().await;
    let key = daemon.issue_launch("t1", "s1").await;
    let reply = daemon
        .spawn("t1", Command::new("/nonexistent/program/for/the/holder"))
        .await;
    assert!(matches!(reply, FromHolder::SpawnFailed { .. }), "{reply:?}");
    let err = post(port, key, "/hook/s1", b"{}")
        .await
        .expect_err("refused");
    assert_eq!(crate::loopback::error_status(&err), Some(401));
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_key_of_a_launch_a_draining_holder_refuses_is_revoked() {
    let holder = Running::start().await;
    let port = holder.entry().await.port;
    let mut daemon = holder.link().await;
    daemon.spawn("t0", programs::sleep(60).term_command()).await;
    daemon.send(ToHolder::Drain).await;
    let key = daemon.issue_launch("t1", "s1").await;
    let reply = daemon.spawn("t1", programs::sleep(60).term_command()).await;
    assert!(matches!(reply, FromHolder::SpawnFailed { .. }), "{reply:?}");
    let err = post(port, key, "/hook/s1", b"{}")
        .await
        .expect_err("refused");
    assert_eq!(crate::loopback::error_status(&err), Some(401));
    daemon.end_terminals().await;
    drop(daemon);
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_goes_away_fails_a_question_in_flight() {
    let holder = Running::start().await;
    let port = holder.entry().await.port;
    let mut daemon = holder.link().await;
    let key = daemon.issue_launch("t1", "s1").await;
    let asking = tokio::spawn(post(port, key, "/mcp/token", b"{}"));
    daemon.next_relay().await;
    drop(daemon);
    let err = asking.await.unwrap().expect_err("the question failed");
    // The holder is on its way out too, so the helper may be cut off before it hears why.
    assert!(
        crate::loopback::is_daemon_lost(&err) || crate::loopback::error_status(&err).is_none(),
        "{err}"
    );
    holder.ended().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_welcome_says_the_holder_runs_companions() {
    let holder = Running::start().await;
    let daemon = holder.link().await;
    assert!(
        matches!(
            daemon.welcome,
            FromHolder::Welcome {
                companions: true,
                ..
            }
        ),
        "{:?}",
        daemon.welcome
    );
    drop(daemon);
    holder.ended().await;
}

/// The companions: processes a holder starts for a terminal before it, whose companions are real
/// processes in these tests, shells that make the socket they are waited for on and stay up.
#[cfg(unix)]
mod companions {
    use super::*;
    use crate::holder::companion_test_support::{directory_of, socket_kind, touching};
    use crate::holder::protocol::{CompanionEndpoint, EndpointKind, Ready};
    use crate::test_support::{kill, within};

    fn sh(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    /// `spec` as the spawn of a terminal that binds the companion started under its id.
    fn bound(mut spec: SpawnSpec) -> SpawnSpec {
        spec.companion = true;
        spec
    }

    /// Waits for the companion process `pid` to be gone.
    async fn gone(pid: u32) {
        let ended = within(PATIENCE, move || !is_running(pid as i32)).await;
        assert!(ended, "the companion {pid} kept running");
    }

    /// Waits for the directory of the socket endpoint to be removed.
    async fn removed(endpoint: &CompanionEndpoint) {
        let directory = directory_of(endpoint);
        let shown = directory.clone();
        let ended = within(PATIENCE, move || !directory.exists()).await;
        assert!(ended, "{} was not removed", shown.display());
    }

    /// This case proves readiness runs out; a server that never creates its socket needs no
    /// opportunity to answer inside the window.
    const NEVER_READY_WAIT: Duration = Duration::from_millis(500);

    impl Daemon {
        /// The endpoint the holder opens for the companion of the terminal `id`, or why not.
        async fn open_endpoint(
            &mut self,
            id: &str,
            req: u64,
            kind: EndpointKind,
        ) -> Result<CompanionEndpoint, String> {
            self.send(ToHolder::OpenEndpoint {
                id: id.to_string(),
                req,
                kind,
            })
            .await;
            let frames = self
                .until(|frame| {
                    matches!(
                        frame,
                        Frame::Reply(FromHolder::EndpointOpened { req: got, .. }) if *got == req
                    )
                })
                .await;
            match frames.into_iter().last() {
                Some(Frame::Reply(FromHolder::EndpointOpened {
                    endpoint: Some(endpoint),
                    ..
                })) => Ok(endpoint),
                Some(Frame::Reply(FromHolder::EndpointOpened {
                    error: Some(error), ..
                })) => Err(error),
                other => panic!("{other:?}"),
            }
        }

        /// The pid and the port of the companion the holder starts for the terminal `id`, or why it
        /// did not.
        async fn start_companion(
            &mut self,
            id: &str,
            req: u64,
            command: Command,
            ready: Ready,
            within: Duration,
        ) -> Result<(u32, Option<u16>), String> {
            self.send(ToHolder::StartCompanion {
                id: id.to_string(),
                req,
                command: Box::new(command),
                ready,
                ready_within_ms: u64::try_from(within.as_millis()).unwrap(),
                containment: None,
            })
            .await;
            let frames = self
                .until(|frame| {
                    matches!(
                        frame,
                        Frame::Reply(FromHolder::CompanionStarted { req: got, .. }) if *got == req
                    )
                })
                .await;
            match frames.into_iter().last() {
                Some(Frame::Reply(FromHolder::CompanionStarted {
                    pid: Some(pid),
                    port,
                    error: None,
                    ..
                })) => Ok((pid, port)),
                Some(Frame::Reply(FromHolder::CompanionStarted {
                    error: Some(error), ..
                })) => Err(error),
                other => panic!("{other:?}"),
            }
        }

        async fn end_companion(&mut self, id: &str, req: u64) -> FromHolder {
            self.send(ToHolder::EndCompanion {
                id: id.to_string(),
                req,
            })
            .await;
            self.reply_to(req).await
        }

        /// Opens a socket endpoint for the companion of `id` and starts one on it that makes the
        /// socket and stays up: the endpoint and the companion's pid. Uses the requests `req` and
        /// the one after it.
        async fn companion_on_socket(&mut self, id: &str, req: u64) -> (CompanionEndpoint, u32) {
            let endpoint = self
                .open_endpoint(id, req, socket_kind())
                .await
                .expect("the endpoint opens");
            let CompanionEndpoint::Socket { path } = &endpoint else {
                panic!("{endpoint:?}");
            };
            let (pid, port) = self
                .start_companion(id, req + 1, touching(path), Ready::SocketExists, PATIENCE)
                .await
                .expect("the companion starts");
            assert_eq!(port, None);
            (endpoint, pid)
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_companion_started_before_its_terminal_is_listed_with_it_and_ended_with_it() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (endpoint, companion) = daemon.companion_on_socket("t1", 10).await;
        assert!(directory_of(&endpoint).is_dir());
        let reply = daemon
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;
        assert!(pid_of(&reply) > 0);

        drop(daemon);
        let mut next = holder.link().await;
        let terminal = listed(&next)
            .iter()
            .find(|terminal| terminal.id == "t1")
            .expect("the terminal is listed");
        let found = terminal
            .companion
            .as_ref()
            .expect("the companion is listed with its terminal");
        assert_eq!(found.endpoint, endpoint);
        assert_eq!(found.pid, companion);
        assert!(!found.exited);
        assert!(is_running(companion as i32));

        next.end_ids(&["t1"]).await;
        gone(companion).await;
        removed(&endpoint).await;
        drop(next);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_companion_is_ended_once_its_terminal_has_exited() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (endpoint, companion) = daemon.companion_on_socket("t1", 10).await;
        daemon
            .spawn_spec(bound(spec("t1", programs::echo("done").term_command())))
            .await;
        daemon.output_until_exit("t1").await;
        gone(companion).await;
        removed(&endpoint).await;
        daemon.end_terminals().await;
        drop(daemon);
        holder.ended().await;
    }

    /// A companion that ignores the polite request, so that its end takes the whole grace, and
    /// makes the file at `path` once it does.
    fn stubborn(path: &str) -> Command {
        sh(&format!("trap '' TERM; touch '{path}'; exec sleep 60"))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_holder_does_not_exit_while_the_end_of_a_companion_is_under_way() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let endpoint = daemon
            .open_endpoint("t1", 10, socket_kind())
            .await
            .expect("the endpoint opens");
        let CompanionEndpoint::Socket { path } = &endpoint else {
            panic!("{endpoint:?}");
        };
        let (companion, _) = daemon
            .start_companion("t1", 11, stubborn(path), Ready::SocketExists, PATIENCE)
            .await
            .expect("the companion starts");
        daemon
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;
        // The terminal's exit and its release both end the companion, which takes the grace.
        daemon.end_ids(&["t1"]).await;
        drop(daemon);
        holder.ended().await;
        assert!(
            !is_running(companion as i32),
            "the holder exited with the companion still ending"
        );
        removed(&endpoint).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_spawn_after_a_link_takeover_fails_and_the_older_links_companion_is_ended() {
        let holder = Running::start().await;
        let mut older = holder.link().await;
        let (endpoint, companion) = older.companion_on_socket("t1", 10).await;
        let mut newer = holder.link().await;
        let reply = newer
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;
        assert!(matches!(reply, FromHolder::SpawnFailed { .. }), "{reply:?}");
        gone(companion).await;
        removed(&endpoint).await;
        drop((older, newer));
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_companion_that_dies_ends_its_terminal_and_a_linked_daemon_is_told() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (_, companion) = daemon.companion_on_socket("t1", 10).await;
        let reply = daemon
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;
        let terminal = pid_of(&reply) as i32;
        kill(companion as i32);

        let (mut told, mut exited) = (false, false);
        while !(told && exited) {
            match daemon.next().await {
                Some(Frame::Reply(FromHolder::CompanionEnded { id })) if id == "t1" => told = true,
                Some(Frame::Reply(FromHolder::Exited { id, .. })) if id == "t1" => exited = true,
                Some(_) => {}
                None => panic!("the link closed (told: {told}, exited: {exited})"),
            }
        }
        assert!(!is_running(terminal));
        daemon.end_terminals().await;
        drop(daemon);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_companion_that_dies_with_no_daemon_linked_still_ends_its_terminal() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (_, companion) = daemon.companion_on_socket("t1", 10).await;
        daemon
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;
        drop(daemon);
        kill(companion as i32);

        // The next daemon finds both ended, once the holder has seen them go.
        let mut next = holder
            .link_when(|daemon| {
                listed(daemon).iter().any(|terminal| {
                    terminal.id == "t1"
                        && terminal.exited
                        && terminal
                            .companion
                            .as_ref()
                            .is_some_and(|found| found.exited)
                })
            })
            .await;
        next.end_ids(&["t1"]).await;
        drop(next);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_companion_no_terminal_bound_is_ended_with_the_link_that_asked_for_it() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (endpoint, companion) = daemon.companion_on_socket("t1", 10).await;
        drop(daemon);
        gone(companion).await;
        removed(&endpoint).await;
        // Nothing is left for it to wait for: the holder has no daemon and no terminal.
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn revoking_a_launch_ends_the_companion_no_terminal_bound() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (endpoint, companion) = daemon.companion_on_socket("t1", 10).await;
        daemon
            .send(ToHolder::RevokeLaunch {
                id: "t1".to_string(),
            })
            .await;
        gone(companion).await;
        removed(&endpoint).await;
        drop(daemon);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ending_a_companion_removes_it_unless_its_terminal_has_bound_it() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (endpoint, companion) = daemon.companion_on_socket("t1", 10).await;
        let ended = daemon.end_companion("t1", 20).await;
        assert!(
            matches!(ended, FromHolder::Done { error: None, .. }),
            "{ended:?}"
        );
        gone(companion).await;
        removed(&endpoint).await;

        let (_, bound_companion) = daemon.companion_on_socket("t2", 30).await;
        daemon
            .spawn_spec(bound(spec("t2", programs::sleep(60).term_command())))
            .await;
        let refused = daemon.end_companion("t2", 40).await;
        assert!(
            matches!(refused, FromHolder::Done { error: Some(_), .. }),
            "{refused:?}"
        );
        assert!(is_running(bound_companion as i32));
        daemon.end_terminals().await;
        gone(bound_companion).await;
        drop(daemon);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_spawn_that_expects_a_missing_or_dead_companion_is_refused() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let missing = daemon
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;
        assert!(
            matches!(missing, FromHolder::SpawnFailed { .. }),
            "{missing:?}"
        );

        let (_, companion) = daemon.companion_on_socket("t2", 10).await;
        kill(companion as i32);
        daemon
            .until(|frame| {
                matches!(frame, Frame::Reply(FromHolder::CompanionEnded { id }) if id == "t2")
            })
            .await;
        let dead = daemon
            .spawn_spec(bound(spec("t2", programs::sleep(60).term_command())))
            .await;
        assert!(matches!(dead, FromHolder::SpawnFailed { .. }), "{dead:?}");
        drop(daemon);
        // No terminal was started and no companion is left awaiting one.
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_spawn_that_expects_no_companion_ends_the_one_left_under_its_id() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let (endpoint, companion) = daemon.companion_on_socket("t1", 10).await;
        let reply = daemon
            .spawn_spec(spec("t1", programs::sleep(60).term_command()))
            .await;
        assert!(pid_of(&reply) > 0);
        gone(companion).await;
        removed(&endpoint).await;

        drop(daemon);
        let mut next = holder.link().await;
        let terminal = listed(&next)
            .iter()
            .find(|terminal| terminal.id == "t1")
            .expect("the terminal is listed");
        assert_eq!(terminal.companion, None);
        next.end_ids(&["t1"]).await;
        drop(next);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_second_endpoint_for_the_same_terminal_is_elsewhere_and_ends_the_first() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let first = daemon
            .open_endpoint("t1", 10, socket_kind())
            .await
            .expect("the first endpoint opens");
        let second = daemon
            .open_endpoint("t1", 11, socket_kind())
            .await
            .expect("the second endpoint opens");
        assert_ne!(first, second);
        assert_ne!(directory_of(&first), directory_of(&second));
        removed(&first).await;
        assert!(directory_of(&second).is_dir());

        let CompanionEndpoint::Socket { path } = &second else {
            panic!("{second:?}");
        };
        let started = daemon
            .start_companion("t1", 12, touching(path), Ready::SocketExists, PATIENCE)
            .await;
        let (companion, _) = started.expect("the companion starts on the second endpoint");
        let ended = daemon.end_companion("t1", 13).await;
        assert!(matches!(ended, FromHolder::Done { error: None, .. }));
        gone(companion).await;
        removed(&second).await;
        drop(daemon);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_port_printed_after_the_prefix_is_reported_and_listed_with_the_terminal() {
        const PREFIX: &str = "listening on: ws://127.0.0.1:";
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let opened = daemon
            .open_endpoint("t1", 10, EndpointKind::Loopback)
            .await
            .expect("the endpoint opens");
        let CompanionEndpoint::Loopback { token, port: None } = opened else {
            panic!("a loopback endpoint with no port yet: {opened:?}");
        };
        assert_eq!(token.len(), 64);
        let server = sh(&format!(
            "echo starting; echo '{PREFIX}0'; echo '{PREFIX}4321'; exec sleep 60"
        ));
        let ready = Ready::PortAfter {
            prefix: PREFIX.to_string(),
        };
        let (companion, port) = daemon
            .start_companion("t1", 11, server, ready, PATIENCE)
            .await
            .expect("the companion starts");
        assert_eq!(port, Some(4321));
        daemon
            .spawn_spec(bound(spec("t1", programs::sleep(60).term_command())))
            .await;

        drop(daemon);
        let mut next = holder.link().await;
        let found = listed(&next)
            .iter()
            .find(|terminal| terminal.id == "t1")
            .and_then(|terminal| terminal.companion.clone())
            .expect("the companion is listed with its terminal");
        assert_eq!(
            found.endpoint,
            CompanionEndpoint::Loopback {
                token,
                port: Some(4321)
            }
        );
        assert_eq!(found.pid, companion);
        next.end_ids(&["t1"]).await;
        gone(companion).await;
        drop(next);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_companion_that_exits_or_is_not_ready_fails_its_start_and_leaves_nothing() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;

        let exits = daemon
            .open_endpoint("t1", 10, socket_kind())
            .await
            .expect("the endpoint opens");
        let error = daemon
            .start_companion("t1", 11, sh("exit 3"), Ready::SocketExists, PATIENCE)
            .await
            .expect_err("a companion that exits is not ready");
        assert!(error.contains("exited before it was ready"), "{error}");
        removed(&exits).await;

        // Never ready within the short wait this test gives it.
        let silent = daemon
            .open_endpoint("t2", 20, socket_kind())
            .await
            .expect("the endpoint opens");
        let error = daemon
            .start_companion(
                "t2",
                21,
                sh("exec sleep 60"),
                Ready::SocketExists,
                NEVER_READY_WAIT,
            )
            .await
            .expect_err("a companion that never listens is not ready");
        assert!(error.contains("did not become ready"), "{error}");
        removed(&silent).await;

        // Nothing awaits a terminal any more: a spawn expecting either companion is refused.
        for id in ["t1", "t2"] {
            let reply = daemon
                .spawn_spec(bound(spec(id, programs::sleep(60).term_command())))
                .await;
            assert!(matches!(reply, FromHolder::SpawnFailed { .. }), "{reply:?}");
        }
        drop(daemon);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_endpoint_or_readiness_the_holder_does_not_know_is_refused() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        let unknown = daemon.open_endpoint("t1", 10, EndpointKind::Unknown).await;
        assert!(unknown.is_err(), "{unknown:?}");
        let path = daemon
            .open_endpoint(
                "t1",
                11,
                EndpointKind::Socket {
                    prefix: "../escape".to_string(),
                    name: "rpc".to_string(),
                },
            )
            .await;
        assert!(
            path.is_err(),
            "a prefix that is a path was opened: {path:?}"
        );

        let endpoint = daemon
            .open_endpoint("t1", 12, socket_kind())
            .await
            .expect("the endpoint opens");
        let started = daemon
            .start_companion("t1", 13, sh("exec sleep 60"), Ready::Unknown, PATIENCE)
            .await;
        assert!(started.is_err(), "{started:?}");
        removed(&endpoint).await;
        drop(daemon);
        holder.ended().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_draining_holder_refuses_a_companion_and_waits_for_one_that_awaits_its_terminal() {
        let holder = Running::start().await;
        let mut daemon = holder.link().await;
        daemon.spawn("t1", programs::sleep(60).term_command()).await;
        let awaiting = daemon
            .open_endpoint("t2", 10, socket_kind())
            .await
            .expect("the endpoint opens before the drain");
        daemon.send(ToHolder::Drain).await;

        let refused = daemon.open_endpoint("t3", 11, socket_kind()).await;
        assert!(refused.is_err(), "{refused:?}");
        let refused = daemon
            .start_companion("t2", 12, sh("exec sleep 60"), Ready::SocketExists, PATIENCE)
            .await;
        assert!(refused.is_err(), "{refused:?}");

        // The last terminal goes, and the companion awaiting its own keeps the holder serving
        // until it is ended.
        daemon.end_ids(&["t1"]).await;
        let ended = daemon.end_companion("t2", 13).await;
        assert!(
            matches!(ended, FromHolder::Done { error: None, .. }),
            "{ended:?}"
        );
        removed(&awaiting).await;
        holder.ended().await;
        drop(daemon);
    }
}
