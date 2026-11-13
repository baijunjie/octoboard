//! The HTTP the update makes, behind a seam so that no test touches the network: reading the
//! manifest and streaming the archive. Reached the way the user's own network reaches the host:
//! through the proxy their shell environment names, trusting the system's certificate store.

use std::io::{self, Read};
use std::time::Duration;

use anyhow::Context;

/// Why a request did not give a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The server answered with this status (a redirect chain's last one).
    Status(u16),
    /// The host could not be reached, the answer did not come in time, the body was longer than
    /// the limit or could not be taken in; the English text says which.
    Failed(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status(status) => write!(f, "the server answered {status}"),
            Self::Failed(detail) => f.write_str(detail),
        }
    }
}

/// What takes the body of a response, a piece at a time, with the body's size when the server
/// said it. An error from it (a disk that is full) ends the request.
pub type Sink<'a> = dyn FnMut(&[u8], Option<u64>) -> io::Result<()> + 'a;

/// A source of bytes by address.
pub trait Fetcher: Send + Sync {
    /// Reads `url` to its end into `sink`, refusing a body longer than `max_bytes` and a request
    /// that takes longer than `timeout` as a whole.
    fn get(
        &self,
        url: &str,
        max_bytes: u64,
        timeout: Duration,
        sink: &mut Sink<'_>,
    ) -> Result<(), FetchError>;
}

/// `url`'s body as text, no longer than `max_bytes`.
pub fn get_text(
    fetcher: &dyn Fetcher,
    url: &str,
    max_bytes: u64,
    timeout: Duration,
) -> Result<String, FetchError> {
    let mut body = Vec::new();
    fetcher.get(url, max_bytes, timeout, &mut |chunk, _| {
        body.extend_from_slice(chunk);
        Ok(())
    })?;
    String::from_utf8(body).map_err(|_| FetchError::Failed("the answer is not text".to_owned()))
}

/// How long a connection (name lookup, proxy and TLS included) and the wait for the first answer
/// may each take, and how long a body may go without a byte before the request is given up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
const IDLE: Duration = Duration::from_secs(60);

/// How many threads of requests given up on (`pump`) may still wait on their connections before no
/// new request is made: each ends only with its request's own deadline, so a network that stalls
/// every time would otherwise leave one behind per attempt.
const LEFT_BEHIND_LIMIT: usize = 2;

/// The threads of `pump` alive now.
static PUMPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts a thread of `pump` while it lives.
struct PumpGuard;

impl PumpGuard {
    fn new() -> Self {
        PUMPS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self
    }
}

impl Drop for PumpGuard {
    fn drop(&mut self) {
        PUMPS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Whether `alive` threads of given-up requests are too many for another request.
fn too_many_left_behind(alive: usize) -> bool {
    alive >= LEFT_BEHIND_LIMIT
}

/// The environment a request's proxy is read from.
enum Environment {
    /// The cached snapshot of the user's login shell.
    Snapshot,
    /// This process's own.
    Process,
}

impl Environment {
    fn read(&self) -> anyhow::Result<std::collections::HashMap<String, String>> {
        match self {
            Self::Snapshot => crate::env_shell::cached_snapshot(),
            Self::Process => Ok(std::env::vars().collect()),
        }
    }
}

/// The real one: `ureq`, through the proxy the user's environment names.
pub struct UreqFetcher {
    environment: Environment,
}

impl UreqFetcher {
    /// For the daemon, which is not started from the user's shell: the proxy comes from the
    /// snapshot of their login shell.
    pub fn daemon() -> Self {
        Self {
            environment: Environment::Snapshot,
        }
    }

    /// For a command line process, which already runs in the user's shell environment: the
    /// proxy comes from its own. A snapshot would start the login shell, which an interactive one
    /// stops on the terminal this process shares.
    pub fn command_line() -> Self {
        Self {
            environment: Environment::Process,
        }
    }

    fn agent(&self, url: &str, timeout: Duration) -> Result<ureq::Agent, FetchError> {
        let shell_env = self
            .environment
            .read()
            .context("the shell environment could not be taken")
            .map_err(|err| FetchError::Failed(format!("{err:#}")))?;
        let host = url
            .strip_prefix("https://")
            .and_then(|rest| rest.split(['/', ':']).next())
            .unwrap_or_default();
        let proxy = crate::env_shell::proxy_for(&shell_env, host).map_err(FetchError::Failed)?;
        Ok(ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            // Before any body comes, no phase waits for long; once it does, `pump` gives up on a
            // body that stops coming.
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            // The manifest names the archive's address and a redirect names another: none may
            // leave HTTPS.
            .https_only(true)
            .proxy(proxy)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into())
    }
}

impl Fetcher for UreqFetcher {
    fn get(
        &self,
        url: &str,
        max_bytes: u64,
        timeout: Duration,
        sink: &mut Sink<'_>,
    ) -> Result<(), FetchError> {
        if too_many_left_behind(PUMPS.load(std::sync::atomic::Ordering::SeqCst)) {
            return Err(FetchError::Failed(
                "an earlier download is still being given up".to_owned(),
            ));
        }
        let agent = self.agent(url, timeout)?;
        let response = agent.get(url).call().map_err(|err| match err {
            ureq::Error::StatusCode(status) => FetchError::Status(status),
            other => FetchError::Failed(other.to_string()),
        })?;
        let total = response.body().content_length();
        if total.is_some_and(|total| total > max_bytes) {
            return Err(FetchError::Failed(format!(
                "the answer is {} bytes, more than the {max_bytes} allowed",
                total.unwrap_or_default()
            )));
        }
        let reader = response
            .into_body()
            .into_with_config()
            .limit(max_bytes)
            .reader();
        pump(reader, IDLE, total, sink)
    }
}

/// Reads `reader` to its end on a thread of its own and hands each piece to `sink`, giving up when
/// no piece comes for `idle`: a read that waits on a stalled connection cannot be interrupted, so
/// the thread is left to end with the request's own deadline, and the caller is not held by it.
fn pump(
    reader: impl Read + Send + 'static,
    idle: Duration,
    total: Option<u64>,
    sink: &mut Sink<'_>,
) -> Result<(), FetchError> {
    let (send, receive) = std::sync::mpsc::sync_channel::<io::Result<Vec<u8>>>(2);
    let alive = PumpGuard::new();
    std::thread::spawn(move || {
        let _alive = alive;
        let mut reader = reader;
        loop {
            let mut chunk = vec![0u8; 64 * 1024];
            let piece = match reader.read(&mut chunk) {
                // An empty piece is the end.
                Ok(0) => Ok(Vec::new()),
                Ok(read) => {
                    chunk.truncate(read);
                    Ok(chunk)
                }
                Err(err) => Err(err),
            };
            let last = !matches!(&piece, Ok(piece) if !piece.is_empty());
            if send.send(piece).is_err() || last {
                return;
            }
        }
    });
    loop {
        match receive.recv_timeout(idle) {
            Ok(Ok(piece)) if piece.is_empty() => return Ok(()),
            Ok(Ok(piece)) => {
                sink(&piece, total).map_err(|err| FetchError::Failed(err.to_string()))?;
            }
            Ok(Err(err)) => return Err(FetchError::Failed(err.to_string())),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                return Err(FetchError::Failed(format!(
                    "no data came for {} seconds",
                    idle.as_secs().max(1)
                )))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(FetchError::Failed("the connection ended early".to_owned()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that gives its pieces, each after a wait.
    struct Slow(Vec<(Duration, Vec<u8>)>);

    impl Read for Slow {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            let (wait, piece) = self.0.remove(0);
            std::thread::sleep(wait);
            out[..piece.len()].copy_from_slice(&piece);
            Ok(piece.len())
        }
    }

    #[test]
    fn a_body_is_handed_over_piece_by_piece_to_its_end() {
        let mut got = Vec::new();
        let body = Slow(vec![
            (Duration::ZERO, b"ab".to_vec()),
            (Duration::from_millis(20), b"cd".to_vec()),
        ]);
        pump(
            body,
            Duration::from_secs(5),
            Some(4),
            &mut |piece, total| {
                assert_eq!(total, Some(4));
                got.extend_from_slice(piece);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(got, b"abcd");
    }

    /// A connection that stops sending is given up on at the idle deadline, not at the request's
    /// whole one, and the caller is not held by the read that waits.
    #[test]
    fn a_body_that_stops_coming_is_given_up_on_at_the_idle_deadline() {
        let began = std::time::Instant::now();
        let body = Slow(vec![
            (Duration::ZERO, b"ab".to_vec()),
            (Duration::from_secs(2), b"cd".to_vec()),
        ]);
        let mut got = Vec::new();
        let err = pump(body, Duration::from_millis(200), None, &mut |piece, _| {
            got.extend_from_slice(piece);
            Ok(())
        })
        .unwrap_err();
        assert!(matches!(err, FetchError::Failed(text) if text.contains("no data")));
        assert_eq!(got, b"ab");
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn requests_stop_while_two_given_up_on_are_still_waiting() {
        assert!(!too_many_left_behind(0));
        assert!(!too_many_left_behind(1));
        assert!(too_many_left_behind(2));
        // A thread is counted while it lives and not after.
        let (first, second) = (PumpGuard::new(), PumpGuard::new());
        assert!(too_many_left_behind(
            PUMPS.load(std::sync::atomic::Ordering::SeqCst)
        ));
        drop((first, second));
    }

    #[test]
    fn a_refusing_sink_ends_the_read() {
        let body = Slow(vec![(Duration::ZERO, b"ab".to_vec()); 3]);
        let err = pump(body, Duration::from_secs(5), None, &mut |_, _| {
            Err(io::Error::other("stop"))
        })
        .unwrap_err();
        assert_eq!(err, FetchError::Failed("stop".to_owned()));
    }

    /// The command line fetcher reads the proxy from the environment it runs in, and takes no login
    /// shell snapshot (which would wait out a deadline on an interactive terminal).
    #[test]
    fn the_command_line_reads_its_own_environment() {
        let fetcher = UreqFetcher::command_line();
        assert!(matches!(fetcher.environment, Environment::Process));
        let env = fetcher.environment.read().unwrap();
        assert_eq!(env.get("PATH"), std::env::var("PATH").ok().as_ref());
        assert!(matches!(
            UreqFetcher::daemon().environment,
            Environment::Snapshot
        ));
    }
}
