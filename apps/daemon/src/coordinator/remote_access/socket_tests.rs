//! The owner socket and the owner key it holds, on a host with a socket alone.

use super::test_support::{code_of, data_dir, socket_takes};
use super::*;
use crate::protocol::RemoteAccessFailureReason;
use crate::test_support::{app_state, PATIENCE};

/// The setting turns the owner socket and the owner key on and off while the daemon runs:
/// on, the socket answers with the owner key and the address carries it; off, the socket is
/// gone, the key is refused and what it admitted is cut off, and the setting is stored; on
/// again, the same key opens it, so a bookmarked address keeps working.
#[tokio::test]
async fn the_setting_opens_and_closes_the_owner_socket_while_the_daemon_runs() {
    let (state, dir) = app_state("remote-access-toggle");
    state.set_follows_application();
    let (data, private) = data_dir(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), true, WayIn::Socket);
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    let mut events = state.subscribe();

    let off = state.remote_access.info(&state).expect("offered");
    assert_eq!((off.allowed, off.state), (false, RemoteAccessState::Off));
    assert_eq!(
        (off.address, off.forward_command, off.failure),
        (None, None, None)
    );
    assert!(!browser::socket::path_in(&data).exists());
    assert!(!state.owner_key.is_held());

    state.remote_access.set(&state, true).unwrap();
    assert!(state.store.allow_remote_access().unwrap());
    let on = state.remote_access.info(&state).unwrap();
    let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
        panic!("the change was not broadcast");
    };
    assert_eq!(remote_access, on);
    if !private {
        assert_eq!(on.state, RemoteAccessState::Failed);
        let failure = on.failure.expect("why");
        assert!(matches!(
            failure.reason,
            RemoteAccessFailureReason::NotPrivate
        ));
        assert!(failure.detail.contains("chmod 700"), "{}", failure.detail);
        assert!(!state.owner_key.is_held());
        assert!(!browser::socket::path_in(&data).exists());
        return;
    }
    assert_eq!(on.state, RemoteAccessState::Serving);
    let key = state.owner_key.current().expect("the owner key is held");
    assert_eq!(
        crate::owner_key::read_in(&data).as_deref(),
        Some(key.as_str())
    );
    assert_eq!(
        on.address.as_deref(),
        Some(format!("http://127.0.0.1:{DEFAULT_BROWSER_PORT}/?key={key}").as_str())
    );
    let forward = on.forward_command.expect("the forward");
    assert!(
        forward.contains(&format!("127.0.0.1:{DEFAULT_BROWSER_PORT}:")),
        "{forward}"
    );
    assert!(forward.contains("browser.sock"), "{forward}");
    assert!(socket_takes(&data, &key).await.unwrap());
    assert!(!socket_takes(&data, &"0".repeat(64)).await.unwrap());
    let mut admitted = state
        .owner_key
        .admit(Some(&format!("key={key}")))
        .expect("admitted");

    state.remote_access.set(&state, false).unwrap();
    assert!(!state.store.allow_remote_access().unwrap());
    let off = state.remote_access.info(&state).unwrap();
    assert_eq!(off.state, RemoteAccessState::Off);
    assert_eq!(off.address, None);
    assert!(!state.owner_key.is_held());
    assert!(!browser::socket::path_in(&data).exists());
    assert!(
        socket_takes(&data, &key).await.is_err(),
        "the socket still answers"
    );
    tokio::time::timeout(PATIENCE, admitted.revoked())
        .await
        .expect("turning it off cuts off what the key opened");

    state.remote_access.set(&state, true).unwrap();
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    assert!(socket_takes(&data, &key).await.unwrap());
    drop(state.remote_access.close());
}

/// Rotating the key replaces the address and refuses the old key on the socket.
#[tokio::test]
async fn a_rotation_replaces_the_address() {
    let (state, dir) = app_state("remote-access-rotate");
    state.set_follows_application();
    let (data, private) = data_dir(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), true, WayIn::Socket);
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    let refused = state.remote_access.rotate(&state).unwrap_err();
    assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
    if !private {
        return;
    }
    state.remote_access.set(&state, true).unwrap();
    let old = state.owner_key.current().unwrap();
    state.remote_access.rotate(&state).unwrap();
    let new = state.owner_key.current().unwrap();
    assert_ne!(old, new);
    let info = state.remote_access.info(&state).unwrap();
    assert!(info.address.unwrap().ends_with(&new));
    assert!(!socket_takes(&data, &old).await.unwrap());
    assert!(socket_takes(&data, &new).await.unwrap());
    drop(state.remote_access.close());
}

/// A setting left on is applied as the daemon starts: the socket is open and the key held
/// before the daemon serves its clients, which begins the serving.
#[tokio::test]
async fn a_setting_left_on_is_applied_at_the_start() {
    let (state, dir) = app_state("remote-access-start");
    state.set_follows_application();
    state.store.set_allow_remote_access(true).unwrap();
    let (data, private) = data_dir(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), true, WayIn::Socket);
    let info = state.remote_access.info(&state).unwrap();
    if !private {
        assert_eq!(info.state, RemoteAccessState::Failed);
        return;
    }
    assert_eq!(info.state, RemoteAccessState::Serving);
    assert!(state.owner_key.is_held());
    let key = state.owner_key.current().unwrap();
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    assert!(socket_takes(&data, &key).await.unwrap());
    drop(state.remote_access.close());
}

/// A build that embeds no UI has nothing to serve: turned on, remote access says so, and
/// neither the socket nor the key is opened.
#[tokio::test]
async fn a_build_without_a_ui_says_so() {
    let (state, dir) = app_state("remote-access-no-ui");
    state.set_follows_application();
    let (data, _) = data_dir(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), false, WayIn::Socket);
    state.remote_access.set(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(info.state, RemoteAccessState::Failed);
    assert!(matches!(
        info.failure.unwrap().reason,
        RemoteAccessFailureReason::NoUi
    ));
    assert!(!state.owner_key.is_held());
    assert!(!browser::socket::path_in(&data).exists());
}

/// A daemon that does not follow the application offers no remote access: no state in the
/// snapshot, and both requests refused.
#[tokio::test]
async fn a_daemon_on_its_own_offers_no_remote_access() {
    let (state, _dir) = app_state("remote-access-alone");
    assert_eq!(state.remote_access.info(&state), None);
    let set = state.remote_access.set(&state, true).unwrap_err();
    assert_eq!(code_of(&set), error_code::REMOTE_ACCESS_UNAVAILABLE);
    assert!(
        !state.store.allow_remote_access().unwrap(),
        "nothing stored"
    );
    let rotate = state.remote_access.rotate(&state).unwrap_err();
    assert_eq!(code_of(&rotate), error_code::REMOTE_ACCESS_UNAVAILABLE);
}
