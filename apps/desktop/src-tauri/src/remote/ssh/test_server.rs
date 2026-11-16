//! An SSH server in the test process standing in for a remote host running `octoboardd`: it accepts
//! the password `right` and one user key, answers `address --json` as it is told to, says `127`
//! (not found) for the commands it is told are missing, and echoes the bytes of a `proxy` run back,
//! so the tunnel's carrying can be seen. It answers the shell probe as the shell it is told to be
//! (a POSIX one unless told otherwise) and takes the commands in that shell's forms. Given a home
//! directory, it runs the other `sh -c` commands there for real and also accepts the keys its
//! `.ssh/authorized_keys` lists, so installing a device key can be seen working. Its connections
//! can be dropped while it keeps listening, as a network that went away and came back. It can stand
//! in for a host that signs in through keyboard-interactive alone, as PAM hosts do, and for one
//! that never finishes some commands. It refuses a `direct-tcpip` channel unless it is told to carry
//! them, to a listener of the test, and then reports a port nothing answers at (or a loopback address
//! it is told nothing listens on) as a connection that failed, as OpenSSH on Unix does, or, told to
//! stand in for Win32-OpenSSH, confirms the channel and then closes it.

use std::sync::{Arc, Mutex};

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::ssh_key::LineEnding;
use russh::keys::{HashAlg, PrivateKey};
use russh::server::{self, Auth, Msg, Response as Answers, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId, ChannelOpenFailure, MethodKind, MethodSet};

use crate::remote::octoboardd::{Shell, SHELL_PROBE};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// A user key encrypted with the passphrase `secret phrase`, made by `ssh-keygen` for these tests
/// alone; the server does not authorize it.
pub const ENCRYPTED_USER_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABAZCyHPTl
fsBMZkJtLYxMHmAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIFQfDxrDI6S0/f7s
tlaDCEH9v1pVHoNvCsA5+F805e/aAAAAkLdW51u5bHorBU+5dI6J+x+mRbPRoM8+OVYMvY
q5ALMAGqnMsO9dgGLykSryYrXebQxGPFSXF/ng8e3zxbJZEimZjiMqOkDaeXVWbeaqd30o
JOtXJo3cJIeTkcBrh7/d+q8yxkmQpsa5L85PqwEYUUbmwi3AfujSH1UZKmu7nkuy0sMoQG
bxQui/Lqr0u2MGGA==
-----END OPENSSH PRIVATE KEY-----
";

fn host_key() -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[7; 32]))
}

fn user_key() -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[9; 32]))
}

/// The user key the server authorizes, as an OpenSSH private key file holds it.
pub fn user_key_openssh() -> String {
    user_key().to_openssh(LineEnding::LF).unwrap().to_string()
}

/// How the stand-in host answers.
#[derive(Clone, Default)]
pub struct Behaviour {
    /// What `address --json` exits with and prints on stdout and stderr; `None` answers with the
    /// owner key `aa` at the application's own generation.
    pub address: Option<(u32, Vec<u8>, Vec<u8>)>,
    /// The `octoboardd` words (as the command line begins) the shell does not find.
    pub missing: Vec<String>,
    /// The remote user's home directory: `sh -c` commands run with it as `HOME`, and the keys of
    /// its `.ssh/authorized_keys` are accepted.
    pub home: Option<std::path::PathBuf>,
    /// The shell the host's SSH server runs a command through, as its answer to the probe says.
    pub shell: Shell,
    /// What `start`, `update` and `restart` exit with and print on stdout and stderr, where they
    /// are told to fail; `None` lets them succeed silently.
    pub action: Option<(u32, Vec<u8>, Vec<u8>)>,
    /// The authentication methods that take the password.
    pub methods: Methods,
    /// The commands (whole, as run) the host starts and never finishes.
    pub hang: Vec<String>,
    /// The port of this computer a `direct-tcpip` channel (to whatever port) is carried to; `None`
    /// refuses them as a host that does not allow forwarding does.
    pub forward_to: Option<u16>,
    /// The loopback addresses a `direct-tcpip` channel asks for that nothing listens on, as for a server
    /// listening on the other family alone.
    pub not_listening_on: Vec<String>,
    /// How long the host takes to find that nothing listens where a `direct-tcpip` channel asks, as a
    /// Windows host does while its connect attempt is retried. Channels are answered each on its own.
    pub refusal_delay: std::time::Duration,
    /// Whether a `direct-tcpip` channel to where nothing listens is confirmed and then closed, as
    /// Win32-OpenSSH does, rather than refused.
    pub confirms_then_closes: bool,
}

/// How the host takes the password.
#[derive(Clone, Copy, Default, PartialEq)]
pub enum Methods {
    /// The password method alone.
    #[default]
    Password,
    /// No password method: the password is asked for through keyboard-interactive.
    KeyboardInteractive,
    /// Keyboard-interactive beside the password method, both taking the password.
    Both,
}

/// The authentication methods the host offers.
fn methods(behaviour: &Behaviour) -> MethodSet {
    match behaviour.methods {
        Methods::KeyboardInteractive => {
            MethodSet::from(&[MethodKind::PublicKey, MethodKind::KeyboardInteractive][..])
        }
        Methods::Both => MethodSet::from(
            &[
                MethodKind::Password,
                MethodKind::PublicKey,
                MethodKind::KeyboardInteractive,
            ][..],
        ),
        Methods::Password => MethodSet::from(&[MethodKind::Password, MethodKind::PublicKey][..]),
    }
}

/// Whether `command` runs `octoboardd` with `arguments`, in any shell's form.
fn runs(command: &str, arguments: &str) -> bool {
    [
        format!(" {arguments}"),
        format!(" {arguments}) else (exit 127)"),
        format!(" {arguments}; exit $LASTEXITCODE }} else {{ exit 127 }}"),
    ]
    .iter()
    .any(|tail| command.ends_with(tail))
}

/// The default answer of `address --json` with `key`.
pub fn answer(key: &str) -> (u32, Vec<u8>, Vec<u8>) {
    let line = format!(
        "{{\"owner_key\":\"{key}\",\"generation\":{},\"daemon_version\":\"1.0.0\"}}\n",
        crate::remote::octoboardd::LOCAL_GENERATION
    );
    (0, line.into_bytes(), Vec::new())
}

#[derive(Default)]
struct Shared {
    behaviour: Behaviour,
    commands: Vec<String>,
    authentications: usize,
    keyboard_interactive_starts: usize,
    connections: usize,
    /// The host and port each `direct-tcpip` channel asked for.
    forwarded: Vec<(String, u32)>,
    sessions: Vec<JoinHandle<()>>,
}

pub struct TestServer {
    pub port: u16,
    pub fingerprint: String,
    shared: Arc<Mutex<Shared>>,
    accept: JoinHandle<()>,
}

impl TestServer {
    pub async fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let key = host_key();
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let methods = methods(&behaviour);
        let config = Arc::new(server::Config {
            keys: vec![key],
            methods,
            auth_rejection_time: std::time::Duration::from_millis(10),
            auth_rejection_time_initial: Some(std::time::Duration::ZERO),
            ..Default::default()
        });
        let shared = Arc::new(Mutex::new(Shared {
            behaviour,
            ..Shared::default()
        }));
        let accepting = shared.clone();
        let accept = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let handler = Host {
                    shared: accepting.clone(),
                    direct: Default::default(),
                };
                let config = config.clone();
                // The host's side of the connection runs on one end of a pipe, the client's TCP
                // stream relayed to the other by a task of its own: aborting that task drops the
                // client's stream, as a network gone away does.
                let (near, far) = tokio::io::duplex(64 * 1024);
                tokio::spawn(async move {
                    if let Ok(running) = server::run_stream(config, far, handler).await {
                        let _ = running.await;
                    }
                });
                let session = tokio::spawn(async move {
                    let (mut stream, mut near) = (stream, near);
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut near).await;
                });
                let mut shared = accepting.lock().unwrap();
                shared.connections += 1;
                shared.sessions.push(session);
            }
        });
        Self {
            port,
            fingerprint,
            shared,
            accept,
        }
    }

    /// Every command run so far, in order.
    pub fn commands(&self) -> Vec<String> {
        self.shared.lock().unwrap().commands.clone()
    }

    /// How many authentications were attempted.
    pub fn authentications(&self) -> usize {
        self.shared.lock().unwrap().authentications
    }

    /// How many keyboard-interactive logins were begun.
    pub fn keyboard_interactive_starts(&self) -> usize {
        self.shared.lock().unwrap().keyboard_interactive_starts
    }

    /// How many SSH connections were accepted.
    pub fn connections(&self) -> usize {
        self.shared.lock().unwrap().connections
    }

    /// The host and port each `direct-tcpip` channel asked for, in order.
    pub fn forwarded(&self) -> Vec<(String, u32)> {
        self.shared.lock().unwrap().forwarded.clone()
    }

    /// Changes the port `direct-tcpip` channels are carried to from now on.
    pub fn set_forward_to(&self, port: Option<u16>) {
        self.shared.lock().unwrap().behaviour.forward_to = port;
    }

    /// Changes the loopback addresses nothing listens on from now on.
    pub fn set_not_listening_on(&self, hosts: &[&str]) {
        self.shared.lock().unwrap().behaviour.not_listening_on =
            hosts.iter().map(|host| (*host).to_owned()).collect();
    }

    /// Changes how the host answers from now on.
    pub fn set_address(&self, address: Option<(u32, Vec<u8>, Vec<u8>)>) {
        self.shared.lock().unwrap().behaviour.address = address;
    }

    /// Drops every connection, abruptly, the listener staying.
    pub fn drop_connections(&self) {
        for session in self.shared.lock().unwrap().sessions.drain(..) {
            session.abort();
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.accept.abort();
        self.drop_connections();
    }
}

struct Host {
    shared: Arc<Mutex<Shared>>,
    /// The `direct-tcpip` channels, whose data goes to their own stream and is not echoed.
    direct: Arc<Mutex<std::collections::HashSet<ChannelId>>>,
}

impl Host {
    fn respond(&self, command: &str) -> Response {
        let mut shared = self.shared.lock().unwrap();
        shared.commands.push(command.to_owned());
        if shared.behaviour.hang.iter().any(|hung| hung == command) {
            return Response::Hang;
        }
        if command == SHELL_PROBE {
            let printed = match shared.behaviour.shell {
                Shell::Posix => "%OS% :OS\n",
                Shell::Cmd => "Windows_NT $env:OS\r\n",
                Shell::PowerShell => "%OS%\r\nWindows_NT\r\n",
            };
            return Response::Exit(0, printed.as_bytes().to_vec(), Vec::new());
        }
        if shared
            .behaviour
            .missing
            .iter()
            .any(|word| command.starts_with(&format!("{word} ")))
        {
            return Response::Exit(127, Vec::new(), b"sh: command not found".to_vec());
        }
        if runs(command, "proxy") {
            return Response::Echo;
        }
        if runs(command, "address --json") {
            let (status, stdout, stderr) = shared
                .behaviour
                .address
                .clone()
                .unwrap_or_else(|| answer("aa"));
            return Response::Exit(status, stdout, stderr);
        }
        if let Some((status, stdout, stderr)) = shared.behaviour.action.clone() {
            if ["start", "update", "restart"]
                .iter()
                .any(|action| runs(command, action))
            {
                return Response::Exit(status, stdout, stderr);
            }
        }
        if let (Some(home), Some(_)) = (&shared.behaviour.home, command.strip_prefix("sh -c ")) {
            let _spawning = crate::spawning();
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .env("HOME", home)
                .output()
                .unwrap();
            let status = output.status.code().map_or(255, |code| code as u32);
            return Response::Exit(status, output.stdout, output.stderr);
        }
        Response::Exit(0, Vec::new(), Vec::new())
    }

    /// Whether `key` is listed in the home directory's `.ssh/authorized_keys`.
    fn authorized(&self, key: &russh::keys::PublicKey) -> bool {
        let Some(home) = self.shared.lock().unwrap().behaviour.home.clone() else {
            return false;
        };
        let Ok(listed) = std::fs::read_to_string(home.join(".ssh/authorized_keys")) else {
            return false;
        };
        let offered = key.to_openssh().unwrap();
        let Some(body) = offered.split_whitespace().nth(1) else {
            return false;
        };
        listed
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(body))
    }
}

enum Response {
    Exit(u32, Vec<u8>, Vec<u8>),
    Echo,
    /// Nothing, ever.
    Hang,
}

impl server::Handler for Host {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let mut shared = self.shared.lock().unwrap();
        shared.authentications += 1;
        Ok(
            if shared.behaviour.methods != Methods::KeyboardInteractive
                && user == "owner"
                && password == "right"
            {
                Auth::Accept
            } else {
                // As OpenSSH does, every method is still offered after a refused password (russh
                // would take the password method out).
                Auth::Reject {
                    proceed_with_methods: Some(methods(&shared.behaviour)),
                    partial_success: false,
                }
            },
        )
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _submethods: &str,
        response: Option<Answers<'a>>,
    ) -> Result<Auth, Self::Error> {
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.behaviour.methods == Methods::Password {
                return Ok(Auth::reject());
            }
            if response.is_none() {
                shared.keyboard_interactive_starts += 1;
            }
        }
        let Some(mut answers) = response else {
            // As PAM asks: who it is shown, then the password, hidden.
            return Ok(Auth::Partial {
                name: "".into(),
                instructions: "".into(),
                prompts: vec![("Login: ".into(), true), ("Password: ".into(), false)].into(),
            });
        };
        self.shared.lock().unwrap().authentications += 1;
        let (shown, password) = (answers.next(), answers.next());
        Ok(
            if user == "owner"
                && shown.as_deref() == Some(&b""[..])
                && password.as_deref() == Some(&b"right"[..])
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        self.shared.lock().unwrap().authentications += 1;
        Ok(
            if user == "owner"
                && (key.public_key.key_data() == user_key().public_key().key_data()
                    || self.authorized(key.public_key))
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = channel;
        reply.accept().await;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let (to, listening, confirms_then_closes, refusal_delay) = {
            let mut shared = self.shared.lock().unwrap();
            shared
                .forwarded
                .push((host_to_connect.to_owned(), port_to_connect));
            let behaviour = &shared.behaviour;
            (
                behaviour.forward_to,
                !behaviour
                    .not_listening_on
                    .iter()
                    .any(|host| host == host_to_connect),
                behaviour.confirms_then_closes,
                behaviour.refusal_delay,
            )
        };
        let Some(to) = to else {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };
        // Answered on a task of its own, so that a channel the host is slow to answer holds up no other.
        let direct = self.direct.clone();
        tokio::spawn(async move {
            // Connected before the channel is confirmed, as OpenSSH does.
            let connected = if listening {
                tokio::net::TcpStream::connect(("127.0.0.1", to)).await.ok()
            } else {
                tokio::time::sleep(refusal_delay).await;
                None
            };
            let Some(mut target) = connected else {
                if confirms_then_closes {
                    direct.lock().unwrap().insert(channel.id());
                    reply.accept().await;
                    let _ = channel.eof().await;
                    let _ = channel.close().await;
                } else {
                    reply.reject(ChannelOpenFailure::ConnectFailed).await;
                }
                return;
            };
            direct.lock().unwrap().insert(channel.id());
            reply.accept().await;
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
        });
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        match self.respond(&String::from_utf8_lossy(data)) {
            Response::Echo | Response::Hang => {}
            Response::Exit(status, stdout, stderr) => {
                if !stdout.is_empty() {
                    session.data(channel, stdout)?;
                }
                if !stderr.is_empty() {
                    session.extended_data(channel, 1, stderr)?;
                }
                session.exit_status_request(channel, status)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.direct.lock().unwrap().contains(&channel) {
            return Ok(());
        }
        session.data(channel, data.to_vec())?;
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.direct.lock().unwrap().contains(&channel) {
            return Ok(());
        }
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}
