//! Remote access and browser access to the desktop application's daemon, two settings of their own:
//! whether it lets another computer in on its owner socket (on Windows, its owner pipe) and whether
//! it serves a browser at a loopback port, the address and the SSH forward a browser uses, and why
//! either could not. On the server form, only browser access is a setting. See "Remote access" in
//! `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

/// Where remote access and browser access to a daemon that follows the desktop application stand
/// (`snapshot.remote_access`, `remote_access_updated`), and, in the server form's daemon, where
/// browser access stands.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteAccess {
    /// The Allow remote access setting, as the user left it: whether another computer's
    /// application, or a browser forwarding to the socket, may reach this daemon over the owner
    /// socket (Linux, macOS) or the owner pipe (Windows). Off unless the user turned it on, and
    /// always off in the server form's daemon, whose socket serves whatever the setting. It is the
    /// user's alone: no agent's settings tool reads or changes it, and no configuration export
    /// carries it.
    pub allowed: bool,
    /// What the daemon does with remote access right now.
    pub state: RemoteAccessState,
    /// The Allow browser access setting, as the user left it: whether the daemon serves the browser
    /// at the loopback port `browser_port` names. Off unless the user turned it on (on Windows, a
    /// store from before the setting began with the value remote access had). False wherever
    /// `browser_port` is null. Like `allowed`, it is the user's alone.
    pub browser_allowed: bool,
    /// What the daemon does with browser access right now.
    pub browser_state: RemoteAccessState,
    /// The loopback port the daemon serves the browser at while browser access is on (Windows,
    /// macOS and the server form), as the user set it (`set_remote_access_port`); set whatever
    /// `browser_state` is. Null where the daemon offers no browser port (Linux's desktop daemon),
    /// where the browser goes through the owner socket. Like `allowed`, it is the user's alone.
    pub browser_port: Option<u16>,
    /// The address a browser opens, `http://127.0.0.1:<port>/?key=<owner key>`; set while the
    /// browser port serves (the port is `browser_port`, which a browser on this computer opens as
    /// it is), else while `state` is `serving` and the daemon has an owner socket (the port is the
    /// one the socket's forward binds on the browser's machine, which stays the same across
    /// launches, so the address can be bookmarked); null otherwise.
    pub address: Option<String>,
    /// The command the browser's machine forwards a port of its own to the daemon with, as
    /// `octoboardd address` prints it, the remote host left as `<server>` for the owner to fill in:
    /// to `browser_port` while the port serves, else to the owner socket; set whenever `address` is,
    /// null otherwise.
    pub forward_command: Option<String>,
    /// What another computer can reach this one by, for the owner to put in the forward's
    /// `<server>`, in the order Settings offers them: this computer's name (on macOS the Bonjour
    /// `<name>.local`, elsewhere the host name as the system reports it), then its IPv4 and then
    /// its IPv6 addresses, each as text that `ssh` takes after `user@` as it is. Loopback, unspecified, multicast and link-local addresses and
    /// `localhost` names are left out, as are deprecated, tentative and failed addresses where the
    /// system says so (Linux, Windows) and temporary IPv6 addresses on Linux (they are kept on
    /// macOS and Windows). Read when the state is; set while `state` or `browser_state` is
    /// `serving`, empty otherwise, and empty while the computer reports none.
    pub server_addresses: Vec<String>,
    /// Why the daemon could not serve remote access though the setting is on; set while `state` is
    /// `failed`, null otherwise.
    pub failure: Option<RemoteAccessFailure>,
    /// Why the daemon could not serve the browser port though browser access is on; set while
    /// `browser_state` is `failed`, null otherwise.
    pub browser_failure: Option<RemoteAccessFailure>,
    /// The connection details another device's application sets itself up from to reach this
    /// daemon over the computer's SSH server, as `octoboardd connection` prints them on a server;
    /// set while `state` is `serving`, null otherwise.
    pub connection: Option<ConnectionDetails>,
}

/// The connection details of a daemon: what another device needs to reach it over SSH, with no
/// secret (see "Printing the connection details" in `docs/product/daemon-command-line.md`).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionDetails {
    /// The details as one line of text,
    /// `octoboard+ssh://<user>@<host>:<port>?fingerprint=…&octoboardd=…&name=…`, which the desktop
    /// application reads back when it is pasted into a remote daemon's form. The host is this
    /// computer's own name, which the owner may need to correct on the other device.
    pub text: String,
    /// The QR code of `text`, one string per row of modules from the top, each character a module
    /// from the left: `1` dark, `0` light. The quiet zone around it is the client's to draw.
    pub qr: Vec<String>,
}

/// What a daemon that follows the application does with one of its two settings, remote access
/// (`state`) or browser access (`browser_state`).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteAccessState {
    /// The setting is off: the way in it opens is closed (for remote access the owner socket, or
    /// on Windows the owner pipe; for browser access the browser port). The owner key is refused
    /// once both settings are off.
    Off,
    /// The way in is open, serves the UI where it is a browser's, and accepts the owner key.
    Serving,
    /// The setting is on, but its way in could not be opened or the owner key could not be held
    /// (`failure` or `browser_failure` says which): nothing of it is served, and turning the
    /// setting off and on again tries again, as does setting the browser port again for browser
    /// access, the same port included.
    Failed,
}

/// Why a way in (the owner socket or owner pipe of remote access, the browser port of browser
/// access) is not served though its setting is on.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteAccessFailure {
    pub reason: RemoteAccessFailureReason,
    /// The system's own words for it, in English, with the path concerned: for the user to act on
    /// or to report, shown beside the client's own sentence for `reason`.
    pub detail: String,
}

/// The kind of failure, which a client words.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteAccessFailureReason {
    /// The daemon's build embeds no UI to serve (a development build).
    NoUi,
    /// The data directory, or a directory above it, lets another user in, so a socket there would
    /// not be the daemon's alone.
    NotPrivate,
    /// The socket could not be opened (a path too long for a socket, say).
    SocketFailed,
    /// The owner key could not be read or written.
    OwnerKeyFailed,
    /// `browser_port` is already in use, by another program or another user of the computer: the
    /// port is not served until it is changed or freed.
    PortInUse,
    /// `browser_port` is the daemon's own TCP port, which never serves the browser: the port is
    /// not served until it is changed.
    PortIsDaemons,
    /// `browser_port` could not be listened on for another reason.
    PortFailed,
    /// On Windows, the named pipe that is the application's way in (the owner endpoint another
    /// device's application reaches through `octoboardd proxy`) could not be opened: another
    /// program or another user of the computer holds its name, say.
    PipeFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Event;

    fn serving() -> RemoteAccess {
        RemoteAccess {
            allowed: true,
            state: RemoteAccessState::Serving,
            browser_allowed: true,
            browser_state: RemoteAccessState::Serving,
            browser_port: Some(3369),
            address: Some("http://127.0.0.1:3369/?key=k".to_owned()),
            forward_command: Some("ssh -N -L 3369:3369 me@<server>".to_owned()),
            server_addresses: vec!["host.local".to_owned(), "192.168.1.20".to_owned()],
            failure: None,
            browser_failure: None,
            connection: Some(ConnectionDetails {
                text: "octoboard+ssh://me@host:22".to_owned(),
                qr: vec!["101".to_owned(), "010".to_owned()],
            }),
        }
    }

    /// The state travels as the document names it, with every field present, null where it has
    /// nothing to say.
    #[test]
    fn remote_access_travels_as_the_protocol_document_names_it() {
        assert_eq!(
            serde_json::to_value(serving()).unwrap(),
            serde_json::json!({
                "allowed": true,
                "state": "serving",
                "browser_allowed": true,
                "browser_state": "serving",
                "browser_port": 3369,
                "address": "http://127.0.0.1:3369/?key=k",
                "forward_command": "ssh -N -L 3369:3369 me@<server>",
                "server_addresses": ["host.local", "192.168.1.20"],
                "failure": null,
                "browser_failure": null,
                "connection": {"text": "octoboard+ssh://me@host:22", "qr": ["101", "010"]},
            })
        );
        let off = RemoteAccess {
            allowed: false,
            state: RemoteAccessState::Off,
            browser_allowed: false,
            browser_state: RemoteAccessState::Off,
            browser_port: None,
            address: None,
            forward_command: None,
            server_addresses: Vec::new(),
            failure: None,
            browser_failure: None,
            connection: None,
        };
        let json = serde_json::to_value(off).unwrap();
        assert_eq!(json["state"], "off");
        assert_eq!(json["browser_state"], "off");
        for field in [
            "browser_port",
            "address",
            "forward_command",
            "failure",
            "browser_failure",
            "connection",
        ] {
            assert!(json[field].is_null(), "{field}");
        }
        assert_eq!(json["server_addresses"], serde_json::json!([]));
    }

    /// Each reason a client words is spelled as the document spells it.
    #[test]
    fn every_failure_reason_has_the_name_the_document_gives_it() {
        use RemoteAccessFailureReason::*;
        for (reason, name) in [
            (NoUi, "no_ui"),
            (NotPrivate, "not_private"),
            (SocketFailed, "socket_failed"),
            (OwnerKeyFailed, "owner_key_failed"),
            (PortInUse, "port_in_use"),
            (PortFailed, "port_failed"),
            (PipeFailed, "pipe_failed"),
        ] {
            let failed = RemoteAccess {
                state: RemoteAccessState::Failed,
                failure: Some(RemoteAccessFailure {
                    reason,
                    detail: "why".to_owned(),
                }),
                address: None,
                forward_command: None,
                connection: None,
                ..serving()
            };
            let json = serde_json::to_value(failed).unwrap();
            assert_eq!(json["state"], "failed");
            assert_eq!(
                json["failure"],
                serde_json::json!({"reason": name, "detail": "why"})
            );
        }
    }

    /// The broadcast is the whole state under its own key.
    #[test]
    fn the_update_event_carries_the_whole_state_under_its_key() {
        let json = serde_json::to_value(Event::RemoteAccessUpdated {
            remote_access: serving(),
        })
        .unwrap();
        assert_eq!(json["type"], "remote_access_updated");
        assert_eq!(json["remote_access"]["state"], "serving");
        assert_eq!(json["remote_access"]["browser_port"], 3369);
        assert!(json.get("id").is_none());
    }
}
