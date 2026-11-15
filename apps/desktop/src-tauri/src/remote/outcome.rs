//! What the UI and the shell exchange about connections: the request a connection is made with,
//! how it ended, and the state of each connection held, each named as the UI reads it.

use serde::{Deserialize, Serialize};

use super::octoboardd::GenerationMismatch;
use super::ssh::ConnectError;

/// How a remote daemon with a window open is connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Connected,
    /// The SSH connection dropped and is made again at the window's next connection.
    Reconnecting,
}

/// The state of one remote daemon's connection, for the UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    pub id: String,
    pub state: ConnectionState,
    /// Whether a window of the application shows it, or the system's browser.
    pub via: ViaName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ViaName {
    Window,
    Browser,
}

/// What the UI asks a connection to do.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub id: String,
    /// The password, or the key's passphrase; kept for this connection's reconnections alone.
    pub secret: Option<String>,
    /// The fingerprint the owner just confirmed, for an entry with none pinned.
    pub confirm_fingerprint: Option<String>,
    pub action: Option<Action>,
    /// Hold the connection for the system's browser instead of a window: the daemon's own UI is
    /// opened there, whatever its protocol generation.
    #[serde(default)]
    pub browser: bool,
}

/// What is run on the remote machine before the daemon is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// `octoboardd start`, for a server with no daemon running. Nothing here updates a daemon:
    /// updating is done on the machine itself.
    Start,
}

/// How a connection ended, for the UI to say what happened and offer what can be done.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "outcome",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ConnectOutcome {
    Opened,
    /// The connection is held for the browser, which the shell has opened at the tunnel's loopback
    /// address with the owner key, as `octoboardd address` prints a browser's address. The address
    /// is not returned: no page is handed the owner key.
    OpenedInBrowser,
    SecretNeeded {
        kind: SecretKindName,
    },
    HostKeyUnknown {
        fingerprint: String,
    },
    HostKeyMismatch {
        expected: String,
        actual: String,
    },
    Unreachable {
        detail: String,
    },
    AuthFailed {
        detail: String,
    },
    /// The host no longer accepts this device's key (it was removed there): signing in with the
    /// password installs it again.
    DeviceKeyRefused,
    KeyUnreadable {
        detail: String,
    },
    OctoboarddNotFound {
        tried: Vec<String>,
    },
    NoDaemon,
    /// The host's `octoboardd` is the desktop application's, and the application is not running.
    ApplicationNotRunning {
        detail: String,
    },
    NoOwnerEndpoint {
        detail: String,
    },
    EndpointUnreachable {
        detail: String,
    },
    GenerationMismatch {
        remote_generation: u32,
        local_generation: u32,
        remote_version: String,
        local_version: String,
    },
    ActionFailed {
        action: Action,
        detail: String,
    },
    Failed {
        detail: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretKindName {
    Password,
    Passphrase,
}

impl From<ConnectError> for ConnectOutcome {
    fn from(err: ConnectError) -> Self {
        match err {
            ConnectError::Unreachable(detail) => Self::Unreachable { detail },
            ConnectError::HostKeyUnknown { fingerprint } => Self::HostKeyUnknown { fingerprint },
            ConnectError::HostKeyMismatch { expected, actual } => {
                Self::HostKeyMismatch { expected, actual }
            }
            ConnectError::PassphraseNeeded => Self::SecretNeeded {
                kind: SecretKindName::Passphrase,
            },
            ConnectError::KeyUnreadable(detail) => Self::KeyUnreadable { detail },
            ConnectError::AuthFailed(detail) => Self::AuthFailed { detail },
        }
    }
}

impl From<GenerationMismatch> for ConnectOutcome {
    fn from(mismatch: GenerationMismatch) -> Self {
        Self::GenerationMismatch {
            remote_generation: mismatch.remote_generation,
            local_generation: mismatch.local_generation,
            remote_version: mismatch.remote_version,
            local_version: mismatch.local_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outcome_is_tagged_for_the_ui() {
        let json = |outcome: ConnectOutcome| serde_json::to_value(outcome).unwrap();
        assert_eq!(
            json(ConnectOutcome::Opened),
            serde_json::json!({"outcome": "opened"})
        );
        assert_eq!(
            json(ConnectOutcome::ApplicationNotRunning {
                detail: "closed".into()
            }),
            serde_json::json!({"outcome": "application_not_running", "detail": "closed"})
        );
        assert_eq!(
            json(ConnectOutcome::GenerationMismatch {
                remote_generation: 3,
                local_generation: 4,
                remote_version: "a".into(),
                local_version: "b".into(),
            }),
            serde_json::json!({
                "outcome": "generation_mismatch",
                "remoteGeneration": 3,
                "localGeneration": 4,
                "remoteVersion": "a",
                "localVersion": "b",
            })
        );
        assert_eq!(
            json(ConnectOutcome::ActionFailed {
                action: Action::Start,
                detail: "x".into()
            }),
            serde_json::json!({"outcome": "action_failed", "action": "start", "detail": "x"})
        );
        assert_eq!(
            json(ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Passphrase
            }),
            serde_json::json!({"outcome": "secret_needed", "kind": "passphrase"})
        );
        assert_eq!(
            json(ConnectOutcome::DeviceKeyRefused),
            serde_json::json!({"outcome": "device_key_refused"})
        );
    }

    #[test]
    fn the_status_is_named_for_the_ui() {
        assert_eq!(
            serde_json::to_value(Status {
                id: "e".into(),
                state: ConnectionState::Reconnecting,
                via: ViaName::Browser,
            })
            .unwrap(),
            serde_json::json!({"id": "e", "state": "reconnecting", "via": "browser"})
        );
        assert_eq!(
            serde_json::to_value(ConnectOutcome::OpenedInBrowser).unwrap(),
            serde_json::json!({"outcome": "opened_in_browser"}),
            "the browser's address, which carries the owner key, is never returned to a page"
        );
    }

    #[test]
    fn a_request_holds_for_a_window_unless_it_asks_for_the_browser() {
        let read = |json| {
            serde_json::from_value::<ConnectRequest>(json)
                .unwrap()
                .browser
        };
        assert!(!read(serde_json::json!({
            "id": "e", "secret": null, "confirmFingerprint": null, "action": null
        })));
        assert!(read(serde_json::json!({
            "id": "e", "secret": null, "confirmFingerprint": null, "action": null, "browser": true
        })));
    }

    #[test]
    fn the_request_reads_the_ui_s_names() {
        let request: ConnectRequest = serde_json::from_value(serde_json::json!({
            "id": "e", "secret": null, "confirmFingerprint": "SHA256:x", "action": "start"
        }))
        .unwrap();
        assert_eq!(request.confirm_fingerprint.as_deref(), Some("SHA256:x"));
        assert_eq!(request.action, Some(Action::Start));
    }

    #[test]
    fn every_way_the_ssh_connection_fails_is_told_to_the_ui_by_its_kind() {
        let outcome = |err: ConnectError| serde_json::to_value(ConnectOutcome::from(err)).unwrap();
        assert_eq!(
            outcome(ConnectError::Unreachable("down".into())),
            serde_json::json!({"outcome": "unreachable", "detail": "down"})
        );
        assert_eq!(
            outcome(ConnectError::HostKeyUnknown {
                fingerprint: "SHA256:a".into()
            }),
            serde_json::json!({"outcome": "host_key_unknown", "fingerprint": "SHA256:a"})
        );
        assert_eq!(
            outcome(ConnectError::HostKeyMismatch {
                expected: "SHA256:a".into(),
                actual: "SHA256:b".into()
            }),
            serde_json::json!({"outcome": "host_key_mismatch", "expected": "SHA256:a", "actual": "SHA256:b"})
        );
        assert_eq!(
            outcome(ConnectError::PassphraseNeeded),
            serde_json::json!({"outcome": "secret_needed", "kind": "passphrase"})
        );
        assert_eq!(
            outcome(ConnectError::KeyUnreadable("x".into())),
            serde_json::json!({"outcome": "key_unreadable", "detail": "x"})
        );
        assert_eq!(
            outcome(ConnectError::AuthFailed("no".into())),
            serde_json::json!({"outcome": "auth_failed", "detail": "no"})
        );
    }

    #[test]
    fn every_other_outcome_is_tagged_for_the_ui_too() {
        let json = |outcome: ConnectOutcome| serde_json::to_value(outcome).unwrap();
        assert_eq!(
            json(ConnectOutcome::OctoboarddNotFound {
                tried: vec!["a".into(), "b".into()]
            }),
            serde_json::json!({"outcome": "octoboardd_not_found", "tried": ["a", "b"]})
        );
        assert_eq!(
            json(ConnectOutcome::NoDaemon),
            serde_json::json!({"outcome": "no_daemon"})
        );
        assert_eq!(
            json(ConnectOutcome::NoOwnerEndpoint { detail: "d".into() }),
            serde_json::json!({"outcome": "no_owner_endpoint", "detail": "d"})
        );
        assert_eq!(
            json(ConnectOutcome::EndpointUnreachable { detail: "d".into() }),
            serde_json::json!({"outcome": "endpoint_unreachable", "detail": "d"})
        );
        assert_eq!(
            json(ConnectOutcome::Failed { detail: "d".into() }),
            serde_json::json!({"outcome": "failed", "detail": "d"})
        );
        assert_eq!(
            json(ConnectOutcome::HostKeyUnknown {
                fingerprint: "f".into()
            }),
            serde_json::json!({"outcome": "host_key_unknown", "fingerprint": "f"})
        );
        assert_eq!(
            json(ConnectOutcome::ActionFailed {
                action: Action::Start,
                detail: "d".into()
            }),
            serde_json::json!({"outcome": "action_failed", "action": "start", "detail": "d"})
        );
        assert_eq!(
            json(ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Password
            }),
            serde_json::json!({"outcome": "secret_needed", "kind": "password"})
        );
    }

    #[test]
    fn a_daemon_of_another_generation_names_both_generations_and_both_versions() {
        let outcome = ConnectOutcome::from(GenerationMismatch {
            remote_generation: 4,
            local_generation: 5,
            remote_version: "0.9.0".into(),
            local_version: "1.0.0".into(),
        });
        assert_eq!(
            outcome,
            ConnectOutcome::GenerationMismatch {
                remote_generation: 4,
                local_generation: 5,
                remote_version: "0.9.0".into(),
                local_version: "1.0.0".into(),
            }
        );
    }

    #[test]
    fn a_connection_can_only_ask_for_starting() {
        let read = |action: &str| {
            serde_json::from_value::<ConnectRequest>(serde_json::json!({
                "id": "e", "secret": null, "confirmFingerprint": null, "action": action
            }))
        };
        assert_eq!(read("start").unwrap().action, Some(Action::Start));
        for refused in ["update", "stop", "restart", "rm -rf ~", ""] {
            assert!(read(refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn the_connection_states_and_the_places_a_daemon_is_shown_are_named_for_the_ui() {
        let status = |state, via| {
            serde_json::to_value(Status {
                id: "e".into(),
                state,
                via,
            })
            .unwrap()
        };
        assert_eq!(
            status(ConnectionState::Connected, ViaName::Window),
            serde_json::json!({"id": "e", "state": "connected", "via": "window"})
        );
        assert_eq!(
            status(ConnectionState::Reconnecting, ViaName::Window)["state"],
            "reconnecting"
        );
    }
}
