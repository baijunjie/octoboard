//! The server form's daemon, which keeps browser access alone.

use super::test_support::*;
use super::*;
use crate::protocol::RemoteAccessFailureReason;
use crate::test_support::{app_state_for_sockets, ClosedPort};

/// In the server form the browser port is off until the setting turns it on, while the owner
/// key is held throughout; on, it answers the owner key and its address and the owner record
/// name it; off, it is closed, the connections made through it are cut off, the owner key stays
/// held and a connection the key opened elsewhere is not cut off.
#[tokio::test]
async fn the_server_forms_port_follows_the_setting_and_the_key_stays() {
    let (state, _data, _dir) = server_daemon("remote-access-server");
    let off = state.remote_access.info(&state).expect("offered");
    assert_eq!(
        (off.browser_allowed, off.browser_state),
        (false, RemoteAccessState::Off)
    );
    assert_eq!((off.address, off.forward_command), (None, None));
    assert_eq!(off.browser_port, Some(DEFAULT_BROWSER_PORT));
    assert!(state.owner_key.is_held());
    assert_eq!(state.remote_access.recorded_port(&state), None);

    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    assert_eq!(
        state.remote_access.info(&state).unwrap().browser_state,
        RemoteAccessState::Off
    );
    state.remote_access.set_browser(&state, true).unwrap();
    let on = state.remote_access.info(&state).unwrap();
    assert_eq!(
        on.browser_state,
        RemoteAccessState::Serving,
        "{:?}",
        on.browser_failure
    );
    let key = state.owner_key.current().expect("held");
    assert_eq!(
        on.address.as_deref(),
        Some(format!("http://127.0.0.1:{port}/?key={key}").as_str())
    );
    assert_eq!(state.remote_access.recorded_port(&state), Some(port));
    assert!(port_takes(port, &key).await.unwrap());
    assert!(!port_takes(port, &"0".repeat(64)).await.unwrap());
    // A connection the key admitted elsewhere (the socket's), which turning the port off keeps.
    let mut elsewhere = state
        .owner_key
        .admit(Some(&format!("key={key}")))
        .expect("admitted");

    let off_state = state.clone();
    assert!(
        closed_by(port, &key, move || {
            off_state
                .remote_access
                .set_browser(&off_state, false)
                .unwrap();
        })
        .await,
        "the connection through the port was not closed"
    );
    assert!(!state.store.allow_browser_access().unwrap());
    assert_eq!(
        state.remote_access.info(&state).unwrap().browser_state,
        RemoteAccessState::Off
    );
    assert_eq!(state.remote_access.recorded_port(&state), None);
    assert!(port_takes(port, &key).await.is_err(), "the port is closed");
    assert!(state.owner_key.is_held());
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    assert!(futures_util::FutureExt::now_or_never(elsewhere.revoked()).is_none());
    drop(state.remote_access.close());
}

/// The setting is kept: a daemon started again over the same store opens the port it was left
/// on at, and one left off opens none.
#[tokio::test]
async fn the_server_forms_setting_is_kept_across_a_restart() {
    let (state, data, _dir) = server_daemon("remote-access-server-kept");
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    drop(state.remote_access.close());
    assert!(port_takes(port, "x").await.is_err());

    let next = RemoteAccess::default();
    next.begin_server(&state, data.clone(), true);
    next.serve_clients(crate::server::browser_router(state.clone()));
    let key = state.owner_key.current().unwrap();
    assert!(port_takes(port, &key).await.unwrap());
    next.set_browser(&state, false).unwrap();
    drop(next.close());

    let again = RemoteAccess::default();
    again.begin_server(&state, data, true);
    again.serve_clients(crate::server::browser_router(state.clone()));
    assert!(port_takes(port, &key).await.is_err());
    drop(again.close());
}

/// The daemon's own port is refused as the browser port, naming both, and the port that was
/// open is not left open.
#[tokio::test]
async fn the_daemons_own_port_is_refused_as_the_browser_port() {
    let (state, _data, _dir) = server_daemon("remote-access-server-own-port");
    state.remote_access.set_port(&state, state.port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(info.browser_state, RemoteAccessState::Failed);
    let failure = info.browser_failure.expect("why");
    assert_eq!(failure.reason, RemoteAccessFailureReason::PortIsDaemons);
    assert!(
        failure.detail.contains("browser port") && failure.detail.contains("daemon's own port"),
        "{}",
        failure.detail
    );
    assert!(failure.detail.contains(&state.port.to_string()));
    assert_eq!(state.remote_access.recorded_port(&state), None);
    drop(state.remote_access.close());
}

/// A server-form daemon with no UI or no socket offers no setting.
#[tokio::test]
async fn a_server_daemon_that_serves_no_browser_has_no_setting() {
    let (state, _dir) = app_state_for_sockets("remote-access-server-none");
    assert_eq!(state.remote_access.info(&state), None);
    let refused = state.remote_access.set_browser(&state, true).unwrap_err();
    assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
}
