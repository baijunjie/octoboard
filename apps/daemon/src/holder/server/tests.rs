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
    tokio::time::sleep(Duration::from_millis(200)).await;

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
