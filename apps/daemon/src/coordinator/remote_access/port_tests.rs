//! Browser access and its port, and on a host with owner pipes the pipe beside it, exercised on
//! every host through [`RemoteAccess::begin_with`].

use super::test_support::*;
use super::*;
use crate::protocol::RemoteAccessFailureReason;
use crate::test_support::{app_state, ClosedPort, PATIENCE};

/// Whether the daemon whose data directory is `data` takes `key` on its owner pipe, asked as
/// `octoboardd address --json` reaches it; an error when no pipe of its own answers.
async fn pipe_takes(data: &Path, key: &str) -> std::io::Result<bool> {
    let (name, key) = (crate::platform::owner_pipe::name_for(data), key.to_owned());
    tokio::task::spawn_blocking(move || {
        let pipe = crate::platform::owner_pipe::connect(&name, PATIENCE)?;
        browser::key_check_over(pipe, DEFAULT_BROWSER_PORT, &key)
    })
    .await
    .unwrap()
}

/// Where the host has owner pipes (Windows), Allow remote access opens the owner pipe, serving
/// the daemon's router there with the owner key, apart from the browser port: a change of port
/// and browser access turned on and off leave it open, and turning remote access off closes it.
#[tokio::test]
async fn the_owner_pipe_follows_remote_access_alone() {
    if !crate::platform::owner_pipe::AVAILABLE {
        return;
    }
    let (state, dir) = port_daemon("remote-access-pipe");
    assert!(pipe_takes(&dir, "x").await.is_err(), "no pipe while off");
    let (first, second) = (ClosedPort::new(), ClosedPort::new());
    state.remote_access.set_port(&state, first.port()).unwrap();
    state.remote_access.set(&state, true).unwrap();
    let key = state.owner_key.current().expect("serving");
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (info.state, info.browser_state),
        (RemoteAccessState::Serving, RemoteAccessState::Off),
        "{:?}",
        info.failure
    );
    assert_eq!(
        (info.address, info.forward_command),
        (None, None),
        "the pipe is no browser's way in"
    );
    assert!(pipe_takes(&dir, &key).await.unwrap());
    assert!(!pipe_takes(&dir, &"0".repeat(64)).await.unwrap());

    state.remote_access.set_browser(&state, true).unwrap();
    state.remote_access.set_port(&state, second.port()).unwrap();
    state.remote_access.set_browser(&state, false).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(info.state, RemoteAccessState::Serving, "{:?}", info.failure);
    assert!(
        info.connection.is_some(),
        "the details are given while serving"
    );
    assert!(pipe_takes(&dir, &key).await.unwrap());

    state.remote_access.set(&state, false).unwrap();
    let gone = crate::test_support::within(PATIENCE, {
        let dir = dir.to_path_buf();
        move || {
            crate::platform::owner_pipe::connect(
                &crate::platform::owner_pipe::name_for(&dir),
                std::time::Duration::ZERO,
            )
            .is_err()
        }
    })
    .await;
    assert!(gone, "the pipe still answers once remote access is off");
    drop(state.remote_access.close());
}

/// The port is the default until the user sets one, and is said whether browser access is on
/// or not; the switch opens and closes the port while the daemon runs, with the owner key, and
/// the address and forward name the port; the daemon's own port refuses the owner key all the
/// while.
#[tokio::test]
async fn the_switch_opens_and_closes_the_browser_port_while_the_daemon_runs() {
    let (state, dir) = port_daemon("remote-access-port-toggle");
    let off = state.remote_access.info(&state).expect("offered");
    assert_eq!(off.browser_port, Some(DEFAULT_BROWSER_PORT));
    assert_eq!(
        (off.browser_allowed, off.browser_state),
        (false, RemoteAccessState::Off)
    );

    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    assert_eq!(state.store.remote_access_port().unwrap(), Some(port));
    let off = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (off.browser_port, off.browser_state),
        (Some(port), RemoteAccessState::Off)
    );
    assert!(!state.owner_key.is_held(), "setting the port opens nothing");
    assert!(port_takes(port, &"0".repeat(64)).await.is_err());

    let mut events = state.subscribe();
    state.remote_access.set_browser(&state, true).unwrap();
    let on = state.remote_access.info(&state).unwrap();
    let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
        panic!("the change was not broadcast");
    };
    assert_eq!(remote_access, on);
    assert_eq!(
        on.browser_state,
        RemoteAccessState::Serving,
        "{:?}",
        on.browser_failure
    );
    let key = state.owner_key.current().expect("the owner key is held");
    assert_eq!(
        crate::owner_key::read_in(&dir).as_deref(),
        Some(key.as_str())
    );
    assert_eq!(
        on.address.as_deref(),
        Some(format!("http://127.0.0.1:{port}/?key={key}").as_str())
    );
    let forward = on.forward_command.expect("the forward");
    assert!(
        forward.contains(&format!("-L 127.0.0.1:{port}:127.0.0.1:{port} ")),
        "{forward}"
    );
    assert!(port_takes(port, &key).await.unwrap());
    assert!(!port_takes(port, &"0".repeat(64)).await.unwrap());
    assert!(
        !browser::socket::path_in(&dir).exists(),
        "no socket is opened on this way in"
    );

    // The daemon's own port, which the application and the helpers use, refuses the key.
    let own = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let own_port = own.local_addr().unwrap().port();
    let router = crate::server::router(state.clone());
    let own_served = tokio::spawn(async move { axum::serve(own, router).await });
    assert!(!port_takes(own_port, &key).await.unwrap());

    let mut admitted = state
        .owner_key
        .admit(Some(&format!("key={key}")))
        .expect("admitted");
    state.remote_access.set_browser(&state, false).unwrap();
    let off = state.remote_access.info(&state).unwrap();
    assert_eq!(off.browser_state, RemoteAccessState::Off);
    assert_eq!((off.address, off.forward_command), (None, None));
    assert_eq!(off.browser_port, Some(port));
    assert!(!state.owner_key.is_held());
    assert!(
        port_takes(port, &key).await.is_err(),
        "the port still answers"
    );
    tokio::time::timeout(PATIENCE, admitted.revoked())
        .await
        .expect("turning it off cuts off what the key opened");

    own_served.abort();
    drop(state.remote_access.close());
}

/// A change of port while serving lets the old port go and serves the same key at the new one,
/// which the key stays held through; the same port again changes nothing. What was connected
/// through the old port is told to close (see
/// `a_change_of_port_ends_a_connection_made_through_the_old_port`).
#[tokio::test]
async fn a_change_of_port_moves_the_browser_at_once() {
    let (state, _dir) = port_daemon("remote-access-port-change");
    let (old_port, new_port) = (ClosedPort::new(), ClosedPort::new());
    let old = old_port.port();
    state.remote_access.set_port(&state, old).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("serving");
    assert!(port_takes(old, &key).await.unwrap());

    state.remote_access.set_port(&state, old).unwrap();
    assert!(port_takes(old, &key).await.unwrap());

    let new = new_port.port();
    state.remote_access.set_port(&state, new).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(info.browser_state, RemoteAccessState::Serving);
    assert_eq!(info.browser_port, Some(new));
    assert_eq!(
        info.address.as_deref(),
        Some(format!("http://127.0.0.1:{new}/?key={key}").as_str())
    );
    assert!(port_takes(new, &key).await.unwrap());
    assert!(
        port_takes(old, &key).await.is_err(),
        "the old port is let go"
    );
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    drop(state.remote_access.close());
}

/// A port already in use is said, with nothing served and no owner key held, until the port is
/// changed; the daemon runs on.
#[tokio::test]
async fn a_port_in_use_is_said_until_it_is_changed() {
    let (state, _dir) = port_daemon("remote-access-port-in-use");
    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(info.browser_state, RemoteAccessState::Failed);
    let failure = info.browser_failure.expect("why");
    assert!(
        matches!(failure.reason, RemoteAccessFailureReason::PortInUse),
        "{failure:?}"
    );
    assert!(
        failure.detail.contains(&taken_port.to_string()),
        "{}",
        failure.detail
    );
    assert_eq!((info.address, info.forward_command), (None, None));
    assert!(!state.owner_key.is_held());

    let reserved = ClosedPort::new();
    let free = reserved.port();
    state.remote_access.set_port(&state, free).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(info.browser_state, RemoteAccessState::Serving);
    assert_eq!(info.browser_failure, None);
    let key = state.owner_key.current().unwrap();
    assert!(port_takes(free, &key).await.unwrap());
    drop(taken);
    drop(state.remote_access.close());
}

/// Settings left on are applied at the start, at the stored port; the owner record names the
/// port while the key is taken there.
#[tokio::test]
async fn settings_left_on_are_applied_at_the_start() {
    let (state, dir) = app_state("remote-access-port-start");
    state.set_follows_application();
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.store.set_allow_browser_access(true).unwrap();
    state.store.set_remote_access_port(Some(port)).unwrap();
    state
        .remote_access
        .begin_with(&state, dir.to_path_buf(), true, WayIn::Port);
    assert_eq!(state.remote_access.recorded_port(&state), Some(port));
    let key = state.owner_key.current().expect("held before serving");
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    assert!(port_takes(port, &key).await.unwrap());
    state.remote_access.set_browser(&state, false).unwrap();
    assert_eq!(state.remote_access.recorded_port(&state), None);
    drop(state.remote_access.close());
}

/// Where the browser goes through the owner socket, there is no port to set and no browser
/// access to turn on.
#[tokio::test]
async fn the_socket_way_in_has_no_port() {
    let (state, dir) = app_state("remote-access-port-socket");
    state.set_follows_application();
    state
        .remote_access
        .begin_with(&state, dir.to_path_buf(), true, WayIn::Socket);
    assert_eq!(state.remote_access.info(&state).unwrap().browser_port, None);
    let refused = state.remote_access.set_port(&state, 4000).unwrap_err();
    assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
    let refused = state.remote_access.set_browser(&state, true).unwrap_err();
    assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
    assert_eq!(
        state.store.remote_access_port().unwrap(),
        None,
        "nothing stored"
    );
    assert!(!state.store.allow_browser_access().unwrap());
    let (alone, _dir) = app_state("remote-access-port-alone");
    let refused = alone.remote_access.set_port(&alone, 4000).unwrap_err();
    assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
}

/// Another device's application is set up from the connection details while remote access
/// serves: the text is the line `octoboardd connection` prints, the QR code is the code of
/// exactly that text, and there are none while remote access is off or has failed, however
/// browser access stands.
#[tokio::test]
async fn the_connection_details_are_given_while_remote_access_serves_and_not_otherwise() {
    if crate::connection_details::of_this_machine(None).is_none() {
        // Neither the user's nor the machine's name can be read here: no details are told.
        return;
    }
    let (state, dir) = app_state("remote-access-connection");
    state.set_follows_application();
    let (data, private) = data_dir(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), true, WayIn::SocketAndPort);
    let reserved = ClosedPort::new();
    state
        .remote_access
        .set_port(&state, reserved.port())
        .unwrap();
    assert_eq!(state.remote_access.info(&state).unwrap().connection, None);

    // Browser access serving gives another device's application nothing.
    state.remote_access.set_browser(&state, true).unwrap();
    assert_eq!(state.remote_access.info(&state).unwrap().connection, None);

    state.remote_access.set(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    if !private {
        assert_eq!(info.state, RemoteAccessState::Failed);
        assert_eq!(info.connection, None);
        drop(state.remote_access.close());
        return;
    }
    assert_eq!(info.state, RemoteAccessState::Serving, "{:?}", info.failure);
    let connection = info.connection.expect("the details while serving");
    assert!(
        connection.text.starts_with("octoboard+ssh://"),
        "{}",
        connection.text
    );
    assert!(!connection.text.contains(char::is_whitespace));
    let key = state.owner_key.current().unwrap();
    assert!(
        !connection.text.contains(&key),
        "the details carry no secret"
    );
    let modules = crate::connection_details::qr_modules(&connection.text).unwrap();
    assert_eq!(connection.qr.len(), modules.len());
    for (row, modules) in connection.qr.iter().zip(&modules) {
        let drawn: Vec<bool> = row.chars().map(|module| module == '1').collect();
        assert_eq!(&drawn, modules);
        assert!(row.chars().all(|module| module == '0' || module == '1'));
    }

    state.remote_access.set(&state, false).unwrap();
    assert_eq!(state.remote_access.info(&state).unwrap().connection, None);
    drop(state.remote_access.close());
}

/// The names and addresses another computer reaches this one by are told while the way in is
/// served, as this computer's own list gives them, and not while it is off or has failed.
#[tokio::test]
async fn the_server_addresses_are_given_while_serving_and_not_otherwise() {
    let (state, _dir) = port_daemon("remote-access-server-addresses");
    let reserved = ClosedPort::new();
    state
        .remote_access
        .set_port(&state, reserved.port())
        .unwrap();
    assert_eq!(
        state.remote_access.info(&state).unwrap().server_addresses,
        Vec::<String>::new()
    );

    state.remote_access.set_browser(&state, true).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        info.browser_state,
        RemoteAccessState::Serving,
        "{:?}",
        info.browser_failure
    );
    assert_eq!(
        info.server_addresses,
        crate::platform::net::server_addresses()
    );

    state.remote_access.set_browser(&state, false).unwrap();
    assert_eq!(
        state.remote_access.info(&state).unwrap().server_addresses,
        Vec::<String>::new()
    );
    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let failed = state.remote_access.info(&state).unwrap();
    assert_eq!(failed.browser_state, RemoteAccessState::Failed);
    assert_eq!(failed.server_addresses, Vec::<String>::new());
    drop(taken);
    drop(state.remote_access.close());
}

/// Turning browser access off after it failed leaves nothing of the failure: the state is off
/// with no reason, the setting is off, and turning it on again tries again.
#[tokio::test]
async fn turning_it_off_after_a_failure_clears_the_failure() {
    let (state, _dir) = port_daemon("remote-access-failure-cleared");
    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let failed = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (failed.browser_allowed, failed.browser_state),
        (true, RemoteAccessState::Failed)
    );

    state.remote_access.set_browser(&state, false).unwrap();
    let off = state.remote_access.info(&state).unwrap();
    assert_eq!(
        (off.browser_allowed, off.browser_state),
        (false, RemoteAccessState::Off)
    );
    assert_eq!(off.browser_failure, None);
    assert_eq!(off.browser_port, Some(taken_port), "the port setting stays");

    drop(taken);
    let_go(taken_port).await;
    state.remote_access.set_browser(&state, true).unwrap();
    let on = state.remote_access.info(&state).unwrap();
    assert_eq!(
        on.browser_state,
        RemoteAccessState::Serving,
        "{:?}",
        on.browser_failure
    );
    drop(state.remote_access.close());
}

/// A rotation that waits on a switch-off in progress finds the key no longer held, and
/// broadcasts nothing after it: the last word is that browser access is off.
#[tokio::test]
async fn a_rotation_racing_a_switch_off_does_not_outlive_it() {
    let (state, _dir) = port_daemon("remote-access-rotate-race");
    let reserved = ClosedPort::new();
    state
        .remote_access
        .set_port(&state, reserved.port())
        .unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let mut events = state.subscribe();

    let changing = state.remote_access.changing.lock().unwrap();
    let rotation = std::thread::spawn({
        let state = state.clone();
        move || state.remote_access.rotate_key(&state)
    });
    state.store.set_allow_browser_access(false).unwrap();
    state.remote_access.apply(&state);
    drop(changing);

    assert!(matches!(
        rotation.join().unwrap(),
        Err(RotateError::NotHeld)
    ));
    let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
        panic!("the switch-off was not broadcast");
    };
    assert_eq!(remote_access.browser_state, RemoteAccessState::Off);
    assert!(events.try_recv().is_err(), "nothing is broadcast after it");
    drop(state.remote_access.close());
}

/// Remote access turned off and straight back on opens the same browser port again: turning
/// it off frees the port at once, not once the accept loop's cancelled task next runs.
#[tokio::test]
async fn off_and_straight_back_on_reopens_the_same_port() {
    let (state, _dir) = port_daemon("remote-access-port-reopen");
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    assert_eq!(
        state.remote_access.info(&state).unwrap().browser_state,
        RemoteAccessState::Serving
    );

    state.remote_access.set_browser(&state, false).unwrap();
    // `let_go` checks the listener is closed before it awaits anything: before the cancelled
    // task can have run.
    let_go(port).await;
    state.remote_access.set_browser(&state, true).unwrap();
    let on = state.remote_access.info(&state).unwrap();
    assert_eq!(
        on.browser_state,
        RemoteAccessState::Serving,
        "{:?}",
        on.browser_failure
    );
    let key = state.owner_key.current().unwrap();
    assert!(port_takes(port, &key).await.unwrap());
    drop(state.remote_access.close());
}

/// Rotating the key is refused while nothing is served, as for a daemon that offers none,
/// whether browser access is off or failed; while serving it cuts off what the old key opened
/// and the browser port takes the new key alone.
#[tokio::test]
async fn a_rotation_is_refused_unless_serving_and_moves_the_browser_port_to_the_new_key() {
    let (state, _dir) = port_daemon("remote-access-port-rotate");
    let off = state.remote_access.rotate(&state).unwrap_err();
    assert_eq!(code_of(&off), error_code::REMOTE_ACCESS_UNAVAILABLE);

    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let failed = state.remote_access.rotate(&state).unwrap_err();
    assert_eq!(code_of(&failed), error_code::REMOTE_ACCESS_UNAVAILABLE);

    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    let old = state.owner_key.current().expect("serving at the new port");
    let mut admitted = state
        .owner_key
        .admit(Some(&format!("key={old}")))
        .expect("admitted");
    let mut events = state.subscribe();
    state.remote_access.rotate(&state).unwrap();
    let new = state.owner_key.current().unwrap();
    assert_ne!(old, new);
    let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
        panic!("the rotation was not broadcast");
    };
    assert_eq!(
        remote_access.address.as_deref(),
        Some(format!("http://127.0.0.1:{port}/?key={new}").as_str())
    );
    assert!(!port_takes(port, &old).await.unwrap());
    assert!(port_takes(port, &new).await.unwrap());
    tokio::time::timeout(PATIENCE, admitted.revoked())
        .await
        .expect("a rotation cuts off what the old key opened");
    drop(taken);
    drop(state.remote_access.close());
}

/// A port that could not be listened on is tried again when the same port is set again, and
/// setting a port while browser access is off is broadcast with the port and opens nothing.
#[tokio::test]
async fn the_same_port_again_retries_it_and_a_port_set_while_off_is_broadcast() {
    let (state, _dir) = port_daemon("remote-access-port-retry");
    let mut events = state.subscribe();
    let reserved = ClosedPort::new();
    let port = reserved.port();
    state.remote_access.set_port(&state, port).unwrap();
    let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
        panic!("the port was not broadcast");
    };
    assert_eq!(
        (remote_access.browser_port, remote_access.browser_state),
        (Some(port), RemoteAccessState::Off)
    );
    assert!(port_takes(port, &"0".repeat(64)).await.is_err());

    let (taken, _reserved) = held_port();
    let taken_port = taken.local_addr().unwrap().port();
    state.remote_access.set_port(&state, taken_port).unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    assert_eq!(
        state.remote_access.info(&state).unwrap().browser_state,
        RemoteAccessState::Failed
    );
    drop(taken);
    let_go(taken_port).await;
    state.remote_access.set_port(&state, taken_port).unwrap();
    let info = state.remote_access.info(&state).unwrap();
    assert_eq!(
        info.browser_state,
        RemoteAccessState::Serving,
        "{:?}",
        info.browser_failure
    );
    let key = state.owner_key.current().unwrap();
    assert!(port_takes(taken_port, &key).await.unwrap());
    drop(state.remote_access.close());
}

/// Changes asked at once, from requests on tasks of their own, are applied in the order they
/// are stored: however on and off and a change of port interleave, the way in ends open
/// exactly when the stored switch is on, and the owner key is held exactly then.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changes_asked_at_once_leave_the_way_in_as_the_switch_is_stored() {
    const ROUNDS: usize = 20;
    const AT_ONCE: usize = 6;
    let (state, _dir) = port_daemon("remote-access-race");
    // A port of its own for every change of port, held to the end: a port the daemon has let
    // go is never given to it again.
    let reserved: Vec<ClosedPort> = (0..=ROUNDS * AT_ONCE).map(|_| ClosedPort::new()).collect();
    state
        .remote_access
        .set_port(&state, reserved[0].port())
        .unwrap();
    for round in 0..ROUNDS {
        let tasks: Vec<_> = (0..AT_ONCE)
            .map(|i| {
                let state = state.clone();
                let port = reserved[1 + round * AT_ONCE + i].port();
                tokio::task::spawn_blocking(move || {
                    let remote_access = &state.remote_access;
                    match (round + i) % 3 {
                        0 => remote_access.set_browser(&state, true),
                        1 => remote_access.set_browser(&state, false),
                        _ => remote_access.set_port(&state, port),
                    }
                })
            })
            .collect();
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        let allowed = state.store.allow_browser_access().unwrap();
        let mut info = state.remote_access.info(&state).unwrap();
        // A turn on straight after a turn off can meet the copy a child process holds of the
        // listener just closed (see `let_go`) and find the port in use; once the copy is gone,
        // the same port set again tries it again. `let_go` refuses a listener of the daemon's
        // own.
        if info.browser_state == RemoteAccessState::Failed
            && info
                .browser_failure
                .as_ref()
                .is_some_and(|failure| failure.reason == RemoteAccessFailureReason::PortInUse)
        {
            let port = info.browser_port.expect("the port");
            let_go(port).await;
            state.remote_access.set_port(&state, port).unwrap();
            info = state.remote_access.info(&state).unwrap();
        }
        assert_eq!(state.owner_key.is_held(), allowed, "round {round}");
        let open = state.remote_access.inner.lock().unwrap().browser.open();
        assert_eq!(open, allowed, "round {round}: {info:?}");
    }
    drop(state.remote_access.close());
}

/// Turning it on while it is on changes nothing: the key stays, and what it opened is not cut
/// off.
#[tokio::test]
async fn turning_it_on_again_while_serving_cuts_nothing_off() {
    let (state, _dir) = port_daemon("remote-access-on-twice");
    let reserved = ClosedPort::new();
    state
        .remote_access
        .set_port(&state, reserved.port())
        .unwrap();
    state.remote_access.set_browser(&state, true).unwrap();
    let key = state.owner_key.current().expect("serving");
    let mut admitted = state
        .owner_key
        .admit(Some(&format!("key={key}")))
        .expect("admitted");
    state.remote_access.set_browser(&state, true).unwrap();
    assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
    assert!(futures_util::FutureExt::now_or_never(admitted.revoked()).is_none());
    assert_eq!(
        state.remote_access.info(&state).unwrap().browser_state,
        RemoteAccessState::Serving
    );
    drop(state.remote_access.close());
}

/// Once the daemon is stopping, nothing is opened any more: the setting is stored but no way
/// in or key follows.
#[tokio::test]
async fn nothing_is_opened_once_the_daemon_has_closed() {
    let (state, _dir) = port_daemon("remote-access-closed");
    let reserved = ClosedPort::new();
    state
        .remote_access
        .set_port(&state, reserved.port())
        .unwrap();
    drop(state.remote_access.close());
    state.remote_access.set_browser(&state, true).unwrap();
    assert!(!state.owner_key.is_held());
    assert_eq!(
        state.remote_access.info(&state).unwrap().browser_state,
        RemoteAccessState::Off
    );
}
