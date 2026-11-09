//! The client against a holder that is only a script: what it does while a spawn is unconfirmed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::holder::protocol::{FromHolder, LastParting, SpawnSpec, ToHolder};
use crate::holder::registry::RegistryEntry;
use crate::platform::term::Command;
use crate::test_support::{ScratchDir, PATIENCE};

/// What the scripted holder does once it has been asked to start a program.
#[derive(Clone, Copy)]
enum Answer {
    /// Fails the start, after the delay.
    Fail,
    /// Confirms the start, after the delay.
    Confirm,
    /// Goes away, after the delay.
    Vanish,
}

/// The welcome of a holder that runs no terminal.
fn welcome_text() -> String {
    welcome_text_of("test")
}

/// [`welcome_text`] from a holder of `version`.
fn welcome_text_of(version: &str) -> String {
    let welcome = FromHolder::Welcome {
        holder_id: "scripted".to_string(),
        instance_id: "instance".to_string(),
        version: version.to_string(),
        build_id: "test".to_string(),
        last_parting: LastParting::None,
        terminals: Vec::new(),
        facts: crate::platform::term::facts(),
        answers_offline: Vec::new(),
        handover: false,
        companions: false,
    };
    serde_json::to_string(&welcome).unwrap()
}

/// A holder that welcomes the daemon with no terminals, and answers the first spawn as `answer`
/// says after `delay`. Returns the port it listens on.
async fn scripted_holder(answer: Answer, delay: Duration) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("the daemon connects");
        let mut socket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("a websocket");
        // The hello.
        socket.next().await;
        socket
            .send(Message::Text(welcome_text()))
            .await
            .expect("the welcome");
        let id = loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(ToHolder::Spawn(spec)) = serde_json::from_str(&text) {
                        break spec.id;
                    }
                }
                _ => return,
            }
        };
        tokio::time::sleep(delay).await;
        let reply = match answer {
            Answer::Fail => FromHolder::SpawnFailed {
                id,
                error: "late failure".to_string(),
            },
            Answer::Confirm => FromHolder::Spawned {
                id,
                pid: 4242,
                identity: None,
            },
            Answer::Vanish => return,
        };
        socket
            .send(Message::Text(serde_json::to_string(&reply).unwrap()))
            .await
            .ok();
        // Held open until the daemon parts, so that its side ends only by what the test does.
        tokio::time::timeout(PATIENCE, async {
            while let Some(Ok(_)) = socket.next().await {}
        })
        .await
        .ok();
    });
    port
}

fn holders_over(dir: &ScratchDir, port: u16) -> Holders {
    Holders::new(ClientConfig {
        registry_dir: dir.join("holders"),
        exe: PathBuf::from("/x"),
        builds: None,
        range: Range::current(),
        relay: Arc::new(Unavailable),
        launch: Box::new(move |_, holder_id, registry_dir| {
            registry::write(
                registry_dir,
                &RegistryEntry {
                    holder_id: holder_id.to_string(),
                    pid: std::process::id(),
                    started: String::new(),
                    port,
                    secret: "secret".to_string(),
                    version: "test".to_string(),
                    build_id: "test".to_string(),
                    exe: "/x".to_string(),
                },
            )
        }),
    })
    .expect("holders")
}

fn spec() -> SpawnSpec {
    SpawnSpec {
        id: "t1".to_string(),
        command: Command::new("/bin/true"),
        size: Size { rows: 24, cols: 80 },
        containment: None,
        kill_session_leftovers: false,
        meta: serde_json::Value::Null,
        companion: false,
    }
}

/// Runs the spawn on a thread, as the daemon's launch does, and says whether it has returned.
fn spawning(
    holders: Holders,
) -> (
    Arc<AtomicBool>,
    std::thread::JoinHandle<(Holders, Result<HeldTerminal>)>,
) {
    let returned = Arc::new(AtomicBool::new(false));
    let flag = returned.clone();
    let thread = std::thread::spawn(move || {
        let spawned = holders.spawn(spec());
        flag.store(true, Ordering::Release);
        (holders, spawned)
    });
    (returned, thread)
}

/// A start the holder has not confirmed is waited for, not given up on and started again: the
/// caller keeps what it claimed for the launch until the holder says, and a failure that comes
/// late is the answer.
#[tokio::test(flavor = "multi_thread")]
async fn an_unconfirmed_spawn_keeps_waiting_and_a_late_failure_ends_it() {
    let dir = ScratchDir::new("client-late-failure");
    let port = scripted_holder(Answer::Fail, Duration::from_millis(1500)).await;
    let (returned, thread) = spawning(holders_over(&dir, port));
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        !returned.load(Ordering::Acquire),
        "the spawn was given up on before the holder answered"
    );

    let (_holders, spawned) = tokio::task::spawn_blocking(move || thread.join().unwrap())
        .await
        .unwrap();
    let err = spawned.err().expect("the late failure is the answer");
    assert!(format!("{err:#}").contains("late failure"), "{err:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_late_confirmation_is_the_terminal() {
    let dir = ScratchDir::new("client-late-confirmation");
    let port = scripted_holder(Answer::Confirm, Duration::from_millis(1200)).await;
    let (returned, thread) = spawning(holders_over(&dir, port));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!returned.load(Ordering::Acquire));
    let (_holders, spawned) = tokio::task::spawn_blocking(move || thread.join().unwrap())
        .await
        .unwrap();
    assert_eq!(spawned.expect("confirmed").pid().0, 4242);
}

/// A holder that is gone with the start pending runs nothing, so the claim is let go.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_lost_with_a_spawn_pending_ends_the_wait() {
    let dir = ScratchDir::new("client-lost-holder");
    let port = scripted_holder(Answer::Vanish, Duration::from_millis(300)).await;
    let (_returned, thread) = spawning(holders_over(&dir, port));
    let started = Instant::now();
    let (_holders, spawned) = tokio::task::spawn_blocking(move || thread.join().unwrap())
        .await
        .unwrap();
    let err = spawned.err().expect("the link was lost");
    assert!(format!("{err:#}").contains("lost"), "{err:#}");
    assert!(started.elapsed() < PATIENCE);
}

/// A holder that welcomes the daemon once and hangs up, and takes no second connection.
#[cfg(unix)]
async fn one_shot_holder() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("the daemon connects");
        drop(listener);
        let mut socket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("a websocket");
        socket.next().await;
        socket
            .send(Message::Text(welcome_text()))
            .await
            .expect("the welcome");
        socket.close(None).await.ok();
    });
    port
}

/// A stand-in for a holder's process, which a registry file names by its identity, reaped as soon
/// as it ends, as the daemon reaps a holder it started.
#[cfg(unix)]
struct StandIn {
    pid: u32,
    started: String,
    reaper: std::thread::JoinHandle<std::io::Result<std::process::ExitStatus>>,
}

#[cfg(unix)]
impl StandIn {
    fn start() -> Self {
        let mut process = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("a process");
        let pid = process.id();
        let start = crate::platform::proc_table::start_of(pid as i32).expect("it runs");
        Self {
            pid,
            started: format!("{pid}:{start}"),
            reaper: std::thread::spawn(move || process.wait()),
        }
    }

    fn entry(&self, holder_id: &str, port: u16) -> RegistryEntry {
        RegistryEntry {
            holder_id: holder_id.to_string(),
            pid: self.pid,
            started: self.started.clone(),
            port,
            secret: "secret".to_string(),
            version: "test".to_string(),
            build_id: "test".to_string(),
            exe: "/x".to_string(),
        }
    }

    /// Ends the stand-in, as a holder that goes away does.
    fn end(self) {
        crate::test_support::kill(self.pid as i32);
        self.reaper.join().ok();
    }
}

/// A link that drops while the holder's process still runs, and cannot be opened again for now,
/// is pending: the holder is not ended, and the link is tried again in the background until the
/// holder is gone, when it is lost.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_cannot_be_opened_again_for_now_is_pending_and_never_ends_the_holder() {
    let dir = ScratchDir::new("client-link-dropped");
    let port = one_shot_holder().await;
    let stand_in = StandIn::start();
    let entry = stand_in.entry("dropped", port);
    // Its file is there: the holder did not end on its own.
    registry::write(&dir.join("holders"), &entry).unwrap();
    // The directory comes back out, so that the registry stays until the test ends.
    let (holders, link, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.connect(&entry).expect("linked");
        (holders, link, dir)
    })
    .await
    .unwrap();

    let deadline = Instant::now() + PATIENCE;
    while !link.pending() {
        assert!(Instant::now() < deadline, "the link never became pending");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!link.alive());
    // Several tries later the holder still runs.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(crate::test_support::is_running(stand_in.pid as i32));
    assert!(link.pending(), "a refused connection lost the link");

    stand_in.end();
    while link.pending() {
        assert!(Instant::now() < deadline, "the link is still pending");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!link.alive());
    assert_eq!(link.connections(), 1);
    tokio::task::spawn_blocking(move || drop(holders))
        .await
        .unwrap();
    drop(dir);
}

/// A holder of this build's version that welcomes the first connection, turns the next `refused`
/// away, then welcomes again: one alive but too busy to take a link for a while.
#[cfg(unix)]
async fn refusing_holder(refused: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut accepted = 0;
        while let Ok((stream, _)) = listener.accept().await {
            accepted += 1;
            let welcomes = accepted == 1 || accepted > refused + 1;
            if !welcomes {
                drop(stream);
                continue;
            }
            let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                continue;
            };
            socket.next().await;
            let welcome = welcome_text_of(env!("CARGO_PKG_VERSION"));
            socket.send(Message::Text(welcome)).await.ok();
            if accepted == 1 {
                // The first link drops at once.
                socket.close(None).await.ok();
                continue;
            }
            tokio::spawn(async move {
                tokio::time::timeout(PATIENCE, async {
                    while let Some(Ok(_)) = socket.next().await {}
                })
                .await
                .ok();
            });
        }
    });
    port
}

/// A holder that runs but turns connections away for a while is linked to again once it takes
/// one: the link was pending meanwhile, and nothing ended the holder.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_that_refuses_connections_for_a_while_is_linked_to_again() {
    let dir = ScratchDir::new("client-refusing");
    let port = refusing_holder(3).await;
    let stand_in = StandIn::start();
    let entry = stand_in.entry("refusing", port);
    registry::write(&dir.join("holders"), &entry).unwrap();
    // The directory comes back out, so that the registry stays until the test ends.
    let (holders, link, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.connect(&entry).expect("linked");
        (holders, link, dir)
    })
    .await
    .unwrap();

    let deadline = Instant::now() + PATIENCE;
    while !(link.alive() && link.connections() == 2) {
        assert!(Instant::now() < deadline, "the link was not opened again");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!link.pending());
    assert!(crate::test_support::is_running(stand_in.pid as i32));
    tokio::task::spawn_blocking(move || drop(holders))
        .await
        .unwrap();
    stand_in.end();
    drop(dir);
}

/// A holder that left no registry file ended on its own: its link is lost without being opened
/// again, and nothing is signalled, whatever process has the pid by now.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_drops_to_a_holder_without_a_registry_file_is_lost_and_nothing_is_signalled() {
    let dir = ScratchDir::new("client-no-file");
    let port = one_shot_holder().await;
    let mut stand_in = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("a process");
    let pid = stand_in.id();
    let start = crate::platform::proc_table::start_of(pid as i32).expect("it runs");
    let entry = RegistryEntry {
        holder_id: "ended-itself".to_string(),
        pid,
        started: format!("{pid}:{start}"),
        port,
        secret: "secret".to_string(),
        version: "test".to_string(),
        build_id: "test".to_string(),
        exe: "/x".to_string(),
    };
    // The directory comes back out, so that the registry stays until the test ends.
    let (holders, link, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.connect(&entry).expect("linked");
        (holders, link, dir)
    })
    .await
    .unwrap();

    let deadline = Instant::now() + PATIENCE;
    while link.alive() {
        assert!(Instant::now() < deadline, "the link is still alive");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Long enough for a signal sent in error to have landed.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(crate::test_support::is_running(pid as i32));
    assert_eq!(link.connections(), 1, "the link was opened again");
    stand_in.kill().ok();
    stand_in.wait().ok();
    tokio::task::spawn_blocking(move || drop(holders))
        .await
        .unwrap();
    drop(dir);
}

/// A holder in the registry whose process runs but that nobody answers for is not ended: it is
/// pending, tried again in the background, its registry file kept, and resolved as gone once its
/// process is.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_that_runs_but_cannot_be_linked_to_is_pending_until_it_is_gone() {
    let dir = ScratchDir::new("client-unlinkable");
    // A port nothing listens on.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        listener.local_addr().unwrap().port()
    };
    let stand_in = StandIn::start();
    let entry = stand_in.entry("unlinkable", port);
    let registry_dir = dir.join("holders");
    registry::write(&registry_dir, &entry).unwrap();
    let (resolved, resolutions) = std::sync::mpsc::channel();
    let on_resolved: OnResolved = Arc::new(move |resolution| {
        let gone = match resolution {
            Resolved::Gone { holder_id, .. } => holder_id,
            Resolved::Linked { found, .. } => panic!("linked to {}", found.holder_id),
        };
        resolved.send(gone).ok();
    });
    // The directory comes back out, so that the registry stays until the test ends.
    let (holders, adoption, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let adoption = holders.adopt_existing(on_resolved);
        (holders, adoption, dir)
    })
    .await
    .unwrap();
    assert!(adoption.found.is_empty());
    assert_eq!(adoption.pending, vec!["unlinkable".to_string()]);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(crate::test_support::is_running(stand_in.pid as i32));
    assert!(registry::read(&registry_dir, "unlinkable").is_some());

    stand_in.end();
    let gone = tokio::task::spawn_blocking(move || resolutions.recv_timeout(PATIENCE))
        .await
        .unwrap()
        .expect("resolved");
    assert_eq!(gone, "unlinkable");
    assert!(registry::read(&registry_dir, "unlinkable").is_none());
    tokio::task::spawn_blocking(move || drop(holders))
        .await
        .unwrap();
    drop(dir);
}

/// A holder that runs but turns the start's connections away is linked to once it takes one, and
/// taken over then, with the terminals it lists.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_that_could_not_be_linked_to_at_start_is_taken_over_once_it_answers() {
    let dir = ScratchDir::new("client-late-link");
    // The start's one try and the first try after it are refused; the first connection, which
    // it welcomes, is the test's.
    let port = refusing_holder(2).await;
    {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/daemon"))
                .await
                .expect("the first connection");
        socket.send(Message::Text("{}".to_string())).await.ok();
        socket.next().await;
    }
    let stand_in = StandIn::start();
    // Named as the scripted holder's welcome names it.
    let entry = stand_in.entry("scripted", port);
    let registry_dir = dir.join("holders");
    registry::write(&registry_dir, &entry).unwrap();
    let (resolved, resolutions) = std::sync::mpsc::channel();
    let on_resolved: OnResolved = Arc::new(move |resolution| {
        let linked = match resolution {
            Resolved::Linked {
                holder_id,
                found,
                relays,
            } => {
                drop(relays);
                assert_eq!(found.holder_id, holder_id);
                Some(holder_id)
            }
            Resolved::Gone { .. } => None,
        };
        resolved.send(linked).ok();
    });
    // The directory comes back out, so that the registry stays until the test ends.
    let (holders, adoption, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let adoption = holders.adopt_existing(on_resolved);
        (holders, adoption, dir)
    })
    .await
    .unwrap();
    assert!(adoption.found.is_empty());
    assert_eq!(adoption.pending, vec!["scripted".to_string()]);
    let linked = tokio::task::spawn_blocking(move || resolutions.recv_timeout(PATIENCE))
        .await
        .unwrap()
        .expect("resolved");
    assert_eq!(linked.as_deref(), Some("scripted"));
    assert!(crate::test_support::is_running(stand_in.pid as i32));
    tokio::task::spawn_blocking(move || drop(holders))
        .await
        .unwrap();
    stand_in.end();
    drop(dir);
}

/// Holders over a holder that turns the start's connections away and is then linked to in the
/// background, with the relays that link resolves with and the stand-in process the holder's
/// registry file names: ending the holder by its identity ends the stand-in.
#[cfg(unix)]
async fn linked_late(label: &str) -> (Holders, HeldRelays, StandIn, ScratchDir) {
    let dir = ScratchDir::new(label);
    // The start's one try and the first try after it are refused; the first connection, which
    // it welcomes, is this one's.
    let port = refusing_holder(2).await;
    {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/daemon"))
                .await
                .expect("the first connection");
        socket.send(Message::Text("{}".to_string())).await.ok();
        socket.next().await;
    }
    let stand_in = StandIn::start();
    // Named as the scripted holder's welcome names it.
    registry::write(&dir.join("holders"), &stand_in.entry("scripted", port)).unwrap();
    let (resolved, resolutions) = std::sync::mpsc::channel();
    let on_resolved: OnResolved = Arc::new(move |resolution| match resolution {
        Resolved::Linked {
            holder_id, relays, ..
        } => {
            assert_eq!(holder_id, "scripted");
            resolved.send(relays).ok();
        }
        Resolved::Gone { holder_id, .. } => panic!("{holder_id} is gone"),
    });
    let (holders, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let adoption = holders.adopt_existing(on_resolved);
        assert_eq!(adoption.pending, vec!["scripted".to_string()]);
        (holders, dir)
    })
    .await
    .unwrap();
    let relays = tokio::task::spawn_blocking(move || resolutions.recv_timeout(PATIENCE))
        .await
        .unwrap()
        .expect("linked to");
    (holders, relays, stand_in, dir)
}

/// A holder linked to before a planned stop began and left to the next daemon once it has is
/// among the holders left to it, and an ordinary stop that escalates the planned stop takes its
/// link and ends it with the rest.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_left_to_the_next_daemon_is_ended_by_the_escalation() {
    let (holders, relays, stand_in, dir) = linked_late("client-left-for-successor").await;
    let pid = stand_in.pid as i32;
    let (holders, relays) = tokio::task::spawn_blocking(move || {
        assert!(holders.versions().contains_key("scripted"));
        holders.begin_planned_stop();
        relays.leave_for_successor();
        assert!(holders.left_to_successor().contains("scripted"));
        // Still linked to: nothing parts from it until the stop does.
        assert!(holders.versions().contains_key("scripted"));
        assert!(crate::test_support::is_running(pid));

        holders.end_handed_over();
        assert!(!holders.versions().contains_key("scripted"));
        assert!(holders.left_to_successor().is_empty());
        (holders, relays)
    })
    .await
    .unwrap();
    assert!(
        !crate::test_support::is_running(pid),
        "the holder left to the next daemon was left running"
    );
    stand_in.reaper.join().ok();
    tokio::task::spawn_blocking(move || {
        drop(relays);
        drop(holders);
    })
    .await
    .unwrap();
    drop(dir);
}

/// A holder left to the next daemon once an ordinary stop has begun to end those left to it is
/// ended at once, by its verified process.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_left_to_the_next_daemon_after_the_escalation_began_is_ended_at_once() {
    let (holders, relays, stand_in, dir) = linked_late("client-left-after-escalation").await;
    let pid = stand_in.pid as i32;
    let (holders, relays) = tokio::task::spawn_blocking(move || {
        holders.begin_planned_stop();
        // Nothing handed over or parted from yet: nothing to end.
        holders.end_handed_over();
        assert!(crate::test_support::is_running(pid));
        relays.leave_for_successor();
        assert!(holders.left_to_successor().is_empty());
        (holders, relays)
    })
    .await
    .unwrap();
    assert!(
        !crate::test_support::is_running(pid),
        "the holder left to the next daemon was left running"
    );
    stand_in.reaper.join().ok();
    tokio::task::spawn_blocking(move || {
        drop(relays);
        drop(holders);
    })
    .await
    .unwrap();
    drop(dir);
}

/// A holder whose process cannot be verified is gone once nothing listens on its port: its
/// registry file is removed.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_that_cannot_be_verified_and_is_not_listening_loses_its_registry_file() {
    let dir = ScratchDir::new("client-unverifiable-gone");
    // A port nothing listens on.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        listener.local_addr().unwrap().port()
    };
    let registry_dir = dir.join("holders");
    write_unverifiable(&registry_dir, port);
    // The directory comes back out, so that the registry stays until the test ends.
    let (found, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let found = holders.adopt_existing(unresolved()).found;
        drop(holders);
        (found, dir)
    })
    .await
    .unwrap();
    assert!(found.is_empty());
    assert!(registry::read(&registry_dir, "unverifiable").is_none());
    drop(dir);
}

/// A holder whose process cannot be verified and that cannot be linked to, though something
/// answers on its port, is left with its registry file, for a later start to try again: nothing
/// says it is gone.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_that_cannot_be_verified_or_linked_to_keeps_its_registry_file() {
    let dir = ScratchDir::new("client-unverifiable");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    // Takes every connection and drops it: it answers, and never welcomes.
    let answering = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            drop(socket);
        }
    });
    let registry_dir = dir.join("holders");
    write_unverifiable(&registry_dir, port);
    // The directory comes back out, so that the registry stays until the test ends.
    let (found, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let found = holders.adopt_existing(unresolved()).found;
        drop(holders);
        (found, dir)
    })
    .await
    .unwrap();
    answering.abort();
    assert!(found.is_empty());
    assert!(registry::read(&registry_dir, "unverifiable").is_some());
    drop(dir);
}

fn write_unverifiable(registry_dir: &Path, port: u16) {
    registry::write(
        registry_dir,
        &RegistryEntry {
            holder_id: "unverifiable".to_string(),
            pid: 1,
            started: "no identity".to_string(),
            port,
            secret: "secret".to_string(),
            version: "test".to_string(),
            build_id: "test".to_string(),
            exe: "/x".to_string(),
        },
    )
    .unwrap();
}

/// A link that drops while the holder runs is opened again, and the terminal the holder runs stays
/// the session's: it answers over the new connection and has not exited, and its owner is told the
/// link was down.
#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_drops_to_a_holder_that_runs_is_opened_again_with_its_terminals() {
    let dir = ScratchDir::new("client-relink");
    // Only the path goes in, so that the registry stays until the test ends.
    let into = dir.join("holders");
    let (holders, held, link) = tokio::task::spawn_blocking(move || {
        let holders = in_process(into).expect("holders");
        let mut spec = spec();
        spec.command = crate::test_support::programs::sleep(60).term_command();
        let held = holders.spawn(spec).expect("the program starts");
        let link = holders
            .current
            .lock()
            .unwrap()
            .clone()
            .expect("the current link");
        (holders, held, link)
    })
    .await
    .unwrap();

    let resumed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = resumed.clone();
    held.on_resumed(move || {
        counted.fetch_add(1, Ordering::AcqRel);
    });
    link.drop_connection();
    let deadline = Instant::now() + PATIENCE;
    while !(link.connections() == 2 && link.alive()) {
        assert!(Instant::now() < deadline, "the link was not opened again");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The terminal's owner is told what it printed meanwhile went nowhere, once.
    while resumed.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < deadline, "the owner was never told");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(resumed.load(Ordering::Acquire), 1);
    tokio::task::spawn_blocking(move || {
        assert!(
            held.size().is_ok(),
            "the terminal answers over the new link"
        );
        assert_eq!(held.exited_unreaped(), Some(false));
        held.end();
        drop(holders);
    })
    .await
    .unwrap();
}

/// A link that drops again right after it was opened again is opened again once more, after a
/// pause: a short drop says nothing of whether the holder can go on, so it is not ended and the
/// terminal stays the session's, still running.
#[tokio::test(flavor = "multi_thread")]
async fn a_link_that_drops_again_right_after_it_was_opened_again_is_opened_again_after_a_pause() {
    let dir = ScratchDir::new("client-flapping");
    // Only the path goes in, so that the registry stays until the test ends.
    let into = dir.join("holders");
    let (holders, held, link) = tokio::task::spawn_blocking(move || {
        let holders = in_process(into).expect("holders");
        let mut spec = spec();
        spec.command = crate::test_support::programs::sleep(60).term_command();
        let held = holders.spawn(spec).expect("the program starts");
        let link = locked(&holders.current).clone().expect("the current link");
        (holders, held, link)
    })
    .await
    .unwrap();
    let _guard = crate::test_support::KillOnDrop(vec![held.pid().0 as i32]);

    link.drop_connection();
    let deadline = Instant::now() + PATIENCE;
    while !(link.connections() == 2 && link.alive()) {
        assert!(Instant::now() < deadline, "the link was not opened again");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    link.drop_connection();
    while !(link.connections() == 3 && link.alive()) {
        assert!(
            Instant::now() < deadline,
            "the link was not opened a third time"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!link.pending());
    tokio::task::spawn_blocking(move || {
        assert_eq!(held.exited_unreaped(), Some(false));
        assert!(
            held.size().is_ok(),
            "the terminal answers over the new link"
        );
        held.end();
        drop(holders);
    })
    .await
    .unwrap();
}

/// A write too large for one frame goes in as several, and a failure partway counts the chunks
/// that went in whole with what the failing one got.
#[tokio::test(flavor = "multi_thread")]
async fn a_large_write_goes_in_chunks_and_a_failure_counts_what_went_in() {
    let dir = ScratchDir::new("client-chunked-write");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    let writes = Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
    let seen = writes.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("the daemon connects");
        let mut socket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("a websocket");
        socket.next().await;
        socket
            .send(Message::Text(welcome_text()))
            .await
            .expect("the welcome");
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let reply = match serde_json::from_str::<ToHolder>(&text) {
                Ok(ToHolder::Spawn(spec)) => FromHolder::Spawned {
                    id: spec.id,
                    pid: 4242,
                    identity: None,
                },
                Ok(ToHolder::Write { req, bytes, .. }) => {
                    let mut seen = seen.lock().unwrap();
                    seen.push(bytes.len());
                    // The second chunk fails after seven bytes.
                    let failed = seen.len() == 2;
                    FromHolder::Written {
                        req,
                        written: if failed { 7 } else { bytes.len() },
                        error: failed.then(|| crate::holder::protocol::WireError {
                            kind: "BrokenPipe".to_string(),
                            message: "stuck".to_string(),
                        }),
                    }
                }
                _ => continue,
            };
            socket
                .send(Message::Text(serde_json::to_string(&reply).unwrap()))
                .await
                .ok();
        }
    });
    let failed = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let held = holders.spawn(spec()).expect("the terminal");
        let failed = held
            .write_raw(&vec![b'x'; 3 * 1024 * 1024])
            .expect_err("the second chunk fails");
        drop((held, holders));
        failed
    })
    .await
    .unwrap();
    assert_eq!(failed.written, 1024 * 1024 + 7);
    assert_eq!(*writes.lock().unwrap(), [1024 * 1024, 1024 * 1024]);
}

/// A holder whose welcome names another holder than its registry file cannot be read as the one
/// the file names: it is ended by its process, not taken over and not left pending.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_whose_welcome_names_another_holder_is_ended() {
    let dir = ScratchDir::new("client-other-id");
    let port = refusing_holder(0).await;
    let stand_in = StandIn::start();
    let entry = stand_in.entry("not-the-scripted-one", port);
    let registry_dir = dir.join("holders");
    registry::write(&registry_dir, &entry).unwrap();
    let pid = stand_in.pid;
    let adoption = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let adoption = holders.adopt_existing(unresolved());
        drop(holders);
        adoption
    })
    .await
    .unwrap();
    assert!(adoption.found.is_empty());
    // Being ended off the start, so that what it ran stays claimed until it is gone.
    assert!(adoption.pending.is_empty());
    assert_eq!(adoption.ending, vec!["not-the-scripted-one".to_string()]);
    let status = tokio::task::spawn_blocking(move || stand_in.reaper.join().unwrap())
        .await
        .unwrap()
        .expect("the process ended");
    assert!(!status.success(), "the holder {pid} was asked to end");
    // Ended off the start, which removes the file once the process is gone.
    let deadline = Instant::now() + PATIENCE;
    while registry::read(&registry_dir, "not-the-scripted-one").is_some() {
        assert!(
            Instant::now() < deadline,
            "the registry file was not removed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A run of dealing with a dropped connection that a newer one has overtaken (the connection it
/// opened again dropped at once) changes neither whether the link is pending nor whether it is
/// recovering: those are the newer run's.
#[tokio::test(flavor = "multi_thread")]
async fn an_overtaken_recovery_run_leaves_the_link_state_to_the_newer_one() {
    let dir = ScratchDir::new("client-overtaken-run");
    // Only the path goes in, so that the registry stays until the test ends.
    let into = dir.join("holders");
    let (holders, held, link) = tokio::task::spawn_blocking(move || {
        let holders = in_process(into).expect("holders");
        let mut spec = spec();
        spec.command = crate::test_support::programs::sleep(60).term_command();
        let held = holders.spawn(spec).expect("the program starts");
        let link = locked(&holders.current).clone().expect("the current link");
        (holders, held, link)
    })
    .await
    .unwrap();
    let _guard = crate::test_support::KillOnDrop(vec![held.pid().0 as i32]);

    let stale = link.next_recovery_run();
    let newer = link.next_recovery_run();
    link.wait_in_background(stale);
    assert!(!link.pending(), "an overtaken run made the link pending");
    link.wait_in_background(newer);
    assert!(link.pending());
    assert!(held.unlinked());
    tokio::task::spawn_blocking(move || drop((held, holders)))
        .await
        .unwrap();
}

/// A link taken over by a connection of this very daemon's run (one it gave up, whose handshake
/// got through late) is not lost to another daemon: it is opened again, its terminal still
/// running, and takes the holder back.
#[tokio::test(flavor = "multi_thread")]
async fn a_link_taken_over_by_its_own_run_is_opened_again() {
    let dir = ScratchDir::new("client-own-run");
    let registry_dir = dir.join("holders");
    // Only the path goes in, so that the registry stays until the test ends.
    let into = dir.join("holders");
    let (holders, held, link) = tokio::task::spawn_blocking(move || {
        let holders = in_process(into).expect("holders");
        let mut spec = spec();
        spec.command = crate::test_support::programs::sleep(60).term_command();
        let held = holders.spawn(spec).expect("the program starts");
        let link = locked(&holders.current).clone().expect("the current link");
        (holders, held, link)
    })
    .await
    .unwrap();
    let _guard = crate::test_support::KillOnDrop(vec![held.pid().0 as i32]);
    let entry = registry::list(&registry_dir)
        .into_iter()
        .next()
        .expect("the holder is registered");

    // A late connection of the same run takes the holder over, then goes away.
    {
        let url = format!(
            "ws://127.0.0.1:{}/daemon?secret={}",
            entry.port, entry.secret
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("connected");
        let hello = ToHolder::Hello {
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_id: crate::holder::build_id().to_string(),
            run_id: Some(link.run_id().to_string()),
            handed_over: Vec::new(),
        };
        socket
            .send(Message::Text(serde_json::to_string(&hello).unwrap()))
            .await
            .unwrap();
        socket.next().await;
    }
    let deadline = Instant::now() + PATIENCE;
    while !(link.connections() == 2 && link.alive()) {
        assert!(Instant::now() < deadline, "the link was not opened again");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::task::spawn_blocking(move || {
        assert_eq!(held.exited_unreaped(), Some(false));
        assert!(
            held.size().is_ok(),
            "the terminal answers over the new link"
        );
        held.end();
        drop(holders);
    })
    .await
    .unwrap();
}

/// A terminal handed over is the next daemon's: its output ends at the mark, after what was to be
/// replayed first, nothing this daemon does with it reaches the holder, it is never read as
/// exited, and a planned parting leaves it running for the next daemon, which finds its tail
/// marked by the handover. A handover that does not name its holder leaves it untouched.
#[test]
fn a_terminal_handed_over_is_left_alone_and_running_for_the_next_daemon() {
    let dir = ScratchDir::new("client-handover");
    let previous = in_process(dir.join("holders")).expect("holders");
    let mut spec = spec();
    spec.command = crate::test_support::programs::sleep(60).term_command();
    let held = previous.spawn(spec).expect("the program starts");
    let pid = held.pid().0 as i32;
    let _guard = crate::test_support::KillOnDrop(vec![pid]);
    let link = locked(&previous.current).clone().expect("the current link");
    held.replay_first(b"replayed".to_vec());
    let reader = held.reader();

    assert!(previous.hand_over("other", &[]).is_empty());
    assert!(!link.is_handed_over(&held.held_at().terminal_id));
    assert_eq!(
        previous.hand_over("tok", &[held.held_at().holder_id]),
        vec![held.held_at()]
    );
    let mut buf = [0u8; 64];
    assert_eq!(reader.read_chunk(&mut buf).unwrap(), Some(8));
    assert_eq!(&buf[..8], b"replayed");
    // Then what the program printed before the mark, and the end of the stream.
    while reader
        .read_chunk(&mut buf)
        .unwrap()
        .expect("the reader was stopped")
        > 0
    {}

    // Requests that would be sent and answered over a link that is there, made before the
    // terminal is hung up, so that only the handover keeps them from the holder.
    assert!(matches!(held.kill(), Sent::Gone));
    assert!(held.size().is_err(), "a request was answered");
    assert!(!held.kill_leftovers(&[]).complete, "a request was answered");
    // Input is refused, while the answers to the queries in the output before the mark still go
    // in: nobody else answers those.
    assert_eq!(held.write_raw(b"x").expect_err("input went in").written, 0);
    held.write_answers(b"\x1b[?6c")
        .expect("the answers to the output before the mark were refused");
    held.hang_up();
    previous.part_planned();
    link.settled();
    assert_eq!(held.exited_unreaped(), Some(false));
    drop(held);
    drop(previous);
    assert!(crate::test_support::is_running(pid));

    let next = in_process(dir.join("holders")).expect("holders");
    let mut found = next.adopt_existing(unresolved()).found;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].last_parting, LastParting::Handover);
    let terminal = found[0].terminals.remove(0);
    assert_eq!(terminal.holder_id, found[0].holder_id);
    assert!(!terminal.exited);
    let told = terminal.handover.clone().expect("its tail was marked");
    assert_eq!((told.token.as_str(), told.truncated), ("tok", false));
    terminal.held.end();
    drop(next);
}

/// The next daemon is handed the history before the mark with the tail, and where the output
/// since the mark begins in it; a terminal no handover marked gives its whole tail.
#[cfg(unix)]
#[test]
fn a_handover_leaves_the_history_before_the_mark_in_the_tail_of_the_next_daemon() {
    let dir = ScratchDir::new("client-handover-history");
    let previous = in_process(dir.join("holders")).expect("holders");
    let mut spec = spec();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf history; exec sleep 60"]);
    spec.command = command;
    let held = previous.spawn(spec).expect("the program starts");
    let _guard = crate::test_support::KillOnDrop(vec![held.pid().0 as i32]);
    let reader = held.reader();
    let mut printed = Vec::new();
    let mut buf = [0u8; 64];
    while !String::from_utf8_lossy(&printed).contains("history") {
        let read = reader
            .read_chunk(&mut buf)
            .unwrap()
            .expect("the reader was stopped");
        printed.extend_from_slice(&buf[..read]);
    }
    assert_eq!(
        previous.hand_over("tok", &[held.held_at().holder_id]),
        vec![held.held_at()]
    );
    previous.part_planned();
    drop(held);
    drop(previous);

    let next = in_process(dir.join("holders")).expect("holders");
    let mut found = next.adopt_existing(unresolved()).found;
    let mut terminal = found[0].terminals.remove(0);
    assert!(String::from_utf8_lossy(&terminal.tail).contains("history"));
    let told = terminal.handover.clone().expect("its tail was marked");
    assert_eq!(told.since as usize, terminal.tail.len());
    assert!(terminal.since_handover().is_empty());
    // Taken as the tail of a terminal no handover marked.
    terminal.handover = None;
    assert_eq!(terminal.since_handover(), terminal.tail.as_slice());
    terminal.held.end();
    drop(next);
}

/// How many times `needle` occurs in `haystack`.
#[cfg(unix)]
fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// A link that drops after a handover and is opened again by the same daemon, only to part over
/// it, leaves the terminal it handed over unlinked: the holder goes on answering the queries it
/// answers while no daemon is linked, once each, the queries it does not answer are answered by
/// nobody before the next daemon, and the next daemon finds the output since the mark starting
/// exactly where it did.
#[cfg(unix)]
#[test]
fn a_link_opened_again_after_a_handover_leaves_the_terminal_it_handed_over_unlinked() {
    let dir = ScratchDir::new("client-handover-relink");
    let previous = in_process(dir.join("holders")).expect("holders");
    let (go, heard) = (dir.join("go"), dir.join("heard"));
    let mut spec = spec();
    let mut command = Command::new("/bin/sh");
    command.args([
        std::ffi::OsStr::new("-c"),
        std::ffi::OsStr::new(
            "stty raw -echo; printf ready; while [ ! -e \"$1\" ]; do sleep 0.02; done; \
             printf '\\033[c\\033[6n'; exec cat > \"$2\"",
        ),
        std::ffi::OsStr::new("sh"),
        go.as_os_str(),
        heard.as_os_str(),
    ]);
    spec.command = command;
    let held = previous.spawn(spec).expect("the program starts");
    let _guard = crate::test_support::KillOnDrop(vec![held.pid().0 as i32]);
    let link = locked(&previous.current).clone().expect("the current link");
    let reader = held.reader();
    let mut printed = Vec::new();
    let mut buf = [0u8; 64];
    while !String::from_utf8_lossy(&printed).contains("ready") {
        let read = reader
            .read_chunk(&mut buf)
            .unwrap()
            .expect("the reader was stopped");
        printed.extend_from_slice(&buf[..read]);
    }
    assert_eq!(
        previous.hand_over("tok", &[held.held_at().holder_id]),
        vec![held.held_at()]
    );
    while reader
        .read_chunk(&mut buf)
        .unwrap()
        .expect("the reader was stopped")
        > 0
    {}

    link.drop_connection();
    let deadline = Instant::now() + PATIENCE;
    while !(link.connections() == 2 && link.alive()) {
        assert!(Instant::now() < deadline, "the link was not opened again");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Answered over the new connection: the holder has dealt with its handshake by then.
    let answered = link.request(Some(PATIENCE), |req| ToHolder::Query {
        id: "nobody".into(),
        req,
        what: crate::holder::protocol::QueryKind::Size,
    });
    assert!(
        matches!(
            answered,
            Some(FromHolder::Answer {
                answer: crate::holder::protocol::Answer::UnknownTerminal,
                ..
            })
        ),
        "{answered:?}"
    );

    // The program asks while the old run is linked again.
    std::fs::write(&go, "").expect("the go file");
    let answer: &[u8] = b"\x1b[?6c";
    let deadline = Instant::now() + PATIENCE;
    while occurrences(&std::fs::read(&heard).unwrap_or_default(), answer) == 0 {
        assert!(
            Instant::now() < deadline,
            "the holder did not answer the device attributes"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    let input = std::fs::read(&heard).expect("what the program heard");
    assert_eq!(occurrences(&input, answer), 1, "{input:?}");
    // The cursor position is left to the next daemon, which replays it from the tail.
    assert!(!input.contains(&b'R'), "{input:?}");

    held.hang_up();
    previous.part_planned();
    drop(held);
    drop(previous);

    let next = in_process(dir.join("holders")).expect("holders");
    let mut found = next.adopt_existing(unresolved()).found;
    let terminal = found[0].terminals.remove(0);
    // What the holder answered, the next daemon's replay of the tail leaves alone.
    assert!(terminal
        .answers_offline
        .contains(&crate::holder::protocol::OfflineAnswer::PrimaryDeviceAttributes));
    let told = terminal.handover.clone().expect("its tail was marked");
    assert_eq!((told.token.as_str(), told.truncated), ("tok", false));
    let (history, since_mark) = terminal.tail.split_at(told.since as usize);
    assert!(String::from_utf8_lossy(history).contains("ready"));
    assert!(
        terminal.since_handover().starts_with(b"\x1b[c\x1b[6n"),
        "{since_mark:?}"
    );
    terminal.held.end();
    drop(next);
}

/// A holder whose welcome does not say it carries out a handover, from a build before it, is
/// never sent one, and hands nothing over.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_that_does_not_carry_out_a_handover_is_not_sent_one() {
    let dir = ScratchDir::new("client-no-handover");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = received.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("the daemon connects");
        let mut socket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("a websocket");
        socket.next().await;
        socket
            .send(Message::Text(welcome_text()))
            .await
            .expect("the welcome");
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            seen.lock().unwrap().push(text);
        }
    });
    let handed = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.link().expect("linked");
        let handed = holders.hand_over("tok", &[link.welcome().holder_id.clone()]);
        assert!(link.alive());
        drop(holders);
        handed
    })
    .await
    .unwrap();
    assert!(handed.is_empty());
    assert!(
        !received
            .lock()
            .unwrap()
            .iter()
            .any(|text| text.contains(r#""type":"handover""#)),
        "{received:?}"
    );
}

/// An ordinary stop that follows a planned parting ends the holders parted from, by their
/// verified process, a holder that could not be linked to among them, and forgets them, so that a
/// second call ends none.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_stop_after_a_planned_parting_ends_the_holders_parted_from() {
    let dir = ScratchDir::new("client-planned-then-ordinary");
    // A port nothing listens on.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        listener.local_addr().unwrap().port()
    };
    let stand_in = StandIn::start();
    let pid = stand_in.pid as i32;
    let entry = stand_in.entry("unlinkable", port);
    registry::write(&dir.join("holders"), &entry).unwrap();
    let holders = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let adoption = holders.adopt_existing(unresolved());
        assert_eq!(adoption.pending, vec!["unlinkable".to_string()]);
        holders.part_planned();
        assert!(holders.left_to_successor().contains("unlinkable"));
        holders.end_handed_over();
        assert!(holders.left_to_successor().is_empty());
        // Nothing is left to end.
        holders.end_handed_over();
        (holders, dir)
    })
    .await
    .unwrap();
    assert!(
        !crate::test_support::is_running(pid),
        "the holder parted from was left running"
    );
    stand_in.reaper.join().ok();
    tokio::task::spawn_blocking(move || drop(holders))
        .await
        .unwrap();
}

/// A planned parting ends no holder: one that runs but cannot be linked to stops being tried and
/// is left running, with its registry file, and the ordinary stop that follows finds nothing to
/// end.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_planned_parting_leaves_a_holder_that_cannot_be_linked_to_running() {
    let dir = ScratchDir::new("client-planned-pending");
    // A port nothing listens on.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        listener.local_addr().unwrap().port()
    };
    let stand_in = StandIn::start();
    let entry = stand_in.entry("unlinkable", port);
    let registry_dir = dir.join("holders");
    registry::write(&registry_dir, &entry).unwrap();
    let dir = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let adoption = holders.adopt_existing(unresolved());
        assert_eq!(adoption.pending, vec!["unlinkable".to_string()]);
        holders.part_planned();
        // What the daemon's ordinary stop does next.
        drop(holders);
        dir
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        crate::test_support::is_running(stand_in.pid as i32),
        "the holder was ended"
    );
    assert!(registry::read(&registry_dir, "unlinkable").is_some());
    stand_in.end();
    drop(dir);
}

/// The welcome of a holder that runs companions.
fn welcome_text_running_companions() -> String {
    let mut welcome: serde_json::Value = serde_json::from_str(&welcome_text()).unwrap();
    welcome["companions"] = serde_json::Value::Bool(true);
    welcome.to_string()
}

/// A holder that welcomes the daemon with `welcome` and answers a launch's key, a companion's
/// start and a spawn. With `hangs_up_on_spawn`, once it has answered the spawn it removes its
/// registry files from `registry_dir`, when given, and hangs up, as a holder that ends on its own
/// does (one that leaves its files is a holder that cannot be reached for now). Returns the port
/// it listens on and every frame it received.
async fn launch_holder(
    welcome: String,
    registry_dir: Option<PathBuf>,
    hangs_up_on_spawn: bool,
) -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = received.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("the daemon connects");
        let mut socket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("a websocket");
        socket.next().await;
        socket
            .send(Message::Text(welcome))
            .await
            .expect("the welcome");
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            seen.lock().unwrap().push(text.clone());
            let (reply, vanishes) = match serde_json::from_str::<ToHolder>(&text) {
                Ok(ToHolder::IssueLaunch { req, .. }) => (
                    Some(FromHolder::LaunchIssued {
                        req,
                        key: Some("key".to_string()),
                    }),
                    false,
                ),
                Ok(ToHolder::StartCompanion { req, .. }) => (
                    Some(FromHolder::CompanionStarted {
                        req,
                        pid: Some(4242),
                        identity: None,
                        port: None,
                        error: None,
                    }),
                    false,
                ),
                Ok(ToHolder::Spawn(spec)) => (
                    Some(FromHolder::Spawned {
                        id: spec.id,
                        pid: 4343,
                        identity: None,
                    }),
                    hangs_up_on_spawn,
                ),
                _ => (None, false),
            };
            if let Some(reply) = reply {
                socket
                    .send(Message::Text(serde_json::to_string(&reply).unwrap()))
                    .await
                    .ok();
            }
            if vanishes {
                if let Some(dir) = &registry_dir {
                    for entry in registry::list(dir) {
                        registry::remove(dir, &entry.holder_id);
                    }
                }
                socket.close(None).await.ok();
                return;
            }
        }
    });
    (port, received)
}

/// A holder whose welcome does not advertise companions, as those of builds that predate them do,
/// is sent no frame of theirs: the launch has no companion to start, and its requests for one fail
/// on the daemon's side.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_in_a_holder_that_runs_no_companions_is_told_so_and_sends_none() {
    let dir = ScratchDir::new("client-no-companions");
    let (port, received) = launch_holder(welcome_text(), None, false).await;
    tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let issued = holders
            .issue_launch("s", None)
            .expect("the holder issues the launch's credentials");
        assert!(!issued.companions);
        let error = issued
            .open_endpoint(EndpointKind::Loopback)
            .expect_err("no endpoint is asked of a holder that runs no companions");
        assert!(format!("{error:#}").contains("no companions"), "{error:#}");
        issued
            .start_companion(
                Command::new("/bin/true"),
                Ready::SocketExists,
                Duration::from_secs(1),
                None,
            )
            .err()
            .expect("no companion is started in such a holder");
        issued
            .end_companion()
            .expect_err("no companion is ended in such a holder");
        // The revoke is the last frame the launch sends, so all it sent before has arrived once it
        // has.
        drop(issued);
        let deadline = Instant::now() + PATIENCE;
        while !received
            .lock()
            .unwrap()
            .iter()
            .any(|text| text.contains(r#""type":"revoke_launch""#))
        {
            assert!(Instant::now() < deadline, "the revoke never arrived");
            std::thread::sleep(Duration::from_millis(20));
        }
        let texts = received.lock().unwrap().clone();
        for kind in ["open_endpoint", "start_companion", "end_companion"] {
            assert!(
                !texts
                    .iter()
                    .any(|text| text.contains(&format!(r#""type":"{kind}""#))),
                "a {kind} frame was sent: {texts:?}"
            );
        }
        drop(holders);
    })
    .await
    .unwrap();
}

/// A started companion in a holder `launch_holder` scripts, bound by a terminal spawned for it.
fn bound_companion(link: &Arc<Link>) -> (HeldCompanion, HeldTerminal) {
    let (companion, started_port) = link
        .start_companion(
            "t1",
            Command::new("/bin/true"),
            Ready::SocketExists,
            Duration::from_secs(5),
            None,
        )
        .expect("the holder starts the companion");
    assert_eq!(started_port, None);
    let terminal = link
        .spawn(SpawnSpec {
            companion: true,
            ..spec()
        })
        .expect("the holder starts the terminal");
    (companion, terminal)
}

/// Waits until `done`, which is told what the test is waiting for if it never comes.
fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A holder gone for good takes the companion its terminal bound with it: here the holder ends on
/// its own, leaving no registry file, once the terminal is started.
#[tokio::test(flavor = "multi_thread")]
async fn a_bound_companion_reads_as_exited_once_the_link_to_its_holder_is_lost_for_good() {
    let dir = ScratchDir::new("client-companion-lost");
    let registry_dir = dir.join("holders");
    let (port, _) =
        launch_holder(welcome_text_running_companions(), Some(registry_dir), true).await;
    tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.link().expect("linked");
        let (companion, terminal) = bound_companion(&link);
        wait_for(
            "the holder was lost and the companion still reads as running",
            || companion.exited(),
        );
        drop((terminal, companion, link, holders));
    })
    .await
    .unwrap();
}

/// A link that is only pending leaves the companion its terminal bound as it was, and it reads as
/// exited once the holder is gone: the holder's process runs meanwhile, with its registry file.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_bound_companion_stays_while_the_link_is_pending_and_exits_once_the_holder_is_gone() {
    let dir = ScratchDir::new("client-companion-pending");
    let (port, _) = launch_holder(welcome_text_running_companions(), None, true).await;
    let stand_in = StandIn::start();
    let entry = stand_in.entry("dropped", port);
    registry::write(&dir.join("holders"), &entry).unwrap();
    let (holders, link, companion, terminal, dir) = tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.connect(&entry).expect("linked");
        let (companion, terminal) = bound_companion(&link);
        (holders, link, companion, terminal, dir)
    })
    .await
    .unwrap();
    let deadline = Instant::now() + PATIENCE;
    while !link.pending() {
        assert!(Instant::now() < deadline, "the link never became pending");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!companion.exited(), "a pending link made it read as exited");

    stand_in.end();
    while link.pending() {
        assert!(Instant::now() < deadline, "the link is still pending");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(companion.exited());
    tokio::task::spawn_blocking(move || drop((terminal, companion, link, holders)))
        .await
        .unwrap();
    drop(dir);
}

/// A planned parting leaves the companions as they were: they are the next daemon's.
#[tokio::test(flavor = "multi_thread")]
async fn a_planned_parting_leaves_a_bound_companion_not_exited() {
    let dir = ScratchDir::new("client-companion-parting");
    let (port, _) = launch_holder(welcome_text_running_companions(), None, false).await;
    tokio::task::spawn_blocking(move || {
        let holders = holders_over(&dir, port);
        let link = holders.link().expect("linked");
        let (companion, terminal) = bound_companion(&link);
        link.part(true);
        wait_for("the link stayed alive after the parting", || !link.alive());
        // The link is recovering until what the end of the connection does is done.
        link.settled();
        assert!(
            !companion.exited(),
            "a planned parting made the companion read as exited"
        );
        drop((terminal, companion, link, holders));
    })
    .await
    .unwrap();
}
