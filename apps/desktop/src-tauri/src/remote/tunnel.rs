//! The remote window's way to its daemon: a loopback port the application listens on, assigned by
//! the system and held from before the window opens until it closes, each connection it accepts
//! carried by one run of `octoboardd proxy` over the SSH connection.
//!
//! The port is never let go while the window lives, reconnections included: another user of this
//! computer could otherwise take it and be sent the owner key. When the SSH connection has dropped,
//! it is made again in the background after growing pauses, for as long as the link is held, and by
//! the window's own connections, which are not held back by those pauses: the window spaces its
//! attempts itself and its Try again attempts at once. A failure that would meet the same answer
//! again (a refused password or key, another host key, an unreadable key file) is tried again only
//! after a long pause, whoever asks, unless the window's Try again lifts it. The owner key is read
//! again over a new SSH connection, and whenever the window has no connection open (rotating the key
//! closes every connection it opened), and the window is pointed at the new key only where it
//! changed. See "A dropped connection" in `docs/product/remote-daemons.md`.

use std::mem::Discriminant;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use super::octoboardd::{Address, Remote};
use super::ssh::{self, ConnectError, Session, Target};
use crate::diagnostics::diagnostic;

/// The longest pause between two failed reconnection attempts.
const MAX_RETRY_PAUSE: Duration = Duration::from_secs(30);

/// The pause after a failure that would meet the same answer again (`ConnectError::is_refusal`):
/// long, so that a password the host refuses is not sent again and again, which a host may answer
/// by banning this computer.
const REFUSED_PAUSE: Duration = Duration::from_secs(5 * 60);

/// How often a reconnection that keeps failing in the same way is written to the diagnostics log.
const LOG_EVERY: Duration = Duration::from_secs(10 * 60);

/// How long the pause after `failures` failed attempts in a row is: 1 s, 2 s, 4 s, … up to
/// `MAX_RETRY_PAUSE`.
pub fn retry_pause(failures: u32) -> Duration {
    let seconds = 1u64 << failures.saturating_sub(1).min(5);
    Duration::from_secs(seconds).min(MAX_RETRY_PAUSE)
}

#[derive(Default)]
struct Slot {
    session: Option<Arc<Session>>,
    failures: u32,
    retry_at: Option<Instant>,
    /// When the last attempt failed, while none has succeeded since.
    failed_at: Option<Instant>,
    /// Whether the last attempt failed in a way that would meet the same answer again
    /// (`REFUSED_PAUSE`).
    refused: bool,
    /// The kind of failure last written to the diagnostics log since the last success, and when.
    logged: Option<(Discriminant<ConnectError>, Instant)>,
}

/// Whether a failed attempt of `kind` at `now` is written to the diagnostics log, `last` being
/// the one last written since the last success: the first, one of another kind, and otherwise one
/// every `LOG_EVERY`.
fn worth_logging(
    last: Option<(Discriminant<ConnectError>, Instant)>,
    kind: Discriminant<ConnectError>,
    now: Instant,
) -> bool {
    last.is_none_or(|(logged, at)| logged != kind || now.duration_since(at) >= LOG_EVERY)
}

/// What holds back an attempt to make the SSH connection again.
#[derive(Clone, Copy)]
enum Pace {
    /// The pause after a failed attempt: a connection asked for meanwhile is not made.
    Pause,
    /// The pause after a failure that would meet the same answer again, and an attempt that failed
    /// after `asked`, while the connection asked for then waited for the slot: it takes that
    /// failure rather than making the same attempt again. One of the window's connections, which
    /// the window spaces itself.
    Asked(Instant),
}

/// One remote window's connection to its daemon, kept for as long as the window lives.
pub struct Link {
    target: Target,
    /// The `octoboardd` the connection found, and the shell of the host it runs on, which the
    /// reconnections keep (they are made to the same host).
    remote: Remote,
    slot: tokio::sync::Mutex<Slot>,
    /// A copy of the slot's session for `current`, which must not wait on the slot: updated wherever the slot's
    /// session changes.
    up: Mutex<Option<Arc<Session>>>,
    key: Mutex<String>,
    /// How many of the window's connections are being carried.
    open: watch::Sender<usize>,
    reread: AtomicBool,
    on_key_changed: Box<dyn Fn(String) + Send + Sync>,
    /// Whether the SSH connection is up, for the application to show.
    live: watch::Sender<bool>,
    /// Set by `close`: no connection is made again after it, by a carry that was under way then.
    closed: AtomicBool,
    /// Whether `keep_up` has an attempt under way.
    keeping_up: AtomicBool,
}

impl Link {
    /// A link over `session`, made to `target` with `octoboardd` as `remote`, whose daemon accepted
    /// `key`; `on_key_changed` points the window at a key read later.
    pub fn new(
        target: Target,
        remote: Remote,
        session: Session,
        key: String,
        on_key_changed: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        let session = Arc::new(session);
        Self {
            target,
            remote,
            up: Mutex::new(Some(session.clone())),
            slot: tokio::sync::Mutex::new(Slot {
                session: Some(session),
                ..Slot::default()
            }),
            key: Mutex::new(key),
            open: watch::Sender::new(0),
            reread: AtomicBool::new(false),
            on_key_changed: Box::new(on_key_changed),
            live: watch::Sender::new(true),
            closed: AtomicBool::new(false),
            keeping_up: AtomicBool::new(false),
        }
    }

    /// Whether the host runs Windows, whose SSH server (Win32-OpenSSH) confirms a `direct-tcpip` channel to a port
    /// nothing listens on and then closes it (`ssh::WINDOWS_EARLY_CLOSE`).
    pub(super) fn host_is_windows(&self) -> bool {
        self.remote.shell.is_windows()
    }

    /// Follows whether the SSH connection is up: it is while the window's connections can be
    /// carried, and is not from when it is seen dropped until it is made again.
    pub fn live(&self) -> watch::Receiver<bool> {
        self.live.subscribe()
    }

    /// Notes whether the SSH connection is up, unless a reconnection is under way (which notes it
    /// itself once it is over).
    fn note_live(&self) {
        if let Ok(slot) = self.slot.try_lock() {
            let live = slot
                .session
                .as_ref()
                .is_some_and(|session| !session.is_closed());
            self.live
                .send_if_modified(|was| std::mem::replace(was, live) != live);
        }
    }

    /// The SSH connection, made again when it has dropped and no pause after a failed attempt is
    /// running; `None` when there is none to carry a connection on now, and always once the link
    /// is closed.
    pub(super) async fn session(&self) -> Option<Arc<Session>> {
        self.reach(Pace::Pause).await
    }

    /// The SSH connection, made again when it has dropped unless `pace` holds the attempt back;
    /// `None` when there is none to carry a connection on now, and always once the link is closed.
    async fn reach(&self, pace: Pace) -> Option<Arc<Session>> {
        let mut slot = self.slot.lock().await;
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }
        if let Some(session) = &slot.session {
            if !session.is_closed() {
                return Some(session.clone());
            }
            slot.session = None;
            self.set_up(None);
            self.live
                .send_if_modified(|was| std::mem::replace(was, false));
        }
        let pausing = slot.retry_at.is_some_and(|at| Instant::now() < at);
        let held_back = match pace {
            Pace::Pause => pausing,
            Pace::Asked(asked) => {
                (slot.refused && pausing) || slot.failed_at.is_some_and(|at| at >= asked)
            }
        };
        if held_back {
            return None;
        }
        match ssh::connect(&self.target).await {
            Ok(session) => {
                let session = Arc::new(session);
                slot.session = Some(session.clone());
                self.set_up(Some(session.clone()));
                slot.failures = 0;
                slot.retry_at = None;
                slot.failed_at = None;
                slot.refused = false;
                slot.logged = None;
                self.reread.store(true, Ordering::SeqCst);
                self.live
                    .send_if_modified(|was| !std::mem::replace(was, true));
                Some(session)
            }
            Err(err) => {
                let now = Instant::now();
                slot.failures += 1;
                slot.failed_at = Some(now);
                slot.refused = err.is_refusal();
                let pause = if slot.refused {
                    REFUSED_PAUSE
                } else {
                    retry_pause(slot.failures)
                };
                slot.retry_at = Some(now + pause);
                let kind = std::mem::discriminant(&err);
                if worth_logging(slot.logged, kind, now) {
                    slot.logged = Some((kind, now));
                    diagnostic!(
                        "cannot reconnect to {}@{}:{} (attempt {}): {err:?}",
                        self.target.user,
                        self.target.host,
                        self.target.port,
                        slot.failures
                    );
                }
                None
            }
        }
    }

    /// Lets the next attempt be made at once, whatever pause a failed one left: the window's Try
    /// again. Waits for an attempt under way to end first.
    pub(super) async fn retry_now(&self) {
        let mut slot = self.slot.lock().await;
        slot.retry_at = None;
        slot.refused = false;
    }

    /// Makes the SSH connection again in the background where it has dropped and no pause holds it
    /// back, unless such an attempt is under way already.
    fn keep_up(self: &Arc<Self>) {
        if self.closed.load(Ordering::SeqCst) || self.current().is_some() {
            return;
        }
        if self.keeping_up.swap(true, Ordering::SeqCst) {
            return;
        }
        let link = self.clone();
        tokio::spawn(async move {
            let _ = link.reach(Pace::Pause).await;
            link.keeping_up.store(false, Ordering::SeqCst);
        });
    }

    fn set_up(&self, session: Option<Arc<Session>>) {
        *self.up.lock().unwrap_or_else(|e| e.into_inner()) = session;
    }

    /// The SSH connection while it is up, without making it again and without waiting on a reconnection that
    /// holds the slot: `None` when it has dropped, while it is being made again and once the link is closed.
    pub(super) fn current(&self) -> Option<Arc<Session>> {
        self.up
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|session| !session.is_closed())
    }

    /// Reads the owner key again, and points the window at it where it changed.
    async fn refresh_key(&self, session: &Session) {
        let output = match session.run(&self.remote.address_command()).await {
            Ok(output) => output,
            Err(err) => {
                diagnostic!("cannot read the remote daemon's key again: {err}");
                return;
            }
        };
        match self
            .remote
            .read_address(output.status, &output.stdout, &output.stderr)
        {
            Address::Answer(answer) => {
                let changed = {
                    let mut key = self.key.lock().unwrap_or_else(|e| e.into_inner());
                    let changed = *key != answer.owner_key;
                    key.clone_from(&answer.owner_key);
                    changed
                };
                if changed {
                    (self.on_key_changed)(answer.owner_key);
                }
            }
            other => diagnostic!("the remote daemon's key was not read again: {other:?}"),
        }
    }

    /// Carries `stream`, one of the window's connections, to the daemon's owner endpoint until
    /// either side closes it.
    async fn carry(&self, mut stream: TcpStream) {
        let Some(session) = self.reach(Pace::Asked(Instant::now())).await else {
            return;
        };
        if self.reread.swap(false, Ordering::SeqCst) {
            self.refresh_key(&session).await;
        }
        // Bounded (`ssh::COMMAND_TIMEOUT`): a host that stopped answering ends the window's
        // connection, which it retries, rather than holding it.
        let channel = match session.open(&self.remote.proxy_command()).await {
            Ok(channel) => channel,
            Err(err) => {
                diagnostic!("cannot start the remote proxy: {err}");
                return;
            }
        };
        let _open = Open::new(self);
        let mut remote = channel.into_stream();
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut remote).await;
    }

    /// Ends the SSH connection, which ends every proxy still running over it, for good: no carry
    /// makes it again.
    pub async fn close(&self) {
        let session = {
            let mut slot = self.slot.lock().await;
            self.closed.store(true, Ordering::SeqCst);
            self.set_up(None);
            slot.session.take()
        };
        if let Some(session) = session {
            session.disconnect().await;
        }
    }
}

/// Counts a connection the window has open; once none is, the key is read again at the next.
struct Open<'a>(&'a Link);

impl<'a> Open<'a> {
    fn new(link: &'a Link) -> Self {
        link.open.send_modify(|open| *open += 1);
        Self(link)
    }
}

impl Drop for Open<'_> {
    fn drop(&mut self) {
        // The key is marked to be read again before the count says none is open, so whoever waits
        // for that finds it marked.
        if *self.0.open.borrow() == 1 {
            self.0.reread.store(true, Ordering::SeqCst);
        }
        self.0.open.send_modify(|open| *open -= 1);
    }
}

/// Binds the loopback port the window is opened on.
pub async fn bind() -> std::io::Result<TcpListener> {
    TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await
}

/// How often whether the SSH connection is up is looked at between the window's connections, and
/// made again where it is not.
const LIVE_CHECK: Duration = Duration::from_secs(1);

/// Accepts the window's connections on `listener` and carries each over `link`, keeping the SSH
/// connection up meanwhile, until `stop` is set; the port is let go then, and the SSH connection
/// ended.
pub async fn serve(listener: TcpListener, link: Arc<Link>, stop: watch::Receiver<bool>) {
    serve_checking(listener, link, stop, LIVE_CHECK).await;
}

/// `serve`, looking at the SSH connection every `every`.
async fn serve_checking(
    listener: TcpListener,
    link: Arc<Link>,
    mut stop: watch::Receiver<bool>,
    every: Duration,
) {
    let mut check = tokio::time::interval(every);
    loop {
        tokio::select! {
            _ = check.tick() => {
                link.note_live();
                link.keep_up();
            }
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let link = link.clone();
                    tokio::spawn(async move { link.carry(stream).await });
                }
                Err(err) => {
                    diagnostic!("the remote window's port failed to accept: {err}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            _ = stop.changed() => break,
        }
    }
    drop(listener);
    link.close().await;
}

#[cfg(test)]
mod tests {
    use super::super::octoboardd::Shell;
    use super::super::ssh::test_server::{answer, Behaviour, TestServer};
    use super::super::ssh::Secret;
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn target(server: &TestServer) -> Target {
        Target {
            host: "127.0.0.1".into(),
            port: server.port,
            user: "owner".into(),
            secret: Secret::Password("right".into()),
            pinned: Some(server.fingerprint.clone()),
        }
    }

    struct Running {
        port: u16,
        keys: Arc<Mutex<Vec<String>>>,
        stop: watch::Sender<bool>,
        link: Arc<Link>,
        serving: tokio::task::JoinHandle<()>,
    }

    /// A link over a connection to `server`, served without looking at the SSH connection between
    /// the window's connections, so that nothing but the test makes it again.
    async fn run(server: &TestServer) -> Running {
        run_with(server, target(server), Duration::from_secs(3600)).await
    }

    /// A link over a connection to `server` that makes it again with `target`, served looking at
    /// the SSH connection every `every`.
    async fn run_with(server: &TestServer, target: Target, every: Duration) -> Running {
        let session = ssh::connect(&self::target(server)).await.unwrap();
        let keys = Arc::new(Mutex::new(Vec::new()));
        let seen = keys.clone();
        let link = Arc::new(Link::new(
            target,
            Remote::new(Shell::Posix, "octoboardd"),
            session,
            "aa".into(),
            move |key| seen.lock().unwrap().push(key),
        ));
        let listener = bind().await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop, stopped) = watch::channel(false);
        let serving = tokio::spawn(serve_checking(listener, link.clone(), stopped, every));
        Running {
            port,
            keys,
            stop,
            link,
            serving,
        }
    }

    async fn echo(port: u16, text: &[u8]) -> Vec<u8> {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(text).await.unwrap();
        // The proxy's half-close: the bytes sent come back, then the end.
        stream.shutdown().await.unwrap();
        let mut back = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut back))
            .await
            .expect("the echo came back")
            .unwrap();
        back
    }

    /// Waits until none of the window's connections is being carried.
    async fn idle(link: &Link) {
        let _ = link.open.subscribe().wait_for(|open| *open == 0).await;
    }

    /// Drops the host's side of every connection and waits until the link's SSH connection has
    /// ended.
    async fn dropped(running: &Running, server: &TestServer) {
        let session = running.link.slot.lock().await.session.clone().unwrap();
        server.drop_connections();
        session.closed().await;
    }

    #[tokio::test]
    async fn each_connection_is_carried_by_a_proxy_run() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        assert_eq!(echo(running.port, b"hello").await, b"hello");
        assert_eq!(echo(running.port, b"again").await, b"again");
        let proxies = server
            .commands()
            .iter()
            .filter(|command| *command == "'octoboardd' proxy")
            .count();
        assert_eq!(proxies, 2);
        assert_eq!(
            server.connections(),
            1,
            "one SSH connection carries them all"
        );
    }

    #[tokio::test]
    async fn a_dropped_connection_is_made_again_behind_the_same_port() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        assert_eq!(echo(running.port, b"one").await, b"one");
        let mut live = running.link.live();
        assert!(*live.borrow_and_update());
        dropped(&running, &server).await;
        // Seen dropped at the next look the serving loop takes.
        running.link.note_live();
        assert!(!*live.borrow_and_update());
        assert_eq!(echo(running.port, b"two").await, b"two");
        assert!(*live.borrow());
        assert_eq!(server.connections(), 2);
        // The key, read again over the new connection, did not change: the window is left alone.
        assert!(running.keys.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_port_stays_held_while_the_host_cannot_be_reached() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        let fingerprint = server.fingerprint.clone();
        drop(server);
        // Nothing listens at the host's port any more: the window's connection is closed at once.
        let mut stream = TcpStream::connect(("127.0.0.1", running.port))
            .await
            .unwrap();
        let mut back = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut back)).await;
        assert!(back.is_empty());
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", running.port)).is_err(),
            "the port is still the application's"
        );
        assert!(!fingerprint.is_empty());
    }

    #[tokio::test]
    async fn a_key_rotated_while_disconnected_is_followed() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        dropped(&running, &server).await;
        server.set_address(Some(answer("bb")));
        assert_eq!(echo(running.port, b"x").await, b"x");
        assert_eq!(*running.keys.lock().unwrap(), vec!["bb".to_owned()]);
    }

    #[tokio::test]
    async fn a_key_rotated_while_connected_is_followed_once_the_window_reconnects() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        assert_eq!(echo(running.port, b"x").await, b"x");
        // Rotating the key closes every connection the window had open; its next one reads the
        // key again, before it is carried.
        idle(&running.link).await;
        server.set_address(Some(answer("cc")));
        assert_eq!(echo(running.port, b"y").await, b"y");
        assert_eq!(*running.keys.lock().unwrap(), vec!["cc".to_owned()]);
        assert_eq!(server.connections(), 1);
    }

    #[tokio::test]
    async fn stopping_lets_the_port_go_and_ends_the_connection() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        running.stop.send(true).unwrap();
        // The serving loop lets the port go and ends the connection before it returns.
        running.serving.await.unwrap();
        assert!(std::net::TcpListener::bind(("127.0.0.1", running.port)).is_ok());
        assert!(running.link.slot.lock().await.session.is_none());
        // Nothing was asked of the daemon beyond carrying connections: no stop, no shutdown.
        assert!(server
            .commands()
            .iter()
            .all(|command| command.ends_with(" proxy") || command.ends_with(" address --json")));
    }

    #[tokio::test]
    async fn the_current_connection_is_told_while_the_slot_is_busy_and_not_once_it_is_gone() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        // A reconnection or a carry holding the slot does not make a live connection look down.
        let _busy = running.link.slot.lock().await;
        assert!(running.link.current().is_some());
        drop(_busy);
        running.link.close().await;
        assert!(running.link.current().is_none());
    }

    #[tokio::test]
    async fn a_closed_link_makes_no_connection_again() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        running.link.close().await;
        // A carry that comes after the close, as one under way when the window closed does, finds
        // no connection and makes none.
        assert!(running.link.session().await.is_none());
        assert_eq!(server.connections(), 1);
        assert!(running.link.slot.lock().await.session.is_none());
    }

    /// Notes a failed attempt at `at`, as `reach` does, its pause still running.
    async fn failed(link: &Link, at: Instant) {
        let mut slot = link.slot.lock().await;
        slot.failures = 6;
        slot.failed_at = Some(at);
        slot.retry_at = Some(at + retry_pause(6));
    }

    #[tokio::test]
    async fn a_window_connection_reaches_the_host_in_the_pause_a_failed_attempt_left() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run(&server).await;
        dropped(&running, &server).await;
        failed(&running.link, Instant::now() - Duration::from_millis(1)).await;
        // A forward waits out the pause.
        assert!(running.link.session().await.is_none());
        assert_eq!(server.connections(), 1);
        // The window spaces its attempts itself, and its Try again attempts at once: the host,
        // back meanwhile, is reached.
        assert_eq!(echo(running.port, b"back").await, b"back");
        assert_eq!(server.connections(), 2);
        assert!(*running.link.live().borrow());
    }

    #[tokio::test]
    async fn window_connections_made_while_an_attempt_fails_take_its_failure_and_make_no_other() {
        let server = TestServer::start(Behaviour::default()).await;
        // A host that takes the connection and drops it a second later without a word, counting
        // the attempts made to it.
        let host = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let host_port = host.local_addr().unwrap().port();
        let attempts = Arc::new(Mutex::new(0usize));
        let counted = attempts.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = host.accept().await {
                *counted.lock().unwrap() += 1;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    drop(stream);
                });
            }
        });
        let silent = Target {
            port: host_port,
            ..target(&server)
        };
        let running = run_with(&server, silent, Duration::from_secs(3600)).await;
        dropped(&running, &server).await;
        let mut streams = Vec::new();
        for _ in 0..4 {
            streams.push(
                TcpStream::connect(("127.0.0.1", running.port))
                    .await
                    .unwrap(),
            );
        }
        for mut stream in streams {
            let mut back = Vec::new();
            tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut back))
                .await
                .expect("the connection was closed")
                .unwrap();
            assert!(back.is_empty());
        }
        assert_eq!(*attempts.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn a_refused_reconnection_is_asked_again_only_after_the_long_pause_or_a_try_again() {
        let server = TestServer::start(Behaviour::default()).await;
        let refused = Target {
            secret: Secret::Password("wrong".into()),
            ..target(&server)
        };
        let running = run_with(&server, refused, Duration::from_secs(3600)).await;
        dropped(&running, &server).await;
        assert!(running
            .link
            .reach(Pace::Asked(Instant::now()))
            .await
            .is_none());
        assert_eq!(server.connections(), 2);
        {
            let slot = running.link.slot.lock().await;
            assert!(slot.refused);
            let pause = slot.retry_at.unwrap() - slot.failed_at.unwrap();
            assert_eq!(pause, REFUSED_PAUSE);
        }
        // Neither the window's connections nor the others ask again meanwhile.
        assert!(running
            .link
            .reach(Pace::Asked(Instant::now()))
            .await
            .is_none());
        assert!(running.link.session().await.is_none());
        assert_eq!(server.connections(), 2);
        // The window's Try again does.
        running.link.retry_now().await;
        assert!(running
            .link
            .reach(Pace::Asked(Instant::now()))
            .await
            .is_none());
        assert_eq!(server.connections(), 3);
    }

    #[tokio::test]
    async fn an_unreachable_host_is_asked_again_after_the_growing_pause() {
        let server = TestServer::start(Behaviour::default()).await;
        let closed = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let nowhere = Target {
            port: closed.local_addr().unwrap().port(),
            ..target(&server)
        };
        drop(closed);
        let running = run_with(&server, nowhere, Duration::from_secs(3600)).await;
        dropped(&running, &server).await;
        assert!(running
            .link
            .reach(Pace::Asked(Instant::now()))
            .await
            .is_none());
        let slot = running.link.slot.lock().await;
        assert!(!slot.refused);
        assert_eq!(
            slot.retry_at.unwrap() - slot.failed_at.unwrap(),
            retry_pause(1)
        );
    }

    #[tokio::test]
    async fn the_link_makes_a_dropped_connection_again_by_itself() {
        let server = TestServer::start(Behaviour::default()).await;
        let running = run_with(&server, target(&server), Duration::from_millis(50)).await;
        let mut live = running.link.live();
        dropped(&running, &server).await;
        // Made again with no connection of the window asking for it, and then told up.
        tokio::time::timeout(Duration::from_secs(10), async {
            while server.connections() < 2 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            live.wait_for(|live| *live).await.unwrap();
        })
        .await
        .expect("the connection was made again");
        assert_eq!(server.connections(), 2);
    }

    #[test]
    fn a_failure_is_logged_first_when_its_kind_changes_and_every_ten_minutes_otherwise() {
        let unreachable = std::mem::discriminant(&ConnectError::Unreachable(String::new()));
        let refused = std::mem::discriminant(&ConnectError::AuthFailed(String::new()));
        let then = Instant::now();
        assert!(worth_logging(None, unreachable, then));
        let last = Some((unreachable, then));
        assert!(!worth_logging(
            last,
            unreachable,
            then + Duration::from_secs(60)
        ));
        assert!(worth_logging(last, refused, then + Duration::from_secs(60)));
        assert!(worth_logging(last, unreachable, then + LOG_EVERY));
    }

    #[test]
    fn the_pause_between_failed_attempts_grows_to_half_a_minute() {
        let pauses: Vec<u64> = (1..=8).map(|n| retry_pause(n).as_secs()).collect();
        assert_eq!(pauses, vec![1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[tokio::test]
    async fn the_window_s_port_is_on_loopback_and_assigned_by_the_system() {
        let first = bind().await.unwrap();
        let second = bind().await.unwrap();
        for listener in [&first, &second] {
            let address = listener.local_addr().unwrap();
            assert!(address.ip().is_loopback(), "{address}");
            assert_ne!(address.port(), 0);
        }
        assert_ne!(
            first.local_addr().unwrap().port(),
            second.local_addr().unwrap().port()
        );
    }

    #[test]
    fn the_pause_is_never_nothing_and_never_beyond_half_a_minute() {
        assert_eq!(retry_pause(0), Duration::from_secs(1));
        assert_eq!(retry_pause(1), Duration::from_secs(1));
        assert_eq!(retry_pause(u32::MAX), Duration::from_secs(30));
    }
}
