//! The two settings on a host with both ways in (macOS), and the failures and connections each
//! way in's own settings leave to the other.

use super::test_support::*;
use super::*;
use crate::protocol::RemoteAccessFailureReason;
use crate::test_support::{ClosedPort, PATIENCE};

/// Where the host has both ways in, the two settings are independent. Remote access alone
/// opens the socket and no port; its address and forward are the socket's, at the default
/// port. Browser access alone opens the port and no socket; the key is held and the port
/// takes it. Both on, the port and the socket answer the same owner key beside each other and
/// the address and the forward name the port. Either turned off closes its own way in and
/// leaves the other serving with the key held; the owner key is refused once both are off.
/// A data directory that is not private costs the socket alone, which is said as its failure
/// while the port serves.
#[tokio::test]
async fn the_two_settings_open_and_close_their_own_ways_in() {
    let (state, data, private, _dir) = both_ways_daemon("remote-access-both");
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    assert_eq!(state.remote_access.recorded_port(&state), None);

    // Browser access alone: the port, no socket.
    state.remote_access.set_browser(&state, true).unwrap();
    let alone = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (alone.state, alone.browser_state),
        (RemoteAccessState::Off, RemoteAccessState::Serving),
        "{:?}",
        alone.browser_failure
    );
    assert_eq!(alone.failure, None);
    assert_eq!((alone.allowed, alone.browser_allowed), (false, true));
    let key = state.owner_key.current().expect("the owner key is held");
    assert_eq!(
        alone.address.as_deref(),
        Some(format!("http://127.0.0.1:{port}/?key={key}").as_str())
    );
    assert_eq!(alone.connection, None, "no application's way in is open");
    assert_eq!(state.remote_access.recorded_port(&state), Some(port));
    assert!(port_takes(port, &key).await.unwrap());
    assert!(!port_takes(port, &"0".repeat(64)).await.unwrap());
    assert!(!browser::socket::path_in(&data).exists());

    // Both on.
    state.remote_access.set(&state, true).unwrap();
    let both = state.remote_access.info(&state).unwrap();
    assert_eq!(both.browser_state, RemoteAccessState::Serving);
    assert!(both
        .forward_command
        .expect("the forward")
        .contains(&format!("-L 127.0.0.1:{port}:127.0.0.1:{port} ")));
    if !private {
        let failure = both.failure.expect("the socket's failure");
        assert_eq!(both.state, RemoteAccessState::Failed);
        assert!(matches!(
            failure.reason,
            RemoteAccessFailureReason::NotPrivate
        ));
        assert!(!browser::socket::path_in(&data).exists());
        state.remote_access.set_browser(&state, false).unwrap();
        assert!(!state.owner_key.is_held(), "nothing is open");
        drop(state.remote_access.close());
        return;
    }
    assert_eq!(both.state, RemoteAccessState::Serving);
    assert_eq!((both.failure, both.browser_failure), (None, None));
    assert!(socket_takes(&data, &key).await.unwrap());
    assert!(port_takes(port, &key).await.unwrap());

    // Remote access off: the socket goes, the port and the key stay.
    state.remote_access.set(&state, false).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.state, info.browser_state),
        (RemoteAccessState::Off, RemoteAccessState::Serving)
    );
    assert!(!browser::socket::path_in(&data).exists());
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    assert!(port_takes(port, &key).await.unwrap());
    assert_eq!(state.remote_access.recorded_port(&state), Some(port));

    // Remote access on and browser access off: the port goes, the socket and the key stay.
    state.remote_access.set(&state, true).unwrap();
    state.remote_access.set_browser(&state, false).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.state, info.browser_state),
        (RemoteAccessState::Serving, RemoteAccessState::Off)
    );
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    assert!(port_takes(port, &key).await.is_err(), "the port is closed");
    assert!(socket_takes(&data, &key).await.unwrap());
    assert_eq!(state.remote_access.recorded_port(&state), None);
    assert_eq!(
        info.address.as_deref(),
        Some(format!("http://127.0.0.1:{DEFAULT_BROWSER_PORT}/?key={key}").as_str())
    );
    assert!(info.forward_command.unwrap().contains("browser.sock"));

    // Both off: the key is refused and what it opened is cut off.
    let mut admitted = state
        .owner_key
        .admit(Some(&format!("key={key}")))
        .expect("admitted");
    state.remote_access.set(&state, false).unwrap();
    assert!(!state.owner_key.is_held());
    assert!(!browser::socket::path_in(&data).exists());
    tokio::time::timeout(PATIENCE, admitted.revoked())
        .await
        .expect("turning the last one off cuts off what the key opened");
    drop(state.remote_access.close());
}

/// A browser port another program holds does not stop the socket from serving: remote access
/// serves, browser access says why its port is not, the socket's forward is offered meanwhile
/// and no port is recorded; another port brings the port up.
#[tokio::test]
async fn a_port_in_use_leaves_the_socket_serving() {
    let (state, data, private, _dir) = both_ways_daemon("remote-access-both-in-use");
    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set(&state, true).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    let failure = info.browser_failure.clone().expect("why");
    assert!(
        matches!(failure.reason, RemoteAccessFailureReason::PortInUse),
        "{failure:?}"
    );
    assert_eq!(info.browser_state, RemoteAccessState::Failed);
    assert_eq!(state.remote_access.recorded_port(&state), None);
    if !private {
        // Nothing opened at all: no key, no address.
        assert_eq!(info.state, RemoteAccessState::Failed);
        assert!(matches!(
            info.failure.expect("the socket's failure").reason,
            RemoteAccessFailureReason::NotPrivate
        ));
        assert!(!state.owner_key.is_held());
        return;
    }
    assert_eq!(info.state, RemoteAccessState::Serving);
    let key = state.owner_key.current().expect("the owner key is held");
    assert!(socket_takes(&data, &key).await.unwrap());
    assert!(info.forward_command.unwrap().contains("browser.sock"));

    // The same port again tries it again; a free one is served.
    state.remote_access.set_port(&state, taken_port).unwrap();
    assert!(state
        .remote_access
        .info(&state)
        .unwrap()
        .browser_failure
        .is_some());
    let reserved = ClosedPort::new();
    let free = reserved.port();
    state.remote_access.set_port(&state, free).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.browser_state, info.browser_failure),
        (RemoteAccessState::Serving, None)
    );
    let key = state.owner_key.current().unwrap();
    assert!(port_takes(free, &key).await.unwrap());
    assert!(socket_takes(&data, &key).await.unwrap());
    drop(taken);
    drop(state.remote_access.close());
}

/// Turning remote access off closes the connections made through the socket and leaves one
/// made through the browser port as it is, and turning browser access off does the
/// converse.
#[tokio::test]
async fn each_setting_closes_only_the_connections_of_its_own_way_in() {
    let (state, data, private, _dir) = both_ways_daemon("remote-access-own-connections");
    if !private {
        return;
    }
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    state.remote_access.set(&state, true).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("serving");

    let off = state.clone();
    assert!(
        answers_over_port_after(port, &key, move || {
            off.remote_access.set(&off, false).unwrap();
        })
        .await,
        "the connection through the port did not stay open"
    );
    state.remote_access.set(&state, true).unwrap();
    let off = state.clone();
    assert!(
        answers_over_socket_after(&data, &key, move || {
            off.remote_access.set_browser(&off, false).unwrap();
        })
        .await,
        "the connection through the socket did not stay open"
    );
    let off = state.clone();
    assert!(
        closed_over_socket(&data, &key, move || {
            off.remote_access.set(&off, false).unwrap();
        })
        .await,
        "the connection through the socket was not closed"
    );
    state.remote_access.set_browser(&state, true).unwrap();
    let off = state.clone();
    assert!(
        closed_by(port, &key, move || {
            off.remote_access.set_browser(&off, false).unwrap();
        })
        .await,
        "the connection through the port was not closed"
    );
    assert!(!state.owner_key.is_held(), "both are off");
    drop(state.remote_access.close());
}

/// The socket failing while the port serves leaves browser access serving, names the
/// failure under remote access alone, and gives the port's address and forward, the key
/// held and the port recorded.
#[tokio::test]
async fn the_socket_failing_alone_leaves_the_port_serving_with_the_failure_named() {
    let (state, data, _dir) = socket_refusing_daemon("remote-access-socket-fails");
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    state.remote_access.set(&state, true).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.state, info.browser_state),
        (RemoteAccessState::Failed, RemoteAccessState::Serving),
        "{:?}",
        info.browser_failure
    );
    let failure = info.failure.clone().expect("the socket's failure");
    assert!(
        matches!(failure.reason, RemoteAccessFailureReason::NotPrivate),
        "{failure:?}"
    );
    assert_eq!(info.browser_failure, None);
    let key = state.owner_key.current().expect("the owner key is held");
    assert_eq!(
        info.address.as_deref(),
        Some(format!("http://127.0.0.1:{port}/?key={key}").as_str())
    );
    let forward = info.forward_command.expect("the forward");
    assert!(
        forward.contains(&format!("-L 127.0.0.1:{port}:127.0.0.1:{port} ")),
        "{forward}"
    );
    assert_eq!(state.remote_access.recorded_port(&state), Some(port));
    assert!(port_takes(port, &key).await.unwrap());
    assert!(!browser::socket::path_in(&data).exists());
    drop(state.remote_access.close());
}

/// Neither way in opening is a failure of each with nothing served and no key held, and
/// setting a free port does not mend a socket that stays refused: the port then serves
/// alone.
#[tokio::test]
async fn both_ways_failing_is_failed_until_the_port_can_be_opened() {
    let (state, _data, _dir) = socket_refusing_daemon("remote-access-both-fail");
    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set(&state, true).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.state, info.browser_state),
        (RemoteAccessState::Failed, RemoteAccessState::Failed)
    );
    assert!(info.failure.is_some() && info.browser_failure.is_some());
    assert_eq!((info.address, info.forward_command), (None, None));
    assert!(!state.owner_key.is_held());
    assert_eq!(state.remote_access.recorded_port(&state), None);

    let reserved = ClosedPort::new();
    state
        .remote_access
        .set_port(&state, reserved.port())
        .unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.state, info.browser_state),
        (RemoteAccessState::Failed, RemoteAccessState::Serving),
        "{:?}",
        info.browser_failure
    );
    let key = state.owner_key.current().expect("held");
    assert!(port_takes(reserved.port(), &key).await.unwrap());
    drop(taken);
    drop(state.remote_access.close());
}

/// Turning browser access off closes the port and ends a connection made through it, with
/// the key refused while remote access is off too.
#[tokio::test]
async fn turning_it_off_ends_a_connection_made_through_the_port() {
    let (state, _data, _dir) = socket_refusing_daemon("remote-access-off-connection");
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("serving");
    let off = state.clone();
    assert!(
        closed_by(port, &key, move || {
            off.remote_access.set_browser(&off, false).unwrap();
        })
        .await,
        "the connection through the port was not closed"
    );
    assert!(!state.owner_key.is_held());
    assert!(port_takes(port, &key).await.is_err(), "the port is closed");
    drop(state.remote_access.close());
}

/// A change of port ends every connection made through the old port at once, and the new
/// port answers the same key while the old one is released.
#[tokio::test]
async fn a_change_of_port_ends_a_connection_made_through_the_old_port() {
    let (state, _data, _dir) = socket_refusing_daemon("remote-access-change-connection");
    let (old_port, new_port) = (ClosedPort::new(), ClosedPort::new());
    let (old, new) = (old_port.port(), new_port.port());
    state.remote_access.set_port(&state, old).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("serving");
    let moved = state.clone();
    assert!(
        closed_by(old, &key, move || {
            moved.remote_access.set_port(&moved, new).unwrap();
        })
        .await,
        "the connection through the old port was not closed"
    );
    assert!(port_takes(new, &key).await.unwrap());
    assert!(
        port_takes(old, &key).await.is_err(),
        "the old port is let go"
    );
    drop(state.remote_access.close());
}

/// The server form's port moves as the desktop's does: a change of port ends what was
/// connected through the old port, and the socket's key is kept.
#[tokio::test]
async fn a_change_of_the_servers_port_ends_a_connection_made_through_the_old_port() {
    let (state, _data, _dir) = server_daemon("remote-access-server-change");
    let (old_port, new_port) = (ClosedPort::new(), ClosedPort::new());
    let (old, new) = (old_port.port(), new_port.port());
    state.remote_access.set_port(&state, old).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("held");
    let moved = state.clone();
    assert!(
        closed_by(old, &key, move || {
            moved.remote_access.set_port(&moved, new).unwrap();
        })
        .await,
        "the connection through the old port was not closed"
    );
    assert!(port_takes(new, &key).await.unwrap());
    assert!(
        port_takes(old, &key).await.is_err(),
        "the old port is let go"
    );
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    drop(state.remote_access.close());
}

/// The daemon's own TCP port refuses the owner key on the server form too, while its browser
/// port takes it.
#[tokio::test]
async fn the_servers_own_port_refuses_the_owner_key() {
    let (state, _data, _dir) = server_daemon("remote-access-server-own-refuses");
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("held");
    let own = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let own_port = own.local_addr().unwrap().port();
    let router = crate::server::router(state.clone());
    let own_served = tokio::spawn(async move { axum::serve(own, router).await });
    assert!(port_takes(port, &key).await.unwrap());
    assert!(!port_takes(own_port, &key).await.unwrap());
    own_served.abort();
    drop(state.remote_access.close());
}

/// The address and the forward name the browser port only while it is open: where the
/// port is in use and the socket serves, they are the socket's, at the default port, and
/// no port is recorded; where the socket cannot open either, nothing serves.
#[tokio::test]
async fn the_address_names_the_browser_port_only_while_it_is_open() {
    let (state, data, private, _dir) = both_ways_daemon("remote-access-address-socket");
    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set(&state, true).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        info.browser_port,
        Some(taken_port),
        "the setting is the user's"
    );
    assert_eq!(state.remote_access.recorded_port(&state), None);
    if !private {
        assert_eq!(info.state, RemoteAccessState::Failed);
        assert_eq!((info.address, info.forward_command), (None, None));
        return;
    }
    assert_eq!(info.state, RemoteAccessState::Serving);
    let key = state.owner_key.current().expect("held");
    assert_eq!(
        info.address.as_deref(),
        Some(format!("http://127.0.0.1:{DEFAULT_BROWSER_PORT}/?key={key}").as_str())
    );
    assert!(info.forward_command.unwrap().contains("browser.sock"));
    assert!(socket_takes(&data, &key).await.unwrap());
    drop(taken);
    drop(state.remote_access.close());
}
