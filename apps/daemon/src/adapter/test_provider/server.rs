//! The server of the test-only provider: a server the launch starts through its host, whose
//! terminal attaches to it and whose messages go to it instead of the terminal, as an agent's own
//! server with an attached TUI does. It shows the generic launch and delivery path to carry such a
//! server with [`PrivateServer`] and [`MessageTransport`] as they are: a loopback endpoint whose
//! token is the server's password, a port read from the line the server prints, a message the
//! server answers for, and a server that ends while its terminal runs.
//!
//! The server is the provider's binary run as `<binary> serve`. It prints
//! [`READY_PREFIX`] and the port it listens on, takes `POST /message?key=<password>` with a JSON
//! body `{"id", "text"}`, and answers `204` for a message it took, `409` for one it cannot take
//! now and any other error status for one it refuses. The terminal is `<binary> attach <url>`,
//! given the password in [`PASSWORD_VAR`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::platform::shell::ResolvedBinary;
use crate::platform::term::{Command, PartialWrite};
use crate::session::term::{
    MessageTransport, PrivateServer, QueueStart, RefusedByAgent, Sent, StartedServer,
    TransportEvents, UncertainSubmission,
};
use crate::terminal::server_host::{
    RunningServer, ServerEndpoint, ServerHost, ServerListen, ServerReady,
};

/// What the server prints before the port it listens on.
pub(crate) const READY_PREFIX: &str = "test server listening on port ";

/// The variable the attached terminal is given the server's password in.
pub(crate) const PASSWORD_VAR: &str = "OCTOBOARD_TEST_SERVER_PASSWORD";

/// How often the transport looks whether the server still runs.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// The launch plan's server of the test-only provider
/// (`crate::test_support::test_provider::declaring_private_server`).
pub(crate) struct TestServer;

impl PrivateServer for TestServer {
    fn start(
        &self,
        binary: &ResolvedBinary,
        environment: &HashMap<String, String>,
        cwd: &Path,
        host: &dyn ServerHost,
        _launch_id: &str,
    ) -> Result<StartedServer> {
        let ServerEndpoint::Loopback { token, .. } = host.open_endpoint(ServerListen::Loopback)?
        else {
            bail!("the host opened no loopback endpoint");
        };
        let mut command = Command::new(&binary.program);
        command.args(&binary.leading_args);
        command.args(["serve"]);
        command.env_clear();
        for (key, value) in environment {
            command.env(key, value);
        }
        command.env(PASSWORD_VAR, &token);
        command.cwd(cwd);
        let (server, port) = host.start(
            command,
            ServerReady::PortAfter(READY_PREFIX),
            crate::test_support::PATIENCE,
        )?;
        let port = port.ok_or_else(|| anyhow!("the server reported no port"))?;
        Ok(StartedServer {
            args: vec!["attach".to_string(), format!("http://127.0.0.1:{port}")],
            env: vec![(PASSWORD_VAR.to_string(), token.clone())],
            transport: Arc::new(ServerTransport {
                port,
                password: token,
                server: Arc::from(server),
                stopped: Arc::new(AtomicBool::new(false)),
            }),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The transport to a started [`TestServer`].
pub(crate) struct ServerTransport {
    port: u16,
    password: String,
    server: Arc<dyn RunningServer>,
    stopped: Arc<AtomicBool>,
}

fn failed(error: impl std::error::Error + Send + Sync + 'static) -> PartialWrite {
    PartialWrite {
        written: 0,
        error: std::io::Error::other(error),
    }
}

impl MessageTransport for ServerTransport {
    /// Taken on `204`; declined, for the terminal to take, on `409` and where nothing listens;
    /// refused on any other error status; and uncertain where the request went out and no answer
    /// came back.
    fn send(&self, text: &str, logical_id: &str) -> Result<Sent, PartialWrite> {
        let body = serde_json::json!({ "id": logical_id, "text": text }).to_string();
        let answered = crate::loopback::post(
            self.port,
            &self.password,
            "/message",
            body.as_bytes(),
            crate::test_support::PATIENCE,
        );
        match answered {
            Ok(_) => Ok(Sent::Queued),
            Err(err) => match crate::loopback::error_status(&err) {
                Some(409) => Ok(Sent::Declined("the server is busy".to_string())),
                Some(status) => Err(failed(RefusedByAgent(format!(
                    "the server refused the message (HTTP {status})"
                )))),
                None if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    Ok(Sent::Declined("the server does not answer".to_string()))
                }
                None => Err(failed(UncertainSubmission(format!(
                    "the server took the request and did not answer: {err}"
                )))),
            },
        }
    }

    /// Watches the server: one that ends while the session runs takes its terminal with it, since
    /// a terminal attached to a server stays up without it.
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
        QueueStart::Nothing
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if !self.server.ends_with_its_terminal() {
            self.server.end();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    use super::*;
    use crate::adapter::AgentProvider;
    use crate::messaging::outbox::Drain;
    use crate::platform::proc::Containment;
    use crate::protocol::{AgentId, SessionStatus};
    use crate::session::term::{send_message_with_id, Route};
    use crate::state::test_support::insert_record;
    use crate::terminal::server_host::LocalHost;
    use crate::test_support::{app_state, fake_agent, fake_live_session, ScratchDir, PATIENCE};

    /// The variable the stand-in for the provider's binary reads the port it announces from.
    const PORT_VAR: &str = "OCTOBOARD_TEST_SERVER_PORT";
    /// The variable naming the file whose appearance ends the stand-in server.
    const GONE_VAR: &str = "OCTOBOARD_TEST_SERVER_GONE";

    /// The provider's binary: as `serve` it announces the port the test listens on, which stands
    /// in for the server's endpoint, and runs until the file [`GONE_VAR`] names is there.
    const BINARY: &str = r#"
if [ "$1" = serve ]; then
  echo "test server listening on port $OCTOBOARD_TEST_SERVER_PORT"
  while [ ! -e "$OCTOBOARD_TEST_SERVER_GONE" ]; do sleep 0.05; done
  exit 0
fi
"#;

    /// How the stand-in endpoint answers one request.
    #[derive(Clone, Copy)]
    enum Answer {
        Taken,
        Refused,
        /// Reads the request and hangs up without a word.
        HangUp,
    }

    /// A launch of the test-only provider whose server was started through a local host.
    struct Started {
        server: StartedServer,
        /// The file whose appearance ends the server's process.
        gone: std::path::PathBuf,
        listener: TcpListener,
        _containment: Containment,
        _dir: ScratchDir,
    }

    fn started(label: &str) -> Started {
        let dir = ScratchDir::new(label);
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let gone = dir.join("gone");
        let mut environment = fake_agent(&dir, "agent", BINARY);
        environment.insert(PORT_VAR.to_string(), port.to_string());
        environment.insert(GONE_VAR.to_string(), gone.to_string_lossy().into_owned());
        let binary = crate::platform::shell::resolve_binary("agent", &environment).expect("fake");

        let _declared = crate::test_support::test_provider::declaring_private_server();
        let fixture = crate::adapter::tests::spec_fixture();
        let plan = crate::adapter::test_provider::TestProvider
            .plan(&fixture.spec())
            .expect("a plan");
        let containment = Containment::create("test", label).expect("a containment");
        let server = {
            let host = LocalHost::new(&containment);
            plan.private_server
                .expect("the plan carries the server")
                .start(&binary, &environment, &dir, &host, "launch-1")
                .expect("the server started")
        };
        Started {
            server,
            gone,
            listener,
            _containment: containment,
            _dir: dir,
        }
    }

    impl Started {
        fn password(&self) -> &str {
            let (_, password) = self
                .server
                .env
                .iter()
                .find(|(name, _)| name == PASSWORD_VAR)
                .expect("the terminal is given the password");
            password
        }

        /// Answers the next requests as `answers` say, one each, on a thread of its own, and
        /// gives back each request's first line and body.
        fn answering(&self, answers: &[Answer]) -> std::thread::JoinHandle<Vec<(String, String)>> {
            let listener = self.listener.try_clone().expect("a second handle");
            let answers = answers.to_vec();
            std::thread::spawn(move || {
                answers
                    .into_iter()
                    .map(|answer| {
                        let mut stream = crate::test_support::accept_past_probes(&listener);
                        let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
                        let mut request_line = String::new();
                        reader.read_line(&mut request_line).expect("a request line");
                        let mut length = 0;
                        loop {
                            let mut line = String::new();
                            reader.read_line(&mut line).expect("a header");
                            if line == "\r\n" {
                                break;
                            }
                            if let Some(value) =
                                line.to_ascii_lowercase().strip_prefix("content-length:")
                            {
                                length = value.trim().parse().expect("a length");
                            }
                        }
                        let mut body = vec![0; length];
                        reader.read_exact(&mut body).expect("the body");
                        let head: &[u8] = match answer {
                            Answer::Taken => b"HTTP/1.1 204 No Content\r\n\r\n",
                            Answer::Refused => {
                                b"HTTP/1.1 422 Unprocessable\r\nContent-Length: 0\r\n\r\n"
                            }
                            Answer::HangUp => b"",
                        };
                        stream.write_all(head).expect("an answer");
                        (request_line, String::from_utf8(body).expect("a text body"))
                    })
                    .collect()
            })
        }
    }

    /// The launch starts the server through its host with a password of the launch's own and has
    /// the terminal attach to the port the server printed; a message then goes to the server,
    /// under that password, and the queue takes it as delivered once the server answered for it.
    #[test]
    fn a_message_goes_to_the_server_the_launch_started_and_is_confirmed_by_its_answer() {
        let started = started("test-provider-server-delivery");
        let port = started.listener.local_addr().unwrap().port();
        assert_eq!(
            started.server.args,
            ["attach".to_string(), format!("http://127.0.0.1:{port}")]
        );
        let password = started.password().to_string();
        assert_eq!(password.len(), 64, "the endpoint's own token");

        let (state, dir) = app_state("test-provider-server-delivery-state");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::TEST, 120, 40, "sleep 30")
            .with_transport(started.server.transport.clone());
        state.register_live(live.clone());
        let requests = started.answering(&[Answer::Taken, Answer::Taken]);

        assert_eq!(
            send_message_with_id(&live, "sent directly", "id-1", false, &|| false)
                .expect("the server took it"),
            Route::Queued
        );
        state.queue_message("s", "sent by the queue").unwrap();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(state.queued_count("s"), 0);

        let requests = requests.join().expect("the endpoint answered");
        for ((request_line, body), text) in
            requests.iter().zip(["sent directly", "sent by the queue"])
        {
            assert_eq!(
                *request_line,
                format!("POST /message?key={password} HTTP/1.1\r\n")
            );
            let body: serde_json::Value = serde_json::from_str(body).expect("a JSON body");
            assert_eq!(body["text"], text);
        }
        started.server.transport.stop();
    }

    /// A message the server refuses fails as refused by the agent, and one it took the request of
    /// without answering is uncertain: neither is written into the terminal, where a second copy
    /// could run.
    #[test]
    fn a_refused_message_and_a_write_with_no_answer_fail_without_reaching_the_terminal() {
        let started = started("test-provider-server-failed");
        let live = fake_live_session("s", AgentId::TEST, 120, 40, "sleep 30")
            .with_transport(started.server.transport.clone());
        let requests = started.answering(&[Answer::Refused, Answer::HangUp]);

        let refused = send_message_with_id(&live, "refused", "id-1", false, &|| false)
            .expect_err("the server refused it");
        assert_eq!(refused.written, 0);
        assert!(refused
            .error
            .get_ref()
            .is_some_and(|inner| inner.is::<RefusedByAgent>()));

        let unanswered = send_message_with_id(&live, "unanswered", "id-2", false, &|| false)
            .expect_err("the server did not answer");
        assert!(unanswered
            .error
            .get_ref()
            .is_some_and(|inner| inner.is::<UncertainSubmission>()));

        assert_eq!(requests.join().expect("the endpoint answered").len(), 2);
        started.server.transport.stop();
    }

    /// A server that ends while the session runs is reported gone, which ends the terminal
    /// attached to it; one the session stopped is reported to nobody.
    #[test]
    fn a_server_that_ends_is_reported_gone() {
        let started = started("test-provider-server-gone");
        let (gone, reported) = mpsc::channel();
        started.server.transport.attach(TransportEvents {
            ready: Box::new(|| {}),
            server_gone: Box::new(move || gone.send(()).expect("the test listens")),
            held: Box::new(|_| {}),
            released: Box::new(|_| {}),
        });
        assert!(reported.try_recv().is_err(), "the server runs");
        std::fs::write(&started.gone, "").expect("the file that ends the server");
        reported
            .recv_timeout(PATIENCE)
            .expect("the server's end is reported");
        started.server.transport.stop();
    }
}
