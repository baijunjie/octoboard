//! The SSH connection the application makes to a remote host itself (russh): it verifies the host
//! key against the pinned fingerprint, authenticates with a password (answering a
//! keyboard-interactive login with it where the host asks for one, as hosts that sign in through
//! PAM do), a key file or this device's own key (`device_key`), runs commands and opens the
//! channels the remote window's connections (and the forwarded loopback ports') are carried on, each within a time limit, so a host
//! that stops answering fails the connection rather than holding it forever. A failure comes back
//! by kind, for the UI to say what went wrong and what to do about it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, Handle, KeyboardInteractiveAuthResponse, Msg};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg, ChannelOpenFailure, Disconnect, MethodKind, MethodSet};

/// How long reaching the host and agreeing on keys may take, and then signing in.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a command that reads or sets something up on the host (the shell probe, `address`,
/// installing a key), or starting a proxy, may take before the host is taken as not answering.
#[cfg(not(test))]
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// Short in the tests, which see a host that stops answering run into it.
#[cfg(test)]
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a `direct-tcpip` channel to a Windows host is watched, once confirmed, for the host closing it
/// unasked: Win32-OpenSSH confirms a channel to a port nothing listens on and then closes it, where OpenSSH on
/// Unix refuses it (`ForwardError::NothingListening`). A channel still open after this long is taken as connected.
pub const WINDOWS_EARLY_CLOSE: Duration = Duration::from_millis(500);

/// How many rounds of questions a keyboard-interactive login is answered for before it is taken as
/// refused: a PAM password login asks one.
const KEYBOARD_INTERACTIVE_ROUNDS: usize = 5;

/// How often an idle connection is probed, and how many probes may go unanswered before it is
/// taken as dropped: a connection that died without a word is noticed within a minute.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
const KEEPALIVE_MAX: usize = 3;

/// How the application proves who it is to the host.
#[derive(Clone)]
pub enum Secret {
    Password(String),
    Key {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// This device's own key, read from the credential store.
    DeviceKey(Arc<PrivateKey>),
}

/// What the connection is made with.
#[derive(Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub secret: Secret,
    /// The fingerprint the host key must have; `None` refuses every host, reporting the key it
    /// showed for the owner to confirm.
    pub pinned: Option<String>,
}

/// Why no connection was made.
#[derive(Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// The host could not be reached, or did not speak SSH.
    Unreachable(String),
    /// No fingerprint is pinned: the host showed this one, which the owner may confirm.
    HostKeyUnknown { fingerprint: String },
    /// The host's key is not the pinned one.
    HostKeyMismatch { expected: String, actual: String },
    /// The key file needs its passphrase.
    PassphraseNeeded,
    /// The key file could not be read or decoded, or its passphrase is wrong.
    KeyUnreadable(String),
    /// The host refused the password or the key.
    AuthFailed(String),
}

impl ConnectError {
    /// Whether trying again with the same target would meet the same answer: every failure but a
    /// host that could not be reached — a refused password or key, a host key other than the one
    /// expected, and a key file that cannot be read or needs its passphrase.
    pub fn is_refusal(&self) -> bool {
        !matches!(self, Self::Unreachable(_))
    }
}

/// The fingerprint of a host key as OpenSSH prints it, `SHA256:<base64>`.
pub fn fingerprint(key: &PublicKeyOrCertificate) -> String {
    let data = match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => key.key_data().clone(),
        PublicKeyOrCertificate::Certificate(certificate) => certificate.public_key().clone(),
    };
    data.fingerprint(HashAlg::Sha256).to_string()
}

/// Expands a leading `~` in a key file's path to the home directory.
pub fn expand_home(path: &str) -> PathBuf {
    let home = || {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    };
    if path == "~" {
        if let Some(home) = home() {
            return home;
        }
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = home() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Reads the private key at `path`, decrypting it with `passphrase` when it is encrypted.
pub fn load_key(path: &Path, passphrase: Option<&str>) -> Result<PrivateKey, ConnectError> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| ConnectError::KeyUnreadable(format!("{}: {err}", path.display())))?;
    match russh::keys::decode_secret_key(&text, passphrase) {
        Ok(key) => Ok(key),
        Err(russh::keys::Error::KeyIsEncrypted) if passphrase.is_none() => {
            Err(ConnectError::PassphraseNeeded)
        }
        Err(err) => Err(ConnectError::KeyUnreadable(format!(
            "{}: {err}",
            path.display()
        ))),
    }
}

/// The handler russh asks about the host key: it accepts the pinned key alone, and keeps the
/// fingerprint of whatever key the host showed.
struct HostCheck {
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
    /// Says the connection has ended once russh drops the handler, which it does as the
    /// connection's task ends, however it ended.
    _ended: Ended,
}

/// Sets its flag when dropped.
struct Ended(tokio::sync::watch::Sender<bool>);

impl Drop for Ended {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

impl client::Handler for HostCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let shown = fingerprint(key);
        let accepted = self
            .pinned
            .as_deref()
            .is_some_and(|pinned| super::entries::same_fingerprint(pinned, &shown));
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Some(shown);
        Ok(accepted)
    }
}

/// What a command run on the host ended with.
#[derive(Debug, Default)]
pub struct Output {
    /// `None` when the host sent no exit status (the command was killed by a signal, say).
    pub status: Option<u32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why a command run on the host brought nothing back.
#[derive(Debug)]
pub enum RunError {
    Ssh(russh::Error),
    /// The host did not finish it within this long.
    TimedOut(Duration),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ssh(err) => err.fmt(f),
            Self::TimedOut(limit) => write!(
                f,
                "the host did not answer within {} seconds",
                limit.as_secs()
            ),
        }
    }
}

/// Why a connection to a port on the host's own loopback could not be forwarded.
#[derive(Debug)]
pub enum ForwardError {
    /// The host's SSH server opened no channel because nothing accepted its connection to the port.
    NothingListening,
    /// The host's SSH server does not allow forwarding (or this kind of channel).
    Refused,
    /// The host did not answer within this long.
    TimedOut(Duration),
    Ssh(russh::Error),
}

/// `future`, or `RunError::TimedOut` once `limit` has passed.
async fn within<T>(
    limit: Duration,
    future: impl std::future::Future<Output = Result<T, russh::Error>>,
) -> Result<T, RunError> {
    tokio::time::timeout(limit, future)
        .await
        .map_err(|_| RunError::TimedOut(limit))?
        .map_err(RunError::Ssh)
}

/// An authenticated connection to a host.
pub struct Session {
    handle: Handle<HostCheck>,
    /// The fingerprint of the host key the connection was made to.
    pub fingerprint: String,
    /// Set once the connection has ended (`Ended`).
    ended: tokio::sync::watch::Receiver<bool>,
}

/// Connects to `target` and authenticates.
pub async fn connect(target: &Target) -> Result<Session, ConnectError> {
    // The key is read first: a passphrase it needs is asked for before anything goes out.
    let key = match &target.secret {
        Secret::Key { path, passphrase } => Some(Arc::new(load_key(path, passphrase.as_deref())?)),
        Secret::DeviceKey(key) => Some(key.clone()),
        Secret::Password(_) => None,
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        keepalive_max: KEEPALIVE_MAX,
        nodelay: true,
        ..Default::default()
    });
    let seen = Arc::new(Mutex::new(None));
    let (ended_flag, ended) = tokio::sync::watch::channel(false);
    let handler = HostCheck {
        pinned: target.pinned.clone(),
        seen: seen.clone(),
        _ended: Ended(ended_flag),
    };
    let address = (target.host.as_str(), target.port);
    let connected =
        tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, address, handler))
            .await
            .map_err(|_| {
                ConnectError::Unreachable(format!(
                    "{}:{} did not answer within {} seconds",
                    target.host,
                    target.port,
                    CONNECT_TIMEOUT.as_secs()
                ))
            })?;
    let shown = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut handle = match (connected, shown) {
        (Ok(handle), _) => handle,
        // The host showed a key the handler refused.
        (Err(russh::Error::UnknownKey), Some(actual)) => {
            return Err(match &target.pinned {
                Some(expected) => ConnectError::HostKeyMismatch {
                    expected: expected.clone(),
                    actual,
                },
                None => ConnectError::HostKeyUnknown {
                    fingerprint: actual,
                },
            })
        }
        (Err(err), _) => return Err(ConnectError::Unreachable(err.to_string())),
    };
    let fingerprint = seen
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    let signed_in = tokio::time::timeout(CONNECT_TIMEOUT, authenticate(&mut handle, target, key))
        .await
        .map_err(|_| {
            ConnectError::Unreachable(format!(
                "{}:{} did not finish signing in within {} seconds",
                target.host,
                target.port,
                CONNECT_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|err| ConnectError::Unreachable(err.to_string()))?;
    if !signed_in {
        let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
        let what = match target.secret {
            Secret::Password(_) => "password",
            Secret::Key { .. } => "key",
            Secret::DeviceKey(_) => "device key",
        };
        return Err(ConnectError::AuthFailed(format!(
            "{}@{} refused the {what}",
            target.user, target.host
        )));
    }
    Ok(Session {
        handle,
        fingerprint,
        ended,
    })
}

/// Signs in to `target` over `handle`, with `key` for a key secret; whether the host accepted it.
async fn authenticate(
    handle: &mut Handle<HostCheck>,
    target: &Target,
    key: Option<Arc<PrivateKey>>,
) -> Result<bool, russh::Error> {
    match (&target.secret, key) {
        (Secret::Password(password), _) => {
            let result = handle.authenticate_password(&target.user, password).await?;
            match result {
                AuthResult::Success => Ok(true),
                AuthResult::Failure {
                    remaining_methods, ..
                } if asks_through_keyboard_interactive(&remaining_methods) => {
                    keyboard_interactive(handle, &target.user, password).await
                }
                AuthResult::Failure { .. } => Ok(false),
            }
        }
        (Secret::Key { .. } | Secret::DeviceKey(_), Some(key)) => {
            let hash = handle.best_supported_rsa_hash().await?.flatten();
            Ok(handle
                .authenticate_publickey(&target.user, PrivateKeyWithHashAlg::new(key, hash))
                .await?
                .success())
        }
        (Secret::Key { .. } | Secret::DeviceKey(_), None) => {
            unreachable!("a key secret always has its key loaded")
        }
    }
}

/// Whether a host that refused the password method, with `remaining` the methods it still accepts,
/// asks for the password through keyboard-interactive instead: a host that signs in through PAM
/// offers no password method and asks for the same password that way. One that still accepts the
/// password method refused the password itself, which is not tried a second time, so a wrong
/// password counts against the host's limit once.
fn asks_through_keyboard_interactive(remaining: &MethodSet) -> bool {
    !remaining.contains(&MethodKind::Password)
        && remaining.contains(&MethodKind::KeyboardInteractive)
}

/// Signs in through keyboard-interactive, answering every hidden prompt with `password` and every
/// shown one (which asks for something other than a secret) with nothing; whether it was accepted.
async fn keyboard_interactive(
    handle: &mut Handle<HostCheck>,
    user: &str,
    password: &str,
) -> Result<bool, russh::Error> {
    let mut reply = handle
        .authenticate_keyboard_interactive_start(user, None)
        .await?;
    for _ in 0..KEYBOARD_INTERACTIVE_ROUNDS {
        match reply {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = keyboard_interactive_answers(
                    prompts.iter().map(|prompt| prompt.echo),
                    password,
                );
                reply = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await?;
            }
        }
    }
    Ok(false)
}

/// The answers to a keyboard-interactive round whose prompts show what is typed or not (`echoes`):
/// the password for a hidden one, nothing for a shown one.
fn keyboard_interactive_answers(echoes: impl Iterator<Item = bool>, password: &str) -> Vec<String> {
    echoes
        .map(|echo| {
            if echo {
                String::new()
            } else {
                password.to_owned()
            }
        })
        .collect()
}

impl Session {
    /// Whether the connection has ended.
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed() || *self.ended.borrow()
    }

    /// Waits until the connection has ended.
    #[cfg(test)]
    pub async fn closed(&self) {
        let mut ended = self.ended.clone();
        let _ = ended.wait_for(|ended| *ended).await;
    }

    /// Runs `command` on the host and collects what it printed and its exit status, within
    /// `COMMAND_TIMEOUT`.
    pub async fn run(&self, command: &str) -> Result<Output, RunError> {
        self.run_within(command, COMMAND_TIMEOUT).await
    }

    /// `run` within `limit`, for a command that may take longer than reading something does.
    pub async fn run_within(&self, command: &str, limit: Duration) -> Result<Output, RunError> {
        within(limit, async {
            let mut channel = self.handle.channel_open_session().await?;
            channel.exec(true, command).await?;
            let mut output = Output::default();
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => output.stdout.extend_from_slice(&data),
                    ChannelMsg::ExtendedData { data, ext: 1 } => {
                        output.stderr.extend_from_slice(&data)
                    }
                    ChannelMsg::ExitStatus { exit_status } => output.status = Some(exit_status),
                    _ => {}
                }
            }
            Ok(output)
        })
        .await
    }

    /// Starts `command` on the host on a channel of its own, whose data is the command's standard
    /// input and output; the start, not the run, is bounded by `COMMAND_TIMEOUT`. The tunnel
    /// reaches the daemon this way, through `octoboardd proxy`, rather than by forwarding to its
    /// socket, because Win32-OpenSSH forwards to neither a Unix socket nor a named pipe.
    pub async fn open(&self, command: &str) -> Result<Channel<Msg>, RunError> {
        within(COMMAND_TIMEOUT, async {
            let channel = self.handle.channel_open_session().await?;
            channel.exec(true, command).await?;
            Ok(channel)
        })
        .await
    }

    /// Opens a channel to `port` of `loopback` (a loopback address) on the host, within `limit`: the data of the
    /// channel is the connection's. OpenSSH on Unix confirms the channel once it has connected, so a port nothing
    /// listens on fails here, as `ForwardError::NothingListening`; Win32-OpenSSH confirms it all the same and then
    /// closes it (`WINDOWS_EARLY_CLOSE`).
    pub async fn forward(
        &self,
        loopback: &str,
        port: u16,
        limit: Duration,
    ) -> Result<Channel<Msg>, ForwardError> {
        let opened = tokio::time::timeout(
            limit,
            self.handle
                .channel_open_direct_tcpip(loopback, u32::from(port), "127.0.0.1", 0),
        )
        .await
        .map_err(|_| ForwardError::TimedOut(limit))?;
        opened.map_err(|err| match err {
            russh::Error::ChannelOpenFailure(ChannelOpenFailure::ConnectFailed) => {
                ForwardError::NothingListening
            }
            russh::Error::ChannelOpenFailure(
                ChannelOpenFailure::AdministrativelyProhibited
                | ChannelOpenFailure::UnknownChannelType,
            ) => ForwardError::Refused,
            other => ForwardError::Ssh(other),
        })
    }

    /// Whether something listens on `port` of `loopback` on the host, found by opening a channel to it (`forward`)
    /// and closing it again. On a Windows host (`windows`), a channel the host closes within `WINDOWS_EARLY_CLOSE`
    /// without having sent anything counts as nothing listening too; a server that waits to be spoken to keeps
    /// the channel open, so a probe there that succeeds takes that long.
    pub async fn probe(
        &self,
        loopback: &str,
        port: u16,
        limit: Duration,
        windows: bool,
    ) -> Result<(), ForwardError> {
        let mut channel = self.forward(loopback, port, limit).await?;
        if !windows {
            let _ = channel.close().await;
            return Ok(());
        }
        let closed = tokio::time::timeout(WINDOWS_EARLY_CLOSE, async {
            loop {
                match channel.wait().await {
                    None | Some(ChannelMsg::Eof | ChannelMsg::Close) => return true,
                    Some(ChannelMsg::Data { .. } | ChannelMsg::ExtendedData { .. }) => {
                        return false
                    }
                    Some(_) => {}
                }
            }
        })
        .await
        .unwrap_or(false);
        let _ = channel.close().await;
        if !closed {
            Ok(())
        } else if self.is_closed() {
            Err(ForwardError::Ssh(russh::Error::Disconnect))
        } else {
            Err(ForwardError::NothingListening)
        }
    }

    /// Ends the connection, which ends every command still running over it.
    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await;
    }
}

#[cfg(test)]
pub mod test_server;

#[cfg(test)]
mod tests {
    use super::test_server::{Behaviour, Methods, TestServer, ENCRYPTED_USER_KEY};
    use super::*;
    use crate::config_dir::test_support::Scratch;

    fn target(server: &TestServer, secret: Secret, pinned: Option<String>) -> Target {
        Target {
            host: "127.0.0.1".into(),
            port: server.port,
            user: "owner".into(),
            secret,
            pinned,
        }
    }

    fn password(text: &str) -> Secret {
        Secret::Password(text.into())
    }

    #[tokio::test]
    async fn a_host_with_no_pinned_key_is_refused_showing_its_fingerprint() {
        let server = TestServer::start(Behaviour::default()).await;
        let refused = connect(&target(&server, password("right"), None)).await;
        assert_eq!(
            refused.err(),
            Some(ConnectError::HostKeyUnknown {
                fingerprint: server.fingerprint.clone()
            })
        );
        assert_eq!(
            server.authentications(),
            0,
            "nothing is sent before the key is confirmed"
        );
    }

    #[tokio::test]
    async fn the_confirmed_fingerprint_connects_and_a_different_one_is_refused() {
        let server = TestServer::start(Behaviour::default()).await;
        let session = connect(&target(
            &server,
            password("right"),
            Some(server.fingerprint.clone()),
        ))
        .await
        .unwrap();
        assert_eq!(session.fingerprint, server.fingerprint);
        let other = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned();
        assert_eq!(
            connect(&target(&server, password("right"), Some(other.clone())))
                .await
                .err(),
            Some(ConnectError::HostKeyMismatch {
                expected: other,
                actual: server.fingerprint.clone(),
            })
        );
    }

    #[tokio::test]
    async fn a_wrong_password_is_an_authentication_failure() {
        let server = TestServer::start(Behaviour::default()).await;
        let refused = connect(&target(
            &server,
            password("wrong"),
            Some(server.fingerprint.clone()),
        ))
        .await;
        assert!(matches!(refused.err(), Some(ConnectError::AuthFailed(_))));
    }

    #[tokio::test]
    async fn a_key_file_authenticates_and_an_encrypted_one_asks_for_its_passphrase() {
        let server = TestServer::start(Behaviour::default()).await;
        let scratch = Scratch::new("remote-ssh", "keys");
        std::fs::create_dir_all(&scratch.0).unwrap();
        let plain = scratch.0.join("id_plain");
        std::fs::write(&plain, test_server::user_key_openssh()).unwrap();
        let encrypted = scratch.0.join("id_encrypted");
        std::fs::write(&encrypted, ENCRYPTED_USER_KEY).unwrap();
        let pinned = Some(server.fingerprint.clone());
        let key = |path: &Path, passphrase: Option<&str>| Secret::Key {
            path: path.to_owned(),
            passphrase: passphrase.map(str::to_owned),
        };

        connect(&target(&server, key(&plain, None), pinned.clone()))
            .await
            .unwrap();
        assert_eq!(
            connect(&target(&server, key(&encrypted, None), pinned.clone()))
                .await
                .err(),
            Some(ConnectError::PassphraseNeeded)
        );
        assert!(matches!(
            connect(&target(
                &server,
                key(&encrypted, Some("wrong")),
                pinned.clone()
            ))
            .await
            .err(),
            Some(ConnectError::KeyUnreadable(_))
        ));
        // The encrypted key is not one the server authorizes: decrypted, it is refused by the host.
        assert!(matches!(
            connect(&target(
                &server,
                key(&encrypted, Some("secret phrase")),
                pinned.clone()
            ))
            .await
            .err(),
            Some(ConnectError::AuthFailed(_))
        ));
        assert!(matches!(
            connect(&target(
                &server,
                key(&scratch.0.join("missing"), None),
                pinned
            ))
            .await
            .err(),
            Some(ConnectError::KeyUnreadable(_))
        ));
    }

    #[tokio::test]
    async fn a_host_that_is_not_listening_is_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let refused = connect(&Target {
            host: "127.0.0.1".into(),
            port,
            user: "owner".into(),
            secret: password("right"),
            pinned: None,
        })
        .await;
        assert!(matches!(refused.err(), Some(ConnectError::Unreachable(_))));
    }

    #[tokio::test]
    async fn a_command_s_output_and_exit_status_come_back() {
        let server = TestServer::start(Behaviour {
            address: Some((4, b"".to_vec(), b"no owner endpoint".to_vec())),
            ..Behaviour::default()
        })
        .await;
        let session = connect(&target(
            &server,
            password("right"),
            Some(server.fingerprint.clone()),
        ))
        .await
        .unwrap();
        let output = session.run("'octoboardd' address --json").await.unwrap();
        assert_eq!(output.status, Some(4));
        assert_eq!(output.stderr, b"no owner endpoint");
        assert_eq!(
            server.commands(),
            vec!["'octoboardd' address --json".to_owned()]
        );
    }

    #[test]
    fn a_leading_tilde_is_the_home_directory() {
        let home = expand_home("~");
        assert!(home.is_absolute(), "{home:?}");
        assert_eq!(expand_home("~/.ssh/id"), home.join(".ssh/id"));
        assert_eq!(expand_home("/etc/key"), PathBuf::from("/etc/key"));
        assert_eq!(expand_home("~other/key"), PathBuf::from("~other/key"));
    }

    #[tokio::test]
    async fn a_host_whose_key_does_not_match_is_sent_no_credential() {
        let server = TestServer::start(Behaviour::default()).await;
        let other = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned();
        let refused = connect(&target(&server, password("right"), Some(other))).await;
        assert!(matches!(
            refused.err(),
            Some(ConnectError::HostKeyMismatch { .. })
        ));
        assert_eq!(server.authentications(), 0);
    }

    #[tokio::test]
    async fn a_pinned_fingerprint_copied_without_its_prefix_or_padding_still_matches() {
        let server = TestServer::start(Behaviour::default()).await;
        let bare = server
            .fingerprint
            .trim_start_matches("SHA256:")
            .trim_end_matches('=')
            .to_owned();
        connect(&target(
            &server,
            password("right"),
            Some(format!(" {bare} ")),
        ))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_device_key_the_host_does_not_list_is_an_authentication_failure_by_its_kind() {
        let server = TestServer::start(Behaviour::default()).await;
        let key = super::super::device_key::generate().unwrap();
        let refused = connect(&target(
            &server,
            Secret::DeviceKey(Arc::new(key)),
            Some(server.fingerprint.clone()),
        ))
        .await;
        match refused.err() {
            Some(ConnectError::AuthFailed(detail)) => assert!(detail.contains("key"), "{detail}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_tilde_with_a_windows_separator_is_the_home_directory_too() {
        let home = expand_home("~");
        assert_eq!(expand_home("~\\.ssh\\id"), home.join(".ssh\\id"));
    }

    #[tokio::test]
    async fn a_host_that_asks_through_keyboard_interactive_is_answered_with_the_password() {
        let server = TestServer::start(Behaviour {
            methods: Methods::KeyboardInteractive,
            ..Behaviour::default()
        })
        .await;
        let pinned = Some(server.fingerprint.clone());
        connect(&target(&server, password("right"), pinned.clone()))
            .await
            .unwrap();
        assert!(matches!(
            connect(&target(&server, password("wrong"), pinned))
                .await
                .err(),
            Some(ConnectError::AuthFailed(_))
        ));
    }

    #[tokio::test]
    async fn a_refused_password_is_not_tried_again_through_keyboard_interactive() {
        // The host offers both methods: a wrong password is refused once, by the password method.
        let server = TestServer::start(Behaviour {
            methods: Methods::Both,
            ..Behaviour::default()
        })
        .await;
        let pinned = Some(server.fingerprint.clone());
        assert!(matches!(
            connect(&target(&server, password("wrong"), pinned.clone()))
                .await
                .err(),
            Some(ConnectError::AuthFailed(_))
        ));
        assert_eq!(server.keyboard_interactive_starts(), 0);
        assert_eq!(server.authentications(), 1);
        connect(&target(&server, password("right"), pinned))
            .await
            .unwrap();
        assert_eq!(server.keyboard_interactive_starts(), 0);
    }

    #[tokio::test]
    async fn a_host_offering_keyboard_interactive_alone_is_answered_through_it() {
        let server = TestServer::start(Behaviour {
            methods: Methods::KeyboardInteractive,
            ..Behaviour::default()
        })
        .await;
        connect(&target(
            &server,
            password("right"),
            Some(server.fingerprint.clone()),
        ))
        .await
        .unwrap();
        assert_eq!(server.keyboard_interactive_starts(), 1);
    }

    #[test]
    fn keyboard_interactive_stands_in_only_where_the_password_method_is_gone() {
        let set = |kinds: &[MethodKind]| MethodSet::from(kinds);
        assert!(asks_through_keyboard_interactive(&set(&[
            MethodKind::PublicKey,
            MethodKind::KeyboardInteractive
        ])));
        assert!(!asks_through_keyboard_interactive(&set(&[
            MethodKind::Password,
            MethodKind::KeyboardInteractive
        ])));
        assert!(!asks_through_keyboard_interactive(&set(&[
            MethodKind::PublicKey
        ])));
    }

    #[test]
    fn a_keyboard_interactive_round_gets_the_password_for_its_hidden_prompts_alone() {
        assert_eq!(
            keyboard_interactive_answers([true, false, false].into_iter(), "pw"),
            ["", "pw", "pw"]
        );
        assert!(keyboard_interactive_answers(std::iter::empty(), "pw").is_empty());
    }

    #[tokio::test]
    async fn a_command_the_host_never_finishes_runs_into_the_time_limit() {
        let server = TestServer::start(Behaviour {
            hang: vec!["sleep forever".into()],
            ..Behaviour::default()
        })
        .await;
        let session = connect(&target(
            &server,
            password("right"),
            Some(server.fingerprint.clone()),
        ))
        .await
        .unwrap();
        let limit = Duration::from_millis(200);
        match session.run_within("sleep forever", limit).await {
            Err(RunError::TimedOut(after)) => assert_eq!(after, limit),
            other => panic!("{other:?}"),
        }
        // The connection is still good for what answers.
        assert_eq!(session.run("echo").await.unwrap().status, Some(0));
    }
}
