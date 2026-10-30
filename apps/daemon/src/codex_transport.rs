//! A private Codex app server, and independent messages submitted over its Unix WebSocket.

use std::collections::HashMap;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{self, Message, WebSocket};

use crate::adapter::codex::PrivateServerPlan;
use crate::ptyio::PartialWrite;

const RPC_TIMEOUT: Duration = Duration::from_secs(5);
const START_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_GRACE: Duration = Duration::from_secs(2);

/// Acceptance could not be established after the queue request was sent. Retrying could duplicate it.
#[derive(Debug)]
pub struct UncertainSubmission(pub String);

impl std::fmt::Display for UncertainSubmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UncertainSubmission {}

struct SocketDirectory(PathBuf);

impl SocketDirectory {
    fn create() -> io::Result<Self> {
        // macOS limits Unix socket paths to 104 bytes, independently of TMPDIR's length.
        let path = Path::new("/tmp").join(format!("octo-cdx-{}", uuid::Uuid::new_v4().simple()));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    fn socket(&self) -> PathBuf {
        self.0.join("rpc")
    }
}

impl Drop for SocketDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.socket());
        let _ = std::fs::remove_dir(&self.0);
    }
}

struct ServerProcess {
    child: Child,
    crash_cleanup: Option<crate::crash_cleanup::Entry>,
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let pid = self.child.id() as libc::pid_t;
        // Keep the leader unreaped until its session has been swept, so its id cannot be reused.
        if crate::process_session::exited_unreaped(pid) == Some(false) {
            crate::process_session::signal_group(pid, libc::SIGTERM);
            let deadline = Instant::now() + STOP_GRACE;
            while crate::process_session::exited_unreaped(pid) == Some(false)
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if crate::process_session::exited_unreaped(pid).is_some() {
            crate::process_session::kill_group(pid);
            crate::process_session::kill_members(pid).log(pid, "Codex app server");
        }
        self.crash_cleanup.take();
        let _ = self.child.wait();
    }
}

pub struct CodexTransport {
    server: Mutex<Option<ServerProcess>>,
    socket_directory: SocketDirectory,
    thread: Mutex<Option<String>>,
    resume_thread: Option<String>,
    operation: Mutex<()>,
}

impl CodexTransport {
    pub fn start(
        binary: &str,
        plan: &PrivateServerPlan,
        environment: &HashMap<String, String>,
        cwd: &Path,
    ) -> Result<Self> {
        let socket_directory =
            SocketDirectory::create().context("creating the Codex socket directory")?;
        let mut server_args = plan.args.clone();
        let spawn = |args: &[String]| -> Result<ServerProcess> {
            let mut command = Command::new(binary);
            command
                .args(args)
                .args(["app-server", "--listen"])
                .arg(format!("unix://{}", socket_directory.socket().display()))
                .env_clear()
                .envs(environment)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            crate::process_session::lead_session(&mut command);
            let child = command
                .spawn()
                .context("starting the private Codex app server")?;
            let crash_cleanup = crate::crash_cleanup::register(child.id(), true);
            Ok(ServerProcess {
                child,
                crash_cleanup,
            })
        };
        let mut server = spawn(&server_args)?;
        let mut rpc = wait_for_server(&socket_directory.socket(), &server)?;
        if plan.resume_thread.is_some() {
            // A persistent remote resume intentionally ignores hook-trust bypass. Trust only the
            // generated command using Codex's own fingerprint, as invocation-local overrides.
            let metadata = rpc.request("hooks/list", json!({"cwds":[cwd]}))?;
            let trusts = own_hook_trust(&metadata, &plan.hook_command)?;
            drop(rpc);
            drop(server);
            let _ = std::fs::remove_file(socket_directory.socket());
            server_args.extend(trusts);
            server = spawn(&server_args)?;
            rpc = wait_for_server(&socket_directory.socket(), &server)?;
        }
        drop(rpc);
        Ok(Self {
            server: Mutex::new(Some(server)),
            socket_directory,
            thread: Mutex::new(None),
            resume_thread: plan.resume_thread.clone(),
            operation: Mutex::new(()),
        })
    }

    pub fn endpoint(&self) -> String {
        format!("unix://{}", self.socket_directory.socket().display())
    }

    pub fn stop(&self) {
        self.server
            .lock()
            .expect("Codex sidecar lock poisoned")
            .take();
        let _ = std::fs::remove_file(self.socket_directory.socket());
        let _ = std::fs::remove_dir(&self.socket_directory.0);
    }

    pub fn send(&self, text: &str, logical_id: &str) -> Result<(), PartialWrite> {
        let _operation = self.operation.lock().expect("Codex message lock poisoned");
        let before_add = |error: RpcFailure| PartialWrite {
            written: 0,
            error: io::Error::other(error),
        };
        let mut rpc = Rpc::connect(&self.socket_directory.socket()).map_err(before_add)?;
        let thread_id = {
            let mut thread = self.thread.lock().expect("Codex thread lock poisoned");
            let loaded = rpc
                .request("thread/loaded/list", json!({}))
                .map_err(before_add)?;
            let identified =
                unique_thread(&loaded, thread.as_deref().or(self.resume_thread.as_deref()))
                    .map_err(|error| PartialWrite { written: 0, error })?;
            *thread = Some(identified.clone());
            identified
        };
        let added = rpc.request(
            "thread/queue/add",
            json!({
                "threadId":thread_id,
                "input":[{"type":"text","text":text,"text_elements":[]}],
                "clientUserMessageId":logical_id,
            }),
        );
        let added = match added {
            Ok(added) => added,
            // A complete rejection is different from losing the response after acceptance.
            Err(error @ RpcFailure::Rejected { .. }) => return Err(before_add(error)),
            Err(error) => return Err(uncertain(error.to_string())),
        };
        let Some(id) = added
            .pointer("/queuedSubmission/id")
            .and_then(Value::as_str)
        else {
            return Err(uncertain(
                "Codex acknowledged a queue request without its submission id",
            ));
        };
        match rpc.request(
            "thread/queue/start",
            json!({"threadId":thread_id,"queuedSubmissionId":id}),
        ) {
            Ok(_) => Ok(()),
            Err(RpcFailure::Rejected {
                code: -32600,
                message,
            }) if message == "thread already has an active or pending turn"
                || message == format!("queued submission not found: {id}") =>
            {
                Ok(())
            }
            Err(error) => Err(uncertain(format!(
                "Codex accepted the queued message but its start could not be confirmed: {error}"
            ))),
        }
    }
}

fn uncertain(detail: impl Into<String>) -> PartialWrite {
    PartialWrite {
        written: 1,
        error: io::Error::other(UncertainSubmission(detail.into())),
    }
}

fn unique_thread(value: &Value, expected: Option<&str>) -> io::Result<String> {
    let ids = value.get("data").and_then(Value::as_array);
    match ids.map(Vec::as_slice) {
        Some([id]) if value.get("nextCursor").is_none_or(Value::is_null) => {
            if let Some(id) = id
                .as_str()
                .filter(|id| expected.is_none_or(|expected| expected == *id))
            {
                return Ok(id.to_string());
            }
        }
        _ => {}
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "the private Codex server has no uniquely identified session",
    ))
}

fn own_hook_trust(metadata: &Value, command: &str) -> Result<Vec<String>> {
    let mut entries_to_trust = Vec::new();
    let entries = metadata
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Codex omitted hook metadata"))?;
    for hook in entries
        .iter()
        .filter_map(|entry| entry.get("hooks").and_then(Value::as_array))
        .flatten()
    {
        if hook.get("handlerType").and_then(Value::as_str) != Some("command")
            || hook.get("command").and_then(Value::as_str) != Some(command)
            || hook.get("source").and_then(Value::as_str) != Some("sessionFlags")
        {
            continue;
        }
        let key = hook
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Codex omitted the generated hook key"))?;
        let hash = hook
            .get("currentHash")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Codex omitted the generated hook fingerprint"))?;
        entries_to_trust.push(format!("{}={{trusted_hash={}}}", json!(key), json!(hash)));
    }
    if entries_to_trust.is_empty() {
        return Err(anyhow!(
            "Codex did not identify the generated session hooks"
        ));
    }
    Ok(vec![
        "-c".to_string(),
        format!("hooks.state={{{}}}", entries_to_trust.join(",")),
    ])
}

fn wait_for_server(path: &Path, server: &ServerProcess) -> Result<Rpc> {
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if crate::process_session::exited_unreaped(server.child.id() as libc::pid_t) != Some(false)
        {
            return Err(anyhow!(
                "the private Codex app server exited before becoming ready"
            ));
        }
        if path.exists() {
            return Rpc::connect(path).context("initializing the private Codex app server");
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "the private Codex app server did not create its socket in time"
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[derive(Debug)]
enum RpcFailure {
    Transport(String),
    Rejected { code: i64, message: String },
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

struct Rpc {
    socket: WebSocket<UnixStream>,
    next_id: u64,
    deadline: Instant,
}

impl Rpc {
    fn connect(path: &Path) -> Result<Self, RpcFailure> {
        let deadline = Instant::now() + RPC_TIMEOUT;
        let stream =
            UnixStream::connect(path).map_err(|error| RpcFailure::Transport(error.to_string()))?;
        stream
            .set_read_timeout(Some(RPC_TIMEOUT))
            .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        stream
            .set_write_timeout(Some(RPC_TIMEOUT))
            .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        let config = tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(4 * 1024 * 1024),
            max_frame_size: Some(4 * 1024 * 1024),
            ..Default::default()
        };
        let (socket, _) =
            tungstenite::client::client_with_config("ws://localhost/rpc", stream, Some(config))
                .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        let mut rpc = Self {
            socket,
            next_id: 0,
            deadline,
        };
        rpc.request(
            "initialize",
            json!({
                "clientInfo":{"name":"octoboard","version":env!("CARGO_PKG_VERSION")},
                "capabilities":{"experimentalApi":true},
            }),
        )?;
        rpc.write(json!({"jsonrpc":"2.0","method":"initialized"}))?;
        Ok(rpc)
    }

    fn remaining(&mut self) -> Result<(), RpcFailure> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RpcFailure::Transport(
                "the Codex RPC deadline elapsed".to_string(),
            ));
        }
        self.socket
            .get_mut()
            .set_read_timeout(Some(remaining))
            .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        self.socket
            .get_mut()
            .set_write_timeout(Some(remaining))
            .map_err(|error| RpcFailure::Transport(error.to_string()))?;
        Ok(())
    }

    fn write(&mut self, value: Value) -> Result<(), RpcFailure> {
        self.remaining()?;
        self.socket.send(Message::Text(value.to_string()))?;
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcFailure> {
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            self.remaining()?;
            match self.socket.read()? {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|error| RpcFailure::Transport(error.to_string()))?;
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        if let Some(error) = value.get("error") {
                            return Err(RpcFailure::Rejected {
                                code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                                message: error
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("unknown error")
                                    .to_string(),
                            });
                        }
                        return value.get("result").cloned().ok_or_else(|| {
                            RpcFailure::Transport("Codex omitted an RPC result".to_string())
                        });
                    }
                    if value.get("method").is_some() && value.get("id").is_some() {
                        self.write(json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32601,"message":"unsupported request on the Octoboard queue connection"}}))?;
                    }
                }
                Message::Close(_) => {
                    return Err(RpcFailure::Transport(
                        "Codex closed the queue connection".to_string(),
                    ))
                }
                Message::Ping(_) => self.socket.flush()?,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn test_transport() -> (CodexTransport, std::os::unix::net::UnixListener) {
    let directory = SocketDirectory::create().unwrap();
    let listener = std::os::unix::net::UnixListener::bind(directory.socket()).unwrap();
    (
        CodexTransport {
            server: Mutex::new(None),
            socket_directory: directory,
            thread: Mutex::new(None),
            resume_thread: None,
            operation: Mutex::new(()),
        },
        listener,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    fn fixture() -> (CodexTransport, UnixListener) {
        test_transport()
    }

    fn receive(socket: &mut WebSocket<UnixStream>) -> Value {
        serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap()
    }

    fn reply(socket: &mut WebSocket<UnixStream>, request: &Value, result: Value) {
        socket
            .send(Message::Text(
                json!({"id":request["id"],"result":result}).to_string(),
            ))
            .unwrap();
    }

    fn initialized(listener: UnixListener) -> WebSocket<UnixStream> {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let initialize = receive(&mut socket);
        assert_eq!(initialize["method"], "initialize");
        assert_eq!(
            initialize["params"]["capabilities"]["experimentalApi"],
            true
        );
        reply(&mut socket, &initialize, json!({}));
        assert_eq!(receive(&mut socket)["method"], "initialized");
        socket
    }

    #[test]
    fn queues_independent_text_and_stable_identity_without_interrupting_an_active_turn() {
        let (transport, listener) = fixture();
        let server = std::thread::spawn(move || {
            let mut socket = initialized(listener);
            let list = receive(&mut socket);
            assert_eq!(list["method"], "thread/loaded/list");
            socket
                .send(Message::Text(
                    json!({"method":"thread/status/changed","params":{}}).to_string(),
                ))
                .unwrap();
            reply(
                &mut socket,
                &list,
                json!({"data":["the-thread"],"nextCursor":null}),
            );
            let add = receive(&mut socket);
            assert_eq!(add["method"], "thread/queue/add");
            assert_eq!(add["params"]["clientUserMessageId"], "launch:ticket-12");
            assert_eq!(
                add["params"]["input"],
                json!([{"type":"text","text":"line one\nline two","text_elements":[]}])
            );
            reply(
                &mut socket,
                &add,
                json!({"queuedSubmission":{"id":"queued-12"}}),
            );
            let start = receive(&mut socket);
            assert_eq!(start["method"], "thread/queue/start");
            assert_eq!(
                start["params"],
                json!({"threadId":"the-thread","queuedSubmissionId":"queued-12"})
            );
            socket.send(Message::Text(json!({"id":start["id"],"error":{"code":-32600,"message":"thread already has an active or pending turn"}}).to_string())).unwrap();
        });
        transport
            .send("line one\nline two", "launch:ticket-12")
            .unwrap();
        server.join().unwrap();
    }

    #[test]
    fn losing_the_add_response_is_uncertain_and_must_not_be_retried_as_input() {
        let (transport, listener) = fixture();
        let server = std::thread::spawn(move || {
            let mut socket = initialized(listener);
            let list = receive(&mut socket);
            reply(
                &mut socket,
                &list,
                json!({"data":["the-thread"],"nextCursor":null}),
            );
            assert_eq!(receive(&mut socket)["method"], "thread/queue/add");
            socket.close(None).unwrap();
        });
        let error = transport.send("accepted or lost", "same-id").unwrap_err();
        assert_eq!(error.written, 1);
        assert!(error.error.get_ref().unwrap().is::<UncertainSubmission>());
        server.join().unwrap();
    }

    #[test]
    fn absent_ambiguous_or_wrong_threads_never_receive_a_queue_request() {
        for (data, expected) in [
            (json!([]), None),
            (json!(["one", "two"]), None),
            (json!(["one"]), Some("different")),
        ] {
            assert_eq!(
                unique_thread(&json!({"data":data}), expected)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::WouldBlock
            );
        }
        assert_eq!(
            unique_thread(&json!({"data":["one"],"nextCursor":"another-page"}), None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            unique_thread(&json!({"data":["one"],"nextCursor":null}), Some("one")).unwrap(),
            "one"
        );
    }

    #[test]
    fn resume_fingerprints_only_the_generated_session_flag_hook() {
        let metadata = json!({"data":[{"hooks":[
            {"handlerType":"command","source":"sessionFlags","command":"/scratch/hook","key":"/<session-flags>/config.toml:stop:0:0","currentHash":"sha256:own"},
            {"handlerType":"command","source":"user","command":"/scratch/hook","key":"user-hook","currentHash":"sha256:user"},
            {"handlerType":"command","source":"sessionFlags","command":"user-custom-command","key":"other-hook","currentHash":"sha256:other"}
        ]}]});
        let overrides = own_hook_trust(&metadata, "/scratch/hook").unwrap();
        assert_eq!(overrides[0], "-c");
        assert_eq!(
            overrides[1],
            "hooks.state={\"/<session-flags>/config.toml:stop:0:0\"={trusted_hash=\"sha256:own\"}}"
        );
        let parsed: toml::Table = overrides[1].parse().unwrap();
        assert_eq!(parsed["hooks"]["state"].as_table().unwrap().len(), 1);
    }

    #[test]
    fn stopping_the_session_reaps_its_sidecar_and_removes_the_socket() {
        let (transport, listener) = fixture();
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        crate::process_session::lead_session(&mut command);
        let child = command.spawn().unwrap();
        let pid = child.id();
        let crash_cleanup = crate::crash_cleanup::register(pid, true);
        *transport.server.lock().unwrap() = Some(ServerProcess {
            child,
            crash_cleanup,
        });
        let socket_directory = transport.socket_directory.0.clone();
        transport.stop();
        assert!(!crate::crash_cleanup::is_registered(pid));
        assert!(crate::process_session::unreaped_state(pid as libc::pid_t).is_none());
        assert!(!socket_directory.exists());
        drop(listener);
    }

    #[test]
    fn sockets_are_private_short_and_removed_with_the_owner() {
        let directory = SocketDirectory::create().unwrap();
        let path = directory.0.clone();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(directory.socket().as_os_str().len() < 104);
        let listener = UnixListener::bind(directory.socket()).unwrap();
        drop(listener);
        drop(directory);
        assert!(!path.exists());
    }
}
