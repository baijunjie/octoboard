//! OpenCode's loopback server hosts the model and the launch's plugin. Its TUI attaches over HTTP.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::prelude::*;
use serde_json::{json, Value};

use crate::platform::shell::ResolvedBinary;
use crate::platform::term::{Command, PartialWrite};
use crate::records::rows::RowRecords;
use crate::session::term::{
    MessageTransport, PrivateServer, QueueStart, RefusedByAgent, Sent, StartedServer,
    TransportEvents, UncertainSubmission,
};
use crate::terminal::server_host::{
    RunningServer, ServerEndpoint, ServerHost, ServerListen, ServerReady,
};

use super::version::Generation;

const READY: &str = "opencode server listening on http://127.0.0.1:";
const V2_READY: &str = "server listening on http://127.0.0.1:";
const V2_PASSWORD: &str = "OPENCODE_PASSWORD";
const PASSWORD: &str = "OPENCODE_SERVER_PASSWORD";
const USERNAME: &str = "OPENCODE_SERVER_USERNAME";
const USER: &str = "opencode";
const START_TIMEOUT: Duration = Duration::from_secs(15);
// OpenCode announces the port before the first workspace request initializes its plugins and cache.
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(45);
// A cold Windows workspace can download ripgrep before replying. A second POST after a timeout
// could create another conversation, so let the original request finish within a longer budget.
const WINDOWS_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(240);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const EXIT_POLL: Duration = Duration::from_millis(250);

pub(super) struct Attachment {
    pub config: Option<PathBuf>,
    pub route: PathBuf,
}

pub(super) struct OpenCodeServer {
    pub generation: Generation,
    pub resume: Option<String>,
    pub model: Option<String>,
    pub auto: bool,
    pub attachment: Option<Attachment>,
}

impl PrivateServer for OpenCodeServer {
    fn start(
        &self,
        binary: &ResolvedBinary,
        environment: &HashMap<String, String>,
        cwd: &Path,
        host: &dyn ServerHost,
        launch_id: &str,
    ) -> Result<StartedServer> {
        let ServerEndpoint::Loopback { token, .. } = host.open_endpoint(ServerListen::Loopback)?
        else {
            bail!("the host opened no loopback endpoint");
        };
        let mut command = Command::new(&binary.program);
        command.args(&binary.leading_args);
        command.args(["serve", "--hostname", "127.0.0.1", "--port", "0"]);
        command.env_clear();
        for (key, value) in environment {
            command.env(key, value);
        }
        command.env(PASSWORD, &token);
        if self.generation == Generation::V2 {
            command.env(V2_PASSWORD, &token);
        }
        command.env(USERNAME, USER);
        command.env(
            "OCTOBOARD_OPENCODE_SERVER",
            if self.auto { "auto" } else { "1" },
        );
        if let Some(attachment) = &self.attachment {
            command.env(
                "OCTOBOARD_OPENCODE_ROUTE",
                super::path_text(&attachment.route),
            );
        }
        if let Some(model) = &self.model {
            let content = environment
                .get(super::CONFIG_CONTENT_VAR)
                .context("OpenCode's launch has no configuration")?;
            let mut config: Value = serde_json::from_str(content)
                .context("reading the launch's OpenCode configuration")?;
            config["model"] = json!(model);
            command.env(super::CONFIG_CONTENT_VAR, config.to_string());
        }
        command.cwd(cwd);
        let ready = match self.generation {
            Generation::V1 => READY,
            Generation::V2 => V2_READY,
        };
        let (server, port) = host.start(command, ServerReady::PortAfter(ready), START_TIMEOUT)?;
        let port = port.context("OpenCode reported no port")?;
        let transport = OpenCodeTransport::new(
            self.generation,
            port,
            &token,
            self.attachment
                .as_ref()
                .map(|attachment| attachment.route.clone()),
            server,
        );
        let bootstrap_timeout =
            if crate::platform::HostOs::current() == crate::platform::HostOs::Windows {
                tracing::info!(
                    %launch_id,
                    "waiting for OpenCode's workspace to initialize, including any first-run tools"
                );
                WINDOWS_BOOTSTRAP_TIMEOUT
            } else {
                BOOTSTRAP_TIMEOUT
            };
        let session = match &self.resume {
            Some(id) => {
                anyhow::ensure!(
                    super::records::OpenCodeRecords.names_conversation(id),
                    "OpenCode's session id is invalid"
                );
                if self.generation == Generation::V2 {
                    let response = transport
                        .http
                        .get(format!("{}/api/session/{id}", transport.url))
                        .config()
                        .timeout_global(Some(bootstrap_timeout))
                        .build()
                        .header("Authorization", &transport.auth)
                        .call()
                        .context("OpenCode did not answer session lookup")?;
                    anyhow::ensure!(
                        response.status().is_success(),
                        "OpenCode's session is unavailable"
                    );
                }
                id.clone()
            }
            None => {
                let (path, body) = match self.generation {
                    Generation::V1 => ("/session", json!({})),
                    Generation::V2 => (
                        "/api/session",
                        json!({
                            "location": { "directory": super::path_text(cwd) }
                        }),
                    ),
                };
                let mut response = transport
                    .http
                    .post(format!("{}{path}", transport.url))
                    .config()
                    .timeout_global(Some(bootstrap_timeout))
                    .build()
                    .header("Authorization", &transport.auth)
                    .header("Content-Type", "application/json")
                    .send(body.to_string())
                    .context("OpenCode did not answer session creation")?;
                anyhow::ensure!(
                    response.status().is_success(),
                    "OpenCode refused session creation"
                );
                let body = response
                    .body_mut()
                    .with_config()
                    .limit(256 * 1024)
                    .read_to_string()?;
                let body: Value = serde_json::from_str(&body)?;
                let session = match self.generation {
                    Generation::V1 => &body,
                    Generation::V2 => &body["data"],
                };
                session["id"]
                    .as_str()
                    .context("OpenCode created a session without an id")?
                    .to_owned()
            }
        };
        anyhow::ensure!(
            super::records::OpenCodeRecords.names_conversation(&session),
            "OpenCode's session id is invalid"
        );
        let (mut env, args) = match self.generation {
            Generation::V1 => (
                vec![(PASSWORD.into(), token), (USERNAME.into(), USER.into())],
                vec![
                    "attach".into(),
                    transport.url.clone(),
                    "--session".into(),
                    session,
                ],
            ),
            Generation::V2 => (
                vec![(V2_PASSWORD.into(), token)],
                vec![
                    "--server".into(),
                    transport.url.clone(),
                    "--session".into(),
                    session,
                ],
            ),
        };
        if let Some(config) = self
            .attachment
            .as_ref()
            .and_then(|attachment| attachment.config.as_ref())
        {
            env.push((super::TUI_CONFIG_VAR.into(), super::path_text(config)));
        }
        Ok(StartedServer {
            args,
            env,
            transport: Arc::new(transport),
        })
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct OpenCodeTransport {
    generation: Generation,
    url: String,
    route: Option<PathBuf>,
    auth: String,
    http: ureq::Agent,
    server: Arc<dyn RunningServer>,
    stopped: Arc<AtomicBool>,
}

impl OpenCodeTransport {
    fn new(
        generation: Generation,
        port: u16,
        token: &str,
        route: Option<PathBuf>,
        server: Box<dyn RunningServer>,
    ) -> Self {
        Self {
            generation,
            url: format!("http://127.0.0.1:{port}"),
            auth: format!(
                "Basic {}",
                BASE64_STANDARD.encode(format!("{USER}:{token}"))
            ),
            route,
            server: Arc::from(server),
            stopped: Arc::new(AtomicBool::new(false)),
            http: ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .proxy(None)
                .max_redirects(0)
                .http_status_as_error(false)
                .build()
                .into(),
        }
    }
}

pub(super) fn adopt(
    launch: crate::adapter::AdoptedLaunch<'_>,
) -> Option<Arc<dyn MessageTransport>> {
    let found = launch.server?;
    let ServerEndpoint::Loopback {
        token,
        port: Some(port),
    } = found.endpoint
    else {
        return None;
    };
    let route = launch
        .adoption
        .and_then(|meta| meta["opencode_route"].as_str())
        .map(PathBuf::from);
    let generation = match launch
        .adoption
        .and_then(|meta| meta["opencode_api"].as_u64())
    {
        Some(2) => Generation::V2,
        _ => Generation::V1,
    };
    Some(Arc::new(OpenCodeTransport::new(
        generation,
        port,
        &token,
        route,
        found.server,
    )))
}

fn failed(error: impl std::error::Error + Send + Sync + 'static) -> PartialWrite {
    PartialWrite {
        written: 0,
        error: std::io::Error::other(error),
    }
}

impl MessageTransport for OpenCodeTransport {
    fn send(&self, text: &str, _logical_id: &str) -> Result<Sent, PartialWrite> {
        let session = self
            .route
            .as_deref()
            .and_then(|path| super::read_regular_text(path, 64).ok())
            .filter(|id| super::records::OpenCodeRecords.names_conversation(id));
        let Some(session) = session else {
            return Ok(Sent::Declined(
                "OpenCode's terminal has no known conversation".into(),
            ));
        };
        let (path, body) = match self.generation {
            Generation::V1 => (
                format!("/session/{session}/prompt_async"),
                json!({
                    "parts": [{ "type": "text", "text": text }]
                }),
            ),
            Generation::V2 => (
                format!("/api/session/{session}/prompt"),
                json!({
                    "text": text, "delivery": "steer"
                }),
            ),
        };
        let response = self
            .http
            .post(format!("{}{path}", self.url))
            .header("Authorization", &self.auth)
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|_| {
                failed(UncertainSubmission(
                    "OpenCode did not answer the prompt request".into(),
                ))
            })?;
        let status = response.status().as_u16();
        match (self.generation, status) {
            (Generation::V1, 204) | (Generation::V2, 200) => Ok(Sent::Queued),
            (_, 400..500) => Err(failed(RefusedByAgent(format!(
                "OpenCode refused the prompt (HTTP {status})"
            )))),
            _ => Err(failed(UncertainSubmission(format!(
                "OpenCode answered the prompt (HTTP {status})"
            )))),
        }
    }

    fn attach(&self, events: TransportEvents) {
        let (server, stopped) = (self.server.clone(), self.stopped.clone());
        std::thread::spawn(move || loop {
            if stopped.load(Ordering::Acquire) {
                return;
            }
            if server.exited() {
                (events.server_gone)();
                return;
            }
            std::thread::sleep(EXIT_POLL);
        });
    }

    fn start_queue(&self) -> QueueStart {
        // OpenCode keeps a queued prompt in its conversation even after an abort. Its next
        // prompt will pick it up, so reporting it returned would send it twice.
        QueueStart::Nothing
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if !self.server.ends_with_its_terminal() {
            self.server.end();
        }
    }

    fn command_host(&self) -> Option<crate::platform::proc::ProcessIdentity> {
        self.server.process()
    }
}
