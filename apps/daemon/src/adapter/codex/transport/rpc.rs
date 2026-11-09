//! A JSON-RPC connection to a session's private Codex app server: the stream it runs over, how a
//! client reaches the server, the handshake, requests and their failures.

use std::io;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{self, Message, WebSocket};

use super::super::app_server;
use crate::agents::json_rpc;
use crate::platform::ipc::Connection;
use crate::terminal::server_host::ServerEndpoint;

/// How long one request may take, from sending it to its reply.
pub(super) const RPC_TIMEOUT: Duration = Duration::from_secs(5);
/// The ceiling on one message of the server's.
const MESSAGE_LIMIT: usize = 4 * 1024 * 1024;

/// The environment variable the terminal reads the capability token from.
pub(super) const TOKEN_VARIABLE: &str = "OCTOBOARD_CODEX_REMOTE_TOKEN";

/// How a client reaches a server that listens.
#[derive(Clone)]
pub(super) enum Endpoint {
    Socket(std::path::PathBuf),
    Loopback { port: u16, token: String },
}

impl Endpoint {
    /// How a client reaches the server listening on `endpoint`, on `port` where it reported one
    /// on starting; `None` for a loopback endpoint whose port is known from neither.
    pub(super) fn reached_at(endpoint: ServerEndpoint, port: Option<u16>) -> Option<Self> {
        match endpoint {
            ServerEndpoint::Socket(path) => Some(Self::Socket(path)),
            ServerEndpoint::Loopback {
                token,
                port: listed,
            } => Some(Self::Loopback {
                port: port.or(listed)?,
                token,
            }),
        }
    }

    pub(super) fn connect(&self) -> Result<Rpc, RpcFailure> {
        use tungstenite::client::IntoClientRequest;
        let transport = |error: io::Error| RpcFailure::Transport(error.to_string());
        let config = tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(MESSAGE_LIMIT),
            max_frame_size: Some(MESSAGE_LIMIT),
            ..Default::default()
        };
        let (stream, request) = match self {
            Self::Socket(path) => (
                Connection::socket(path).map_err(transport)?,
                "ws://localhost/rpc".into_client_request()?,
            ),
            Self::Loopback { port, token } => {
                let stream = Connection::loopback(*port, RPC_TIMEOUT).map_err(transport)?;
                let mut request = format!("ws://127.0.0.1:{port}/").into_client_request()?;
                let bearer = format!("Bearer {token}")
                    .parse()
                    .map_err(|_| RpcFailure::Transport("an unusable token".to_string()))?;
                request
                    .headers_mut()
                    .insert(tungstenite::http::header::AUTHORIZATION, bearer);
                (stream, request)
            }
        };
        stream.set_timeouts(Some(RPC_TIMEOUT)).map_err(transport)?;
        let (socket, _) = tungstenite::client::client_with_config(request, stream, Some(config))
            .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        let mut rpc = Rpc {
            socket,
            next_id: 0,
            deadline: Instant::now() + RPC_TIMEOUT,
        };
        let mut params = app_server::initialize_params();
        params["capabilities"] = json!({"experimentalApi": true});
        rpc.request("initialize", params)?;
        rpc.write(app_server::initialized())?;
        Ok(rpc)
    }

    /// What the terminal is given to attach to the server.
    pub(super) fn client_args(&self) -> Vec<String> {
        match self {
            Self::Socket(path) => vec!["--remote".into(), format!("unix://{}", path.display())],
            Self::Loopback { port, .. } => vec![
                "--remote".into(),
                format!("ws://127.0.0.1:{port}"),
                "--remote-auth-token-env".into(),
                TOKEN_VARIABLE.into(),
            ],
        }
    }

    /// What the terminal's environment is given to attach to the server.
    pub(super) fn client_env(&self) -> Vec<(String, String)> {
        match self {
            Self::Socket(_) => Vec::new(),
            Self::Loopback { token, .. } => vec![(TOKEN_VARIABLE.into(), token.clone())],
        }
    }
}

#[derive(Debug)]
pub(super) enum RpcFailure {
    Transport(String),
    Rejected {
        code: i64,
        /// What the error says with its `data`, for showing.
        message: String,
    },
}

impl std::fmt::Display for RpcFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(message) => formatter.write_str(message),
            Self::Rejected { code, message } => {
                write!(formatter, "Codex RPC error {code}: {message}")
            }
        }
    }
}

impl std::error::Error for RpcFailure {}

impl From<tungstenite::Error> for RpcFailure {
    fn from(error: tungstenite::Error) -> Self {
        Self::Transport(error.to_string())
    }
}

pub(super) struct Rpc {
    socket: WebSocket<Connection>,
    next_id: u64,
    deadline: Instant,
}

impl Rpc {
    /// Holds the connection's next reads and writes to what is left of the deadline.
    fn remaining(&mut self) -> Result<(), RpcFailure> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RpcFailure::Transport(
                "the Codex RPC deadline elapsed".to_string(),
            ));
        }
        self.socket
            .get_mut()
            .set_timeouts(Some(remaining))
            .map_err(|error| RpcFailure::Transport(error.to_string()))
    }

    fn write(&mut self, value: Value) -> Result<(), RpcFailure> {
        self.remaining()?;
        self.socket.send(Message::Text(value.to_string()))?;
        Ok(())
    }

    /// Reads one message.
    fn read_message(&mut self) -> Result<Option<Value>, RpcFailure> {
        let message = self.socket.read()?;
        self.take(message)
    }

    /// The next message within `wait`, or `None` when none came. A read that times out leaves the
    /// connection usable: tungstenite keeps what it read of a frame.
    pub(super) fn next_message(&mut self, wait: Duration) -> Result<Option<Value>, RpcFailure> {
        self.socket
            .get_mut()
            .set_timeouts(Some(wait))
            .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        match self.socket.read() {
            Ok(message) => self.take(message),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// What `message` says; `None` for a frame that carries no message. A request of the server's
    /// is left unanswered: Codex sends approval and other requests to the connections subscribed
    /// to the thread, which a connection that queued a message is not, but one that loaded the
    /// thread again (`thread/resume`) is until it closes. Left unanswered, such a request waits for
    /// the terminal, which shows it once the person opens that task (Codex 0.166.0); an answer of
    /// this connection would decide for them.
    fn take(&mut self, message: Message) -> Result<Option<Value>, RpcFailure> {
        match message {
            Message::Text(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|error| RpcFailure::Transport(error.to_string())),
            Message::Close(_) => Err(RpcFailure::Transport(
                "Codex closed the queue connection".to_string(),
            )),
            Message::Ping(_) => {
                self.socket.flush()?;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcFailure> {
        self.deadline = Instant::now() + RPC_TIMEOUT;
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            self.remaining()?;
            let Some(value) = self.read_message()? else {
                continue;
            };
            if value.get("method").is_some() || !json_rpc::is_reply_to(&value, id as i64) {
                continue;
            }
            if let Some(message) = json_rpc::error_detail(&value) {
                return Err(RpcFailure::Rejected {
                    code: json_rpc::error_code(&value).unwrap_or(-32603),
                    message,
                });
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| RpcFailure::Transport("Codex omitted an RPC result".to_string()));
        }
    }
}
