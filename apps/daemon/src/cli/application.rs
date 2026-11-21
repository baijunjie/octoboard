//! `start` and `stop` of the desktop application's own copy of `octoboardd`, whose daemon is the
//! application's: `start` launches the application in the background and waits for its daemon,
//! and `stop` asks the application to quit, without its quit confirmation, which a quit ends the
//! daemon with. The copy recognises itself as `proxy` does ([`is_application_copy`]); where the
//! host does not manage the application this way ([`application::MANAGED`]), the copy starts and
//! stops a daemon like the server form's.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::instance_lock;
use crate::owner_record::Owner;
use crate::platform::application;
use crate::platform::proc::{self, Sent};

use super::succession::POLL;

/// How long `start` waits for the application's daemon to listen: the application opens its
/// store and sweeps what the last run left, as a daemon `start` starts does.
const START_WAIT: Duration = Duration::from_secs(30);

/// How long `stop` waits for the application to quit and its daemon to end.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// Whether this executable is the desktop application's own copy, and the host starts and quits
/// the application from it.
pub(super) fn is_application_copy() -> bool {
    application::MANAGED
        && proc::running_image()
            .is_ok_and(|exe| crate::platform::is_application_copy(&exe, crate::APP_NAME))
}

/// What `start` says to a `--port`, which the application would not take.
fn port_refusal() -> String {
    format!(
        "the {} application picks its own port, so --port is refused; nothing was started: \
         run `start` without it",
        crate::APP_NAME
    )
}

/// `start`, with no daemon running: launches the application in the background, waits for its
/// daemon to listen and prints what the server form's `start` prints. The application picks its
/// own port, so a `--port` is refused, with nothing started. The exit status.
pub(super) fn start(port: Option<u16>) -> Result<i32> {
    if port.is_some() {
        eprintln!("{}", port_refusal());
        return Ok(1);
    }
    let exe = proc::running_image().context("finding this executable")?;
    if let Err(err) = application::launch_in_background(&exe) {
        eprintln!(
            "the {} application could not be started: {err}",
            crate::APP_NAME
        );
        return Ok(1);
    }
    let deadline = Instant::now() + START_WAIT;
    loop {
        if let Some(owner) = listening_daemon() {
            if let Some(key) = owner.recorded_key() {
                println!("{}", super::listening_line(owner.port, &key));
            }
            println!(
                "{}",
                super::background::started_line(owner.identity.pid.0, Some(owner.port))
            );
            return Ok(0);
        }
        if Instant::now() >= deadline {
            eprintln!(
                "the {} application was started, but its daemon did not report listening within \
                 {} seconds; open the application to see why",
                crate::APP_NAME,
                START_WAIT.as_secs()
            );
            return Ok(1);
        }
        std::thread::sleep(POLL);
    }
}

/// The application's daemon, once it holds the lock and has recorded itself.
fn listening_daemon() -> Option<Owner> {
    instance_lock::is_running().ok()?.then_some(())?;
    Owner::recorded().filter(|owner| owner.follows_application == Some(true))
}

/// `stop` of the application's daemon `owner` names, once the check for running work has let it
/// go ahead: asks the application to quit, without its quit confirmation, and waits for it and
/// its daemon to end. Only a parent that runs the bundle of this executable is asked; any other
/// is said so and left alone. The exit status.
pub(super) fn stop(owner: &Owner) -> Result<i32> {
    let exe = proc::running_image().context("finding this executable")?;
    let Some(app) = application::of_daemon(&owner.identity) else {
        eprintln!(
            "the {} application that runs the daemon (pid {}) could not be found; quit it \
             from its own menu",
            crate::APP_NAME,
            owner.identity.pid.0
        );
        return Ok(1);
    };
    if !application::runs_bundle_of(&app, &exe) {
        eprintln!(
            "the process that runs the daemon (pid {}) is not the {} application of this \
             installation, so it was not asked to quit; nothing was stopped",
            app.pid.0,
            crate::APP_NAME
        );
        return Ok(1);
    }
    match application::ask_to_quit(&app) {
        Sent::Delivered | Sent::Gone => {}
        Sent::Failed(err) => {
            return Err(err).context(format!("signalling the application, pid {}", app.pid.0))
        }
    }
    let deadline = Instant::now() + STOP_WAIT;
    while app.is_running() || owner.identity.is_running() || instance_lock::is_running()? {
        if Instant::now() >= deadline {
            eprintln!(
                "the {} application is still quitting after {} seconds (pid {})",
                crate::APP_NAME,
                STOP_WAIT.as_secs(),
                app.pid.0
            );
            return Ok(1);
        }
        std::thread::sleep(POLL);
    }
    println!("{} quit", crate::APP_NAME);
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A port is refused before the application is launched, which would not take it.
    #[test]
    fn start_refuses_a_port_and_launches_nothing() {
        assert_eq!(start(Some(3370)).expect("refused, not failed"), 1);
        let said = port_refusal();
        assert!(said.contains("--port"), "{said}");
        assert!(!said.contains("  "), "{said}");
    }
}
