//! `browser`: turns the running daemon's browser access on and off and says where it stands, by
//! sending the daemon the requests Settings sends (`set_browser_access`, `set_remote_access_port`)
//! over a control connection opened with the launch key from the daemon's owner-only record, so
//! the setting is one the daemon keeps and applies at once, whichever of the two changed it. It
//! changes browser access alone: Allow remote access, which opens the application's socket (on
//! Windows its owner pipe), is a setting of its own that this command never touches.

use std::net::TcpStream;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{self, Message, WebSocket};

use crate::owner_record::Owner;

use super::background::{not_running, NOT_RUNNING, RESTART_WAIT};
use super::succession::{find_daemon, Found};

/// What `browser` does.
#[derive(Subcommand)]
pub(crate) enum Action {
    /// Turn browser access on: the daemon serves the browser at a loopback port and accepts the
    /// owner key there. It asks nothing, and says what it costs. Take the address from
    /// `octoboardd address`. A port other than the daemon's own (`status` says it) is needed.
    On {
        /// The loopback port to serve the browser at, kept across restarts; the one set before
        /// when absent.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    /// Turn browser access off: the daemon closes the browser port and every connection made
    /// through it. Its socket (on Windows, the application's owner pipe) is not touched.
    Off,
    /// Say whether browser access is on and where.
    Status,
}

/// How long the daemon is given for each frame it sends: it applies a change before it answers.
const WAIT: Duration = Duration::from_secs(15);

/// Why a daemon that has no browser access setting has none.
const NO_SETTING: &str = "this daemon serves no browser, so it has no browser access setting: its \
build embeds no UI, or it could not open its browser socket (its log says which)";

/// Why a daemon that offers remote access but no browser port has no browser access setting.
const NO_PORT: &str =
    "this daemon serves the browser on its socket alone and has no browser port, \
so it has no browser access setting: Allow remote access in Settings opens the socket";

/// What turning browser access on costs, which the confirmation in Settings states too.
const COST: &str =
    "On a computer other people also log in to, one of them can take the browser port \
while the daemon is not running or is restarting, and a browser that reconnects then sends \
them the key.";

/// Runs `action` against the running daemon, and returns the exit status: 0 when it was done or
/// reported, 3 when no daemon runs (nothing was changed), 1 otherwise.
pub(super) fn run(action: &Action) -> Result<i32> {
    let owner = match find_daemon(RESTART_WAIT, false)? {
        Found::NotRunning { .. } => {
            eprintln!("{}; browser access was not changed", not_running());
            return Ok(NOT_RUNNING);
        }
        Found::Running { owner, .. } => owner,
        Found::Unrecorded => {
            eprintln!(
                "{} is running, but its address is not known (it may still be starting or \
                 stopping); browser access was not changed",
                crate::APP_NAME
            );
            return Ok(1);
        }
    };
    let Some(key) = owner.recorded_key() else {
        eprintln!(
            "{} is running (pid {}), but its key is not recorded, so it cannot be asked to change \
             browser access",
            crate::APP_NAME,
            owner.identity.pid.0
        );
        return Ok(1);
    };
    match act(&owner, &key, action) {
        Ok(status) => Ok(status),
        Err(err) => {
            eprintln!("browser access was not changed: {err:#}");
            Ok(1)
        }
    }
}

/// `action` on the daemon `owner` names, with its launch key `key`: the requests it takes, then
/// the state read back and said.
fn act(owner: &Owner, key: &str, action: &Action) -> Result<i32> {
    // Refused before anything is changed: the daemon's own port is taken by the daemon itself.
    if let Action::On { port: Some(port) } = action {
        if *port == owner.port {
            bail!(
                "the browser port and the daemon's own port would both be {port}; choose another \
                 browser port (`octoboardd status` names the daemon's own)"
            );
        }
    }
    let mut control = Control::open(owner.port, key)?;
    // Before anything is sent: a daemon that has no browser access to change is not asked.
    match &control.remote_access {
        None => bail!("{}", NO_SETTING),
        Some(remote_access) if remote_access["browser_port"].is_null() => bail!("{}", NO_PORT),
        Some(_) => {}
    }
    match action {
        Action::Status => {}
        Action::On { port } => {
            // Before the switch, so that the daemon opens the port asked for and no other.
            if let Some(port) = port {
                control.request(json!({ "type": "set_remote_access_port", "port": port }))?;
            }
            control.request(json!({ "type": "set_browser_access", "allowed": true }))?;
        }
        Action::Off => {
            control.request(json!({ "type": "set_browser_access", "allowed": false }))?;
        }
    }
    // Read again: the daemon has applied the change before it answered, and a broadcast of it
    // is not ordered with the answer.
    let remote_access = if matches!(action, Action::Status) {
        control.remote_access
    } else {
        drop(control);
        Control::open(owner.port, key)?.remote_access
    };
    let Some(remote_access) = remote_access else {
        bail!("{}", NO_SETTING);
    };
    let application = owner.follows_application == Some(true);
    let (text, status) = report(action, &remote_access, application);
    println!("{text}");
    Ok(status)
}

/// What is said of the state `remote_access` (the snapshot's) after `action`, and the exit status;
/// `application` where the daemon is the desktop application's.
fn report(action: &Action, remote_access: &Value, application: bool) -> (String, i32) {
    let state = remote_access["browser_state"].as_str().unwrap_or("");
    let port = remote_access["browser_port"].as_u64();
    let failure = &remote_access["browser_failure"];
    let detail = failure["detail"].as_str().unwrap_or("no reason given");
    let port_failed = matches!(
        failure["reason"].as_str(),
        Some("port_in_use" | "port_is_daemons" | "port_failed")
    );
    let on_at = |port: Option<u64>| match port {
        Some(port) => format!("browser access is on, at port {port}"),
        None => "browser access is on".to_owned(),
    };
    let mut lines = Vec::new();
    let mut status = 0;
    match state {
        "serving" => {
            lines.push(format!(
                "{}; `octoboardd address` prints the address that opens it",
                on_at(port)
            ));
            if matches!(action, Action::On { .. }) {
                lines.push(String::new());
                lines.push(COST.to_owned());
            }
        }
        "failed" => {
            lines.push(format!("{}, but nothing is served: {detail}", on_at(port)));
            if port_failed {
                lines.push(
                    "choose another port with `octoboardd browser on --port <port>`".to_owned(),
                );
            }
            status = 1;
        }
        _ => {
            lines.push("browser access is off".to_owned());
            if matches!(action, Action::Off) {
                lines.push(if application {
                    "Allow remote access is a setting of its own and was not changed".to_owned()
                } else {
                    "the browser socket keeps serving".to_owned()
                });
            }
        }
    }
    (lines.join("\n"), status)
}

/// A control connection to the daemon, past its hello and snapshot.
struct Control {
    socket: WebSocket<TcpStream>,
    /// The snapshot's `remote_access`: null in a daemon that offers none.
    remote_access: Option<Value>,
    requests: u32,
}

impl Control {
    /// Opens a control connection to the daemon on loopback `port` with `key` and reads its
    /// snapshot.
    fn open(port: u16, key: &str) -> Result<Self> {
        let stream = crate::loopback::connect(port, WAIT)
            .with_context(|| format!("connecting to port {port}"))?;
        let url = format!("ws://127.0.0.1:{port}/ws/control?key={key}");
        let (mut socket, _) = tungstenite::client(url, stream).map_err(|err| match err {
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))
                if response.status() == 401 =>
            {
                anyhow::anyhow!("the daemon refused its key")
            }
            err => anyhow::anyhow!("opening the control connection failed: {err}"),
        })?;
        let remote_access = loop {
            let frame = read_frame(&mut socket)?;
            if frame["type"] == "snapshot" {
                break match &frame["remote_access"] {
                    Value::Null => None,
                    remote_access => Some(remote_access.clone()),
                };
            }
        };
        Ok(Self {
            socket,
            remote_access,
            requests: 0,
        })
    }

    /// Sends `request` and waits for the daemon's answer to it: an error frame is the daemon's
    /// refusal.
    fn request(&mut self, mut request: Value) -> Result<()> {
        self.requests += 1;
        let id = format!("browser-{}", self.requests);
        request["id"] = json!(id);
        self.socket
            .send(Message::text(request.to_string()))
            .context("sending the request failed")?;
        loop {
            let frame = read_frame(&mut self.socket)?;
            if frame["id"] != id.as_str() {
                continue;
            }
            match frame["type"].as_str() {
                Some("ack") => return Ok(()),
                Some("error") => {
                    if frame["code"] == "remote_access_unavailable" {
                        bail!("{}", NO_SETTING);
                    }
                    bail!(
                        "the daemon refused: {}",
                        frame["message"].as_str().unwrap_or("no reason given")
                    );
                }
                _ => {}
            }
        }
    }
}

/// The next text frame of `socket`, read as JSON.
fn read_frame(socket: &mut WebSocket<TcpStream>) -> Result<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => {
                return serde_json::from_str(&text)
                    .context("the daemon sent a frame that is no JSON");
            }
            Ok(Message::Close(_)) => bail!("the daemon closed the connection"),
            Ok(_) => {}
            Err(err) => bail!("reading from the daemon failed: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(state: &str, port: Option<u16>, failure: Option<(&str, &str)>) -> Value {
        json!({
            "state": "off",
            "browser_state": state,
            "browser_port": port,
            "failure": null,
            "browser_failure": failure.map(|(reason, detail)| json!({ "reason": reason, "detail": detail })),
        })
    }

    /// Turning it on says where it is served and what it costs, and exits 0.
    #[test]
    fn on_says_where_it_is_served_and_what_it_costs() {
        let (text, status) = report(
            &Action::On { port: None },
            &state("serving", Some(3369), None),
            true,
        );
        assert_eq!(status, 0);
        assert!(
            text.contains("browser access is on, at port 3369"),
            "{text}"
        );
        assert!(text.contains(COST), "{text}");
    }

    /// Reporting and turning it off say nothing of the cost.
    #[test]
    fn status_and_off_do_not_state_the_cost() {
        let serving = state("serving", Some(3369), None);
        assert!(!report(&Action::Status, &serving, true).0.contains(COST));
        let off = state("off", Some(3369), None);
        let (text, status) = report(&Action::Off, &off, true);
        assert_eq!(
            (text.starts_with("browser access is off"), status),
            (true, 0)
        );
        assert!(!text.contains(COST));
        assert_eq!(
            report(&Action::Status, &off, true).0,
            "browser access is off"
        );
    }

    /// Browser access serving is said so whatever remote access stands at: the report reads
    /// browser access alone.
    #[test]
    fn the_report_reads_browser_access_alone() {
        let mut remote_failed = state("serving", Some(3369), None);
        remote_failed["state"] = json!("failed");
        remote_failed["failure"] = json!({ "reason": "not_private", "detail": "open to others" });
        let (text, status) = report(&Action::Status, &remote_failed, true);
        assert_eq!(status, 0);
        assert!(
            text.contains("browser access is on, at port 3369"),
            "{text}"
        );
        assert!(!text.contains("open to others"), "{text}");
        let mut remote_on = state("off", Some(3369), None);
        remote_on["state"] = json!("serving");
        assert_eq!(
            report(&Action::Status, &remote_on, true).0,
            "browser access is off"
        );
    }

    /// A port that is taken fails the command and says how to choose another.
    #[test]
    fn a_port_in_use_fails_the_command_and_asks_for_another() {
        let taken = Some(("port_in_use", "listening on 127.0.0.1:3369: in use"));
        let (text, status) = report(
            &Action::On { port: None },
            &state("failed", Some(3369), taken),
            true,
        );
        assert_eq!(status, 1);
        assert!(text.contains("in use"), "{text}");
        assert!(text.contains("--port <port>"), "{text}");
        assert!(!text.contains(COST), "{text}");
    }

    /// The daemon's own port as the browser port is a port that failed too: another is asked for.
    #[test]
    fn the_daemons_own_port_fails_the_command_and_asks_for_another() {
        let own = Some((
            "port_is_daemons",
            "the browser port and the daemon's own port are both 3370",
        ));
        let (text, status) = report(
            &Action::On { port: None },
            &state("failed", Some(3370), own),
            true,
        );
        assert_eq!(status, 1);
        assert!(text.contains("--port <port>"), "{text}");
    }

    /// Nothing served is a failed command with the daemon's reason, and the hint to choose another
    /// port only where the port is what failed.
    #[test]
    fn a_failure_is_said_with_its_reason_and_the_port_hint_only_for_the_port() {
        let no_ui = state(
            "failed",
            Some(3369),
            Some(("no_ui", "this build of the daemon embeds no UI")),
        );
        let (text, status) = report(&Action::On { port: None }, &no_ui, false);
        assert_eq!(status, 1);
        assert!(text.contains("embeds no UI"), "{text}");
        assert!(!text.contains("--port"), "{text}");
        assert!(!text.contains(COST), "{text}");

        let broken = state(
            "failed",
            Some(3369),
            Some(("port_failed", "listening on 127.0.0.1:3369: denied")),
        );
        let (text, status) = report(&Action::Status, &broken, true);
        assert_eq!(status, 1);
        assert!(text.contains("denied"), "{text}");
        assert!(text.contains("--port <port>"), "{text}");
    }

    /// A scripted daemon: sends a hello and a snapshot carrying `remote_access`, answers every
    /// request with an ack (or the error `code` for one of the `refused` types), and hands back
    /// what it was asked on a channel. Serves `connections` connections.
    fn scripted_daemon(
        remote_access: Value,
        refused: &'static str,
        connections: usize,
    ) -> (u16, std::sync::mpsc::Receiver<Value>) {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (asked, requests) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..connections {
                let (stream, _) = listener.accept().unwrap();
                let Ok(mut socket) = tungstenite::accept(stream) else {
                    continue;
                };
                let hello = json!({ "type": "hello" }).to_string();
                let snapshot =
                    json!({ "type": "snapshot", "remote_access": remote_access }).to_string();
                if socket.send(Message::text(hello)).is_err()
                    || socket.send(Message::text(snapshot)).is_err()
                {
                    continue;
                }
                while let Ok(Message::Text(text)) = socket.read() {
                    let request: Value = serde_json::from_str(&text).unwrap();
                    let reply = if request["type"] == refused {
                        json!({ "type": "error", "id": request["id"],
                                "code": "remote_access_unavailable", "message": "no" })
                    } else {
                        json!({ "type": "ack", "id": request["id"] })
                    };
                    let _ = asked.send(request);
                    if socket.send(Message::text(reply.to_string())).is_err() {
                        break;
                    }
                }
            }
        });
        (port, requests)
    }

    fn owner_at(port: u16) -> Owner {
        crate::cli::background::tests::owner_on(port)
    }

    /// `on --port` sets the port, then turns browser access on, with the launch key, as Settings
    /// does.
    #[test]
    fn on_with_a_port_sends_the_port_then_the_switch() {
        let (port, requests) = scripted_daemon(state("serving", Some(4000), None), "", 2);
        let key = crate::cli::background::tests::KEY;
        let status = act(&owner_at(port), key, &Action::On { port: Some(4000) }).unwrap();
        assert_eq!(status, 0);
        let first = requests.recv().unwrap();
        let second = requests.recv().unwrap();
        assert_eq!(
            (first["type"].as_str(), first["port"].as_u64()),
            (Some("set_remote_access_port"), Some(4000))
        );
        assert_eq!(
            (second["type"].as_str(), second["allowed"].as_bool()),
            (Some("set_browser_access"), Some(true))
        );
    }

    /// `off` sends the switch alone; `status` sends nothing.
    #[test]
    fn off_sends_the_switch_and_status_sends_nothing() {
        let key = crate::cli::background::tests::KEY;
        let (port, requests) = scripted_daemon(state("off", Some(3369), None), "", 2);
        assert_eq!(act(&owner_at(port), key, &Action::Off).unwrap(), 0);
        let request = requests.recv().unwrap();
        assert_eq!(
            (request["type"].as_str(), request["allowed"].as_bool()),
            (Some("set_browser_access"), Some(false))
        );
        let (port, requests) = scripted_daemon(state("off", Some(3369), None), "", 1);
        assert_eq!(act(&owner_at(port), key, &Action::Status).unwrap(), 0);
        // Read once the command has ended: every request it sent was answered, so was sent first.
        assert_eq!(requests.try_iter().count(), 0);
    }

    /// A daemon that offers no browser access (one that serves no browser) is said to have no
    /// setting.
    #[test]
    fn a_daemon_with_no_setting_is_said_so() {
        let key = crate::cli::background::tests::KEY;
        for action in [Action::Status, Action::Off] {
            let (port, _requests) = scripted_daemon(Value::Null, "set_browser_access", 2);
            let err = act(&owner_at(port), key, &action).unwrap_err();
            assert!(err.to_string().contains("serves no browser"), "{err}");
        }
    }

    /// Off says on the server form that the socket keeps serving, and on the application's daemon
    /// that Allow remote access is a setting of its own that was not changed.
    #[test]
    fn off_says_what_it_leaves_by_the_form() {
        let off = state("off", Some(3369), None);
        let server = report(&Action::Off, &off, false).0;
        assert!(
            server.contains("the browser socket keeps serving"),
            "{server}"
        );
        let application = report(&Action::Off, &off, true).0;
        assert!(application.contains("Allow remote access"), "{application}");
    }

    /// A daemon that offers remote access but no browser port (a Linux desktop) is said to have no
    /// browser access, with nothing sent.
    #[test]
    fn a_daemon_with_no_browser_port_is_said_so() {
        let key = crate::cli::background::tests::KEY;
        let (port, requests) = scripted_daemon(state("off", None, None), "", 1);
        let err = act(&owner_at(port), key, &Action::Off).unwrap_err();
        assert!(err.to_string().contains("no browser port"), "{err}");
        // The command bailed before it sent anything.
        assert_eq!(requests.try_iter().count(), 0);
    }

    /// A browser port equal to the daemon's own port is refused naming both, and nothing is sent
    /// (no connection is even opened).
    #[test]
    fn the_daemons_own_port_is_refused_as_the_browser_port() {
        let key = crate::cli::background::tests::KEY;
        let (port, requests) = scripted_daemon(state("off", Some(3369), None), "", 1);
        let err = act(&owner_at(port), key, &Action::On { port: Some(port) }).unwrap_err();
        let said = err.to_string();
        assert!(said.contains("browser port"), "{said}");
        assert!(said.contains("daemon's own port"), "{said}");
        assert!(said.contains(&port.to_string()), "{said}");
        // Read once the command has ended: every request it sent was answered, so was sent first.
        assert_eq!(requests.try_iter().count(), 0);
    }
}
