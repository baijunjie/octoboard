//! The running daemon's owner endpoint, for a program on the daemon's machine running as the
//! daemon's user — one the owner's application runs over SSH, say: `proxy`, which carries bytes
//! between its standard input and output and the endpoint, and `address --json`, which tells such a
//! program the owner key, the protocol generation and the release version of the daemon it reaches
//! there.
//!
//! The endpoint is the browser socket in the data directory ([`crate::browser::socket`]) on Linux
//! and macOS, and on Windows the owner pipe ([`crate::platform::owner_pipe`]), which a daemon has
//! when it accepts the owner key. A pipe has no half-close, so on Windows `proxy` cannot tell the
//! daemon that its input has ended while still taking the answer: once its input ends, it carries
//! what the daemon still sends until the daemon has been silent for [`LINGER`]. Neither mode takes
//! a key on its command line: `proxy` reads no key at all, the client sending it in its own
//! requests as a browser does, and `address --json` reads it from `owner.key` and prints it on its
//! standard output alone.

use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::owner_record::Owner;
use crate::platform::local_socket::{self, BlockingStream};
use crate::platform::owner_pipe;

use super::background::{not_running, NOT_RUNNING, RESTART_WAIT};
use super::succession::{find_daemon, Found};

/// The exit status when the running daemon has no owner endpoint.
pub(super) const NO_OWNER_ENDPOINT: i32 = 4;

/// The exit status when the running daemon's owner endpoint cannot be reached.
pub(super) const ENDPOINT_UNREACHABLE: i32 = 5;

/// The exit status when no daemon runs and this `octoboardd` is the desktop application's own
/// copy, whose daemon runs only while the application does: there is nothing to start but the
/// application.
pub(super) const APPLICATION_NOT_RUNNING: i32 = 6;

/// How long, once `proxy`'s input has ended on an endpoint that has no half-close (the owner
/// pipe), the daemon may stay silent before the connection is ended: long enough for the answer to
/// a request sent last, short enough that an idle connection the client has left does not keep
/// `proxy` running.
const LINGER: Duration = Duration::from_secs(2);

/// How long connecting to the endpoint, and each read and write of `address --json`'s question,
/// may take.
const CONNECT_WAIT: Duration = Duration::from_secs(5);

/// `proxy`: connects to the running daemon's owner endpoint and carries bytes both ways between it
/// and this process's standard input and output until either side closes, writing nothing else on
/// standard output. Exits 3 when no daemon runs, [`NO_OWNER_ENDPOINT`], [`ENDPOINT_UNREACHABLE`]
/// and [`APPLICATION_NOT_RUNNING`] as their names say, each with the reason on stderr, 0 once the
/// connection is over and 1 when it failed.
pub(super) fn proxy() -> Result<i32> {
    let (endpoint, _, _) = match find_endpoint() {
        Ok(found) => found,
        Err(status) => return Ok(status),
    };
    let carried = match &endpoint {
        Endpoint::Socket(path) => match local_socket::connect(path, CONNECT_WAIT) {
            Ok(stream) => carry(stream, io::stdin(), io::stdout()),
            Err(err) => return Ok(say_unreachable(&endpoint, &err)),
        },
        Endpoint::Pipe(name) => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let carried = runtime.block_on(async {
                let pipe = match owner_pipe::connect_async(name, CONNECT_WAIT).await {
                    Ok(pipe) => pipe,
                    Err(err) => return Err(err),
                };
                Ok(
                    carry_without_half_close(pipe, tokio::io::stdin(), tokio::io::stdout(), LINGER)
                        .await,
                )
            });
            // Standard input is read on a thread of its own that cannot be stopped: the process
            // ends without waiting for it.
            runtime.shutdown_background();
            match carried {
                Ok(carried) => carried,
                Err(err) => return Ok(say_unreachable(&endpoint, &err)),
            }
        }
    };
    match carried {
        Ok(()) => Ok(0),
        Err(err) => {
            eprintln!("the connection to {} failed: {err}", crate::APP_NAME);
            Ok(1)
        }
    }
}

/// Says on stderr that `endpoint` cannot be reached, for `err`: the status to exit with.
fn say_unreachable(endpoint: &Endpoint, err: &io::Error) -> i32 {
    eprintln!(
        "the owner endpoint of the running {}, {endpoint}, cannot be reached: {err}",
        crate::APP_NAME
    );
    ENDPOINT_UNREACHABLE
}

/// `address --json`: prints on one line of stdout a JSON object with the running daemon's owner
/// key (`owner_key`), the protocol generation it speaks (`generation`) and its release version
/// (`daemon_version`), as the daemon itself states them on its owner endpoint, once it has accepted
/// the key there. The exit statuses are `proxy`'s (3, [`NO_OWNER_ENDPOINT`],
/// [`ENDPOINT_UNREACHABLE`], [`APPLICATION_NOT_RUNNING`]); a key the daemon refuses, or an answer
/// that cannot be read, is status 1, the reason on stderr and nothing on stdout.
pub(super) fn print_answer() -> Result<i32> {
    let (endpoint, owner, data) = match find_endpoint() {
        Ok(found) => found,
        Err(status) => return Ok(status),
    };
    let stream: Box<dyn ReadWrite> = match &endpoint {
        Endpoint::Socket(path) => match local_socket::connect(path, CONNECT_WAIT) {
            Ok(stream) => Box::new(stream),
            Err(err) => return Ok(say_unreachable(&endpoint, &err)),
        },
        Endpoint::Pipe(name) => match owner_pipe::connect(name, CONNECT_WAIT) {
            Ok(stream) => Box::new(stream),
            Err(err) => return Ok(say_unreachable(&endpoint, &err)),
        },
    };
    let Some(key) = crate::owner_key::read_in(&data) else {
        eprintln!("owner.key holds no usable owner key; `octoboardd address --rotate` makes one");
        return Ok(1);
    };
    match ask_hello(stream, owner.port, &key) {
        Ok(hello) => {
            println!("{}", answer_json(&key, &hello));
            Ok(0)
        }
        Err(Asked::KeyRefused) => {
            eprintln!(
                "the running {} refuses the owner key in owner.key (it may have been changed \
                 since it started); `octoboardd address --rotate` makes one it accepts",
                crate::APP_NAME
            );
            Ok(1)
        }
        Err(Asked::Failed(why)) => {
            eprintln!(
                "the running {} could not be asked which protocol it speaks: {why}",
                crate::APP_NAME
            );
            Ok(1)
        }
    }
}

/// The line `address --json` prints.
fn answer_json(key: &str, hello: &Hello) -> String {
    serde_json::json!({
        "owner_key": key,
        "generation": hello.generation,
        "daemon_version": hello.daemon_version,
    })
    .to_string()
}

/// A connection `address --json` asks its question over.
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// Where the running daemon's owner endpoint is.
#[derive(Debug, PartialEq, Eq)]
enum Endpoint {
    /// The socket at this path (Linux, macOS).
    Socket(PathBuf),
    /// The named pipe of this name (Windows).
    Pipe(String),
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Socket(path) => write!(f, "{}", path.display()),
            Self::Pipe(name) => f.write_str(name),
        }
    }
}

/// Finds the running daemon, waiting through a planned restart as the other commands do, and
/// where its owner endpoint is: the endpoint, the daemon and its data directory, or the status to
/// exit with, its reason said on stderr.
fn find_endpoint() -> std::result::Result<(Endpoint, Owner, PathBuf), i32> {
    let owner = match find_daemon(RESTART_WAIT, false) {
        Ok(Found::Running { owner, .. }) => owner,
        Ok(Found::NotRunning { .. }) => {
            let application = crate::platform::proc::running_image()
                .is_ok_and(|exe| crate::platform::is_application_copy(&exe, crate::APP_NAME));
            if application {
                eprintln!("{}", application_not_running());
                return Err(APPLICATION_NOT_RUNNING);
            }
            eprintln!("{}", not_running());
            return Err(NOT_RUNNING);
        }
        Ok(Found::Unrecorded) => {
            eprintln!(
                "{} is running, but where it listens is not known (it may still be starting or \
                 stopping)",
                crate::APP_NAME
            );
            return Err(1);
        }
        Err(err) => {
            eprintln!("{err:#}");
            return Err(1);
        }
    };
    let data = crate::paths::data_dir();
    match endpoint_of(&owner, &data) {
        Ok(endpoint) => Ok((endpoint, owner, data)),
        Err(why) => {
            eprintln!("{why}");
            Err(NO_OWNER_ENDPOINT)
        }
    }
}

/// What `proxy` and `address --json` say when the desktop application whose copy of `octoboardd`
/// they are is not running.
fn application_not_running() -> String {
    format!(
        "the {app} application is not running on this computer: its daemon runs only while the \
         application does; open {app} here and keep it running",
        app = crate::APP_NAME
    )
}

/// The owner endpoint of `owner`, a daemon whose data directory is `data`, or why it has none.
fn endpoint_of(owner: &Owner, data: &Path) -> std::result::Result<Endpoint, String> {
    if !local_socket::AVAILABLE && !owner_pipe::AVAILABLE {
        return Err(format!(
            "the running {} has no owner endpoint: there is none on this system",
            crate::APP_NAME
        ));
    }
    if owner.follows_application == Some(true) && !owner.takes_owner_key {
        return Err(format!(
            "the running {app} has no owner endpoint: remote access to the {app} application's \
             daemon is off; turn on Allow remote access in its Settings, under Remote access",
            app = crate::APP_NAME
        ));
    }
    if !owner.takes_owner_key {
        return Err(format!(
            "the running {} has no owner endpoint: it is an earlier build, its build embeds no UI, \
             or its data directory is not private to its user or its socket could not be opened, \
             as its log says",
            crate::APP_NAME
        ));
    }
    if owner_pipe::AVAILABLE {
        return Ok(Endpoint::Pipe(owner_pipe::name_for(data)));
    }
    Ok(Endpoint::Socket(crate::browser::socket::path_in(data)))
}

/// Carries bytes from `input` to `stream` and from `stream` to `output` until the daemon's side
/// closes. When `input` ends, the stream's sending half is shut down and what the daemon still
/// sends is carried on, so that the answer to the last request is not lost. A daemon that resets
/// the connection, or an `output` whose reader has gone, ends it as a close does. The two
/// directions are copied on separate threads: through Win32-OpenSSH's cmd or PowerShell, a piper
/// that reads and writes on one thread stalls under full-duplex load.
fn carry(
    stream: BlockingStream,
    mut input: impl Read + Send + 'static,
    mut output: impl Write,
) -> io::Result<()> {
    // The connect bounded every read and write; a connection may now be idle for as long as it is
    // open.
    stream.set_read_timeout(None)?;
    stream.set_write_timeout(None)?;
    let mut sending = stream.try_clone()?;
    std::thread::spawn(move || {
        let _ = io::copy(&mut input, &mut sending);
        let _ = sending.shutdown(Shutdown::Write);
    });
    let mut receiving = stream;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = match receiving.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if is_closed(&err) => return Ok(()),
            Err(err) => return Err(err),
        };
        match output
            .write_all(&buffer[..read])
            .and_then(|()| output.flush())
        {
            Ok(()) => {}
            Err(err) if is_closed(&err) => return Ok(()),
            Err(err) => return Err(err),
        }
    }
}

/// Whether `err` says the other side is gone, which ends a connection as a close does.
fn is_closed(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    )
}

/// Carries bytes from `input` to `stream` and from `stream` to `output`, on an endpoint that has
/// no half-close: until the daemon's side closes, or, once `input` has ended, until the daemon has
/// sent nothing for `linger`. Sending and receiving go on side by side, so neither waits on the
/// other. A daemon that resets the connection, or an `output` whose reader has gone, ends it as a
/// close does.
async fn carry_without_half_close<S, I, O>(
    stream: S,
    mut input: I,
    mut output: O,
    linger: Duration,
) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    I: tokio::io::AsyncRead + Unpin,
    O: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut receiving, mut sending) = tokio::io::split(stream);
    let (ended, mut input_ended) = tokio::sync::watch::channel(false);
    let send = async move {
        let _ = tokio::io::copy(&mut input, &mut sending).await;
        let _ = sending.flush().await;
        let _ = ended.send(true);
        // Kept open: dropping the sending half would not close a pipe, and it is not needed.
        std::future::pending::<()>().await;
    };
    let receive = async move {
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = if *input_ended.borrow() {
                match tokio::time::timeout(linger, receiving.read(&mut buffer)).await {
                    Ok(read) => read,
                    Err(_) => return Ok(()),
                }
            } else {
                tokio::select! {
                    read = receiving.read(&mut buffer) => read,
                    _ = input_ended.changed() => continue,
                }
            };
            let read = match read {
                Ok(0) => return Ok(()),
                Ok(read) => read,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) if is_closed(&err) => return Ok(()),
                Err(err) => return Err(err),
            };
            let written = output.write_all(&buffer[..read]).await;
            match written.and(output.flush().await) {
                Ok(()) => {}
                Err(err) if is_closed(&err) => return Ok(()),
                Err(err) => return Err(err),
            }
        }
    };
    tokio::select! {
        received = receive => received,
        () = send => unreachable!("sending never ends of itself"),
    }
}

/// What the daemon states in the `hello` that opens a control connection.
#[derive(Debug, PartialEq, Eq)]
struct Hello {
    generation: u32,
    daemon_version: String,
}

/// Why [`ask_hello`] has no answer.
#[derive(Debug)]
enum Asked {
    /// The daemon refused the key.
    KeyRefused,
    /// Anything else, said.
    Failed(String),
}

/// Opens a control connection on `stream` with `key`, naming `port` as the address's, and reads the
/// `hello` the daemon opens it with. The hello's shape is frozen across generations (see "Versions
/// and compatibility" in `apps/daemon/PROTOCOL.md`), so a daemon of any generation is read; one
/// that opens with anything else predates the hello.
fn ask_hello(stream: impl Read + Write, port: u16, key: &str) -> std::result::Result<Hello, Asked> {
    use tokio_tungstenite::tungstenite::{self, Message};

    let url = format!("ws://127.0.0.1:{port}/ws/control?key={key}");
    let (mut socket, _) = match tungstenite::client(url, stream) {
        Ok(connected) => connected,
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response)))
            if response.status() == 401 =>
        {
            return Err(Asked::KeyRefused);
        }
        Err(err) => return Err(Asked::Failed(err.to_string())),
    };
    let hello = loop {
        match socket.read() {
            Ok(Message::Text(text)) => break read_hello(&text),
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(other) => break Err(format!("it opened with {other:?} rather than its hello")),
            Err(err) => break Err(err.to_string()),
        }
    };
    let _ = socket.close(None);
    let _ = socket.flush();
    hello.map_err(Asked::Failed)
}

/// The hello in the text frame `text`.
fn read_hello(text: &str) -> std::result::Result<Hello, String> {
    let frame: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("its first frame is not JSON: {err}"))?;
    if frame.get("type").and_then(|kind| kind.as_str()) != Some("hello") {
        return Err("it opened with no hello, as a build earlier than the hello does".to_owned());
    }
    let generation = frame
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        .and_then(|generation| u32::try_from(generation).ok());
    let daemon_version = frame
        .get("daemon_version")
        .and_then(|version| version.as_str());
    match (generation, daemon_version) {
        (Some(generation), Some(version)) => Ok(Hello {
            generation,
            daemon_version: version.to_owned(),
        }),
        _ => Err(format!("its hello is not readable: {text}")),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use std::os::unix::net::UnixStream;

    use crate::browser::serve_on as serve;
    use crate::browser::socket::path_in;
    use crate::test_support::PATIENCE;

    /// The daemon's own browser router on a real socket in the data directory of a scratch state,
    /// with its owner key, until the returned guard is dropped.
    fn served(
        name: &str,
    ) -> (
        crate::browser::ServeTask,
        crate::test_support::ScratchDir,
        String,
    ) {
        let (state, dir) = crate::test_support::app_state(name);
        assert!(state.owner_key.start(true, &dir));
        let key = crate::owner_key::read_in(&dir).expect("the owner key");
        let listener = local_socket::Listener::bind_replacing_stale(&path_in(&dir)).unwrap();
        (
            serve(listener, crate::server::browser_router(state)),
            dir,
            key,
        )
    }

    /// The program's side of a [`carry`] running on a thread of its own: what it writes is the
    /// carrier's input, what it reads the carrier's output.
    struct Program {
        to_carrier: UnixStream,
        from_carrier: UnixStream,
        carrier: std::thread::JoinHandle<io::Result<()>>,
    }

    impl Program {
        fn through(path: &Path) -> Self {
            let stream = local_socket::connect(path, PATIENCE).unwrap();
            let (to_carrier, input) = UnixStream::pair().unwrap();
            let (output, from_carrier) = UnixStream::pair().unwrap();
            from_carrier.set_read_timeout(Some(PATIENCE)).unwrap();
            let carrier = std::thread::spawn(move || carry(stream, input, output));
            Self {
                to_carrier,
                from_carrier,
                carrier,
            }
        }
    }

    /// An HTTP request written to the carrier's input is answered on its output as the socket
    /// answers it, the answer to the last request arriving after the input has ended; the carrier
    /// then ends.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_http_request_through_the_carrier_is_answered_as_on_the_socket() {
        let (_served, dir, key) = served("proxy-http");
        let path = path_in(&dir);
        let answers = tokio::task::spawn_blocking(move || {
            let mut answers = Vec::new();
            for key in [key.as_str(), &"0".repeat(64)] {
                let mut program = Program::through(&path);
                write!(
                    program.to_carrier,
                    "GET /key-check?key={key} HTTP/1.1\r\nHost: 127.0.0.1:3369\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                program.to_carrier.shutdown(Shutdown::Write).unwrap();
                let mut answer = String::new();
                program.from_carrier.read_to_string(&mut answer).unwrap();
                program.carrier.join().unwrap().unwrap();
                answers.push(answer.lines().next().unwrap_or_default().to_owned());
            }
            answers
        })
        .await
        .unwrap();
        assert_eq!(
            answers,
            ["HTTP/1.1 204 No Content", "HTTP/1.1 401 Unauthorized"]
        );
    }

    /// A WebSocket opened through the carrier gets the daemon's hello and answers a ping, as one
    /// opened on the socket does; closing it ends the carrier.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_websocket_through_the_carrier_is_served_as_on_the_socket() {
        use tokio_tungstenite::tungstenite::{self, Message};

        let (_served, dir, key) = served("proxy-ws");
        let path = path_in(&dir);
        tokio::task::spawn_blocking(move || {
            let program = Program::through(&path);
            let duplex = Duplex {
                read: program.from_carrier,
                write: program.to_carrier,
            };
            let url = format!("ws://127.0.0.1:3369/ws/control?key={key}");
            let (mut socket, response) = tungstenite::client(url, duplex).expect("an upgrade");
            assert_eq!(response.status(), 101);
            let hello = socket.read().unwrap();
            assert!(
                matches!(&hello, Message::Text(text) if read_hello(text).is_ok()),
                "{hello:?}"
            );
            socket.send(Message::Ping(vec![7])).unwrap();
            loop {
                match socket.read().expect("an answer to the ping") {
                    Message::Pong(payload) => {
                        assert_eq!(payload, vec![7]);
                        break;
                    }
                    Message::Close(_) => panic!("closed before the pong"),
                    _ => {}
                }
            }
            socket.close(None).unwrap();
            while socket.read().is_ok() {}
            drop(socket);
            program.carrier.join().unwrap().unwrap();
        })
        .await
        .unwrap();
    }

    /// The program's side of the carrier as one stream, for a WebSocket client.
    struct Duplex {
        read: UnixStream,
        write: UnixStream,
    }

    impl Read for Duplex {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.read.read(buffer)
        }
    }

    impl Write for Duplex {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.write.write(buffer)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.write.flush()
        }
    }

    impl Drop for Duplex {
        fn drop(&mut self) {
            let _ = self.write.shutdown(Shutdown::Write);
        }
    }

    /// The daemon's hello on its owner endpoint is read for the owner key, and the key it refuses
    /// is told apart.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_hello_is_asked_on_the_endpoint_with_the_owner_key() {
        let (_served, dir, key) = served("proxy-hello");
        let path = path_in(&dir);
        let (accepted, refused) = tokio::task::spawn_blocking(move || {
            let ask =
                |key: &str| ask_hello(local_socket::connect(&path, PATIENCE).unwrap(), 3369, key);
            (ask(&key), ask(&"0".repeat(64)))
        })
        .await
        .unwrap();
        assert_eq!(
            accepted.unwrap(),
            Hello {
                generation: crate::protocol::GENERATION,
                daemon_version: crate::protocol::DAEMON_VERSION.to_owned(),
            }
        );
        assert!(matches!(refused, Err(Asked::KeyRefused)), "{refused:?}");
    }

    #[test]
    fn the_answer_is_one_json_object_with_the_key_the_generation_and_the_version() {
        let line = answer_json(
            "ab",
            &Hello {
                generation: 4,
                daemon_version: "1.2.3".to_owned(),
            },
        );
        assert_eq!(
            line,
            r#"{"owner_key":"ab","generation":4,"daemon_version":"1.2.3"}"#
        );
    }

    #[test]
    fn a_first_frame_that_is_no_hello_is_not_read_as_one() {
        assert!(read_hello(r#"{"type":"snapshot"}"#).is_err());
        assert!(read_hello(r#"{"type":"hello","generation":4}"#).is_err());
        assert!(read_hello("not json").is_err());
        assert_eq!(
            read_hello(r#"{"type":"hello","generation":5,"daemon_version":"9.0.0","extra":1}"#),
            Ok(Hello {
                generation: 5,
                daemon_version: "9.0.0".to_owned()
            })
        );
    }

    /// A daemon that takes no owner key has no owner endpoint, the application's daemon because
    /// remote access is off; one that does has its data directory's socket, the application's
    /// daemon included.
    #[test]
    fn only_a_daemon_taking_the_owner_key_has_an_owner_endpoint() {
        let owner = crate::cli::background::tests::owner_on(7531);
        let data = Path::new("/data");
        let alone = endpoint_of(&owner, data).unwrap_err();
        assert!(alone.contains("no owner endpoint"), "{alone}");
        assert!(!alone.contains("remote access"), "{alone}");
        let application = Owner {
            follows_application: Some(true),
            ..owner.clone()
        };
        let off = endpoint_of(&application, data).unwrap_err();
        assert!(off.contains("no owner endpoint"), "{off}");
        assert!(off.contains("Allow remote access"), "{off}");
        assert_eq!(
            endpoint_of(&application.with_owner_key(true), data).unwrap(),
            Endpoint::Socket(path_in(data))
        );
        assert_eq!(
            endpoint_of(&owner.with_owner_key(true), data).unwrap(),
            Endpoint::Socket(path_in(data))
        );
    }
}

/// The carrier for an endpoint with no half-close, on in-memory streams standing in for the pipe,
/// standard input and standard output. The linger is the production one, run on a paused clock, so
/// that no outcome depends on how fast the machine runs.
#[cfg(test)]
mod carrier_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// The answer to a request sent just before the input ended still arrives, and the connection
    /// ends once the daemon has been silent for the linger, though the daemon keeps it open.
    #[tokio::test(start_paused = true)]
    async fn the_answer_after_the_input_ends_arrives_and_silence_ends_it() {
        let (pipe, mut daemon) = tokio::io::duplex(1024);
        let daemon_side = tokio::spawn(async move {
            let mut request = [0; 7];
            daemon.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"request");
            // The answer comes later than the input ended, within the linger.
            tokio::time::sleep(LINGER / 2).await;
            daemon.write_all(b"answer").await.unwrap();
            // Kept open, as an idle keep-alive connection is.
            std::future::pending::<()>().await;
        });
        let mut output = Vec::new();
        let started = tokio::time::Instant::now();
        carry_without_half_close(pipe, &b"request"[..], &mut output, LINGER)
            .await
            .unwrap();
        assert_eq!(output, b"answer");
        let waited = started.elapsed();
        assert!(
            waited >= LINGER,
            "ended before the daemon had been silent for the linger: {waited:?}"
        );
        assert!(
            waited < LINGER * 3,
            "the daemon's open connection kept it: {waited:?}"
        );
        daemon_side.abort();
    }

    /// A daemon that closes ends the connection at once, while the input is still open: it does
    /// not wait for the linger, nor for the input.
    #[tokio::test(start_paused = true)]
    async fn the_daemon_closing_ends_it_while_the_input_is_open() {
        let (pipe, mut daemon) = tokio::io::duplex(1024);
        let (_input_writer, input) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            daemon.write_all(b"bye").await.unwrap();
        });
        let mut output = Vec::new();
        let started = tokio::time::Instant::now();
        tokio::time::timeout(
            LINGER / 2,
            carry_without_half_close(pipe, input, &mut output, LINGER),
        )
        .await
        .expect("ended when the daemon closed")
        .unwrap();
        assert_eq!(output, b"bye");
        assert!(started.elapsed() < LINGER / 2, "{:?}", started.elapsed());
    }

    /// While the input is open, a silent daemon does not end the connection however long it is
    /// silent, and what is sent later in both directions is carried.
    #[tokio::test(start_paused = true)]
    async fn an_idle_connection_lasts_while_the_input_is_open() {
        let (pipe, mut daemon) = tokio::io::duplex(1024);
        let (mut input_writer, input) = tokio::io::duplex(1024);
        let (output, mut output_reader) = tokio::io::duplex(1024);
        let carrier = tokio::spawn(carry_without_half_close(pipe, input, output, LINGER));
        // Everything is idle, so the clock runs through the whole silence.
        tokio::time::sleep(LINGER * 3).await;
        assert!(!carrier.is_finished(), "an idle connection was ended");
        input_writer.write_all(b"ping").await.unwrap();
        let mut ping = [0; 4];
        daemon.read_exact(&mut ping).await.unwrap();
        assert_eq!(&ping, b"ping");
        daemon.write_all(b"pong").await.unwrap();
        let mut pong = [0; 4];
        output_reader.read_exact(&mut pong).await.unwrap();
        assert_eq!(&pong, b"pong");
        drop(daemon);
        tokio::time::timeout(LINGER, carrier)
            .await
            .expect("ended when the daemon closed")
            .unwrap()
            .unwrap();
    }

    /// An input that ends with nothing sent, to a daemon that never answers, ends the connection
    /// after the linger alone; the carrier is not left running for a client that has gone.
    #[tokio::test(start_paused = true)]
    async fn an_input_that_ends_to_a_silent_daemon_ends_it_after_the_linger() {
        let (pipe, _daemon) = tokio::io::duplex(1024);
        let mut output = Vec::new();
        let started = tokio::time::Instant::now();
        carry_without_half_close(pipe, &b""[..], &mut output, LINGER)
            .await
            .unwrap();
        assert!(output.is_empty());
        assert!(started.elapsed() >= LINGER, "{:?}", started.elapsed());
    }
}
