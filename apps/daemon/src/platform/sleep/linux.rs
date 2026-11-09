//! The Linux hold: a logind inhibitor lock on idle, taken over the system bus. logind hands back
//! a file descriptor and keeps the lock for as long as it is open, so closing it (or the process
//! ending) is the release. `systemd-inhibit --list` shows it under Octoboard's name. It is the
//! idle lock only, so a suspend the user asks for, or a closed lid, still sleeps the machine, as
//! on macOS and Windows.
//!
//! Where the session bus has a screen saver service (`org.freedesktop.ScreenSaver`, which GNOME,
//! KDE and others serve), an inhibition of it is added as well, so the desktop's own idle
//! blanking, lock and automatic suspend do not act. That one is best-effort: a desktop without the
//! service, one that refuses, or one too slow to answer leaves the logind lock standing alone.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedFd;

const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const SCREEN_SAVER: &str = "org.freedesktop.ScreenSaver";
const SCREEN_SAVER_PATH: &str = "/org/freedesktop/ScreenSaver";
/// The name the locks are taken under.
const WHO: &str = "Octoboard";
/// How long a call on either bus may take once connected (`method_timeout`), and how long each
/// stage of the probe and the acquisition waits as a whole. Building a connection (the handshake)
/// has no timeout of its own, so those are bounded by [`bounded`] instead.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs `work` on a thread of its own and waits at most [`CALL_TIMEOUT`] for it, or `None` when it
/// takes longer. The thread is left to finish by itself, and its result is dropped then.
fn bounded<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.recv_timeout(CALL_TIMEOUT).ok()
}

fn system_bus() -> zbus::Result<Connection> {
    Builder::system()?.method_timeout(CALL_TIMEOUT).build()
}

fn session_bus() -> zbus::Result<Connection> {
    Builder::session()?.method_timeout(CALL_TIMEOUT).build()
}

/// The locks taken, released when it is dropped.
pub(super) struct Held {
    /// logind's lock; it is released when the descriptor closes.
    _lock: OwnedFd,
    screen_saver: Option<ScreenSaverInhibition>,
}

/// An inhibition of the session's screen saver, which the service drops when its holder leaves the
/// bus or asks for the cookie to be released.
struct ScreenSaverInhibition {
    connection: Connection,
    cookie: u32,
}

impl Held {
    /// Takes the locks. The logind lock is the one that counts: it fails when the system bus takes
    /// longer than [`CALL_TIMEOUT`] to answer, and a lock taken after that is released at once, as
    /// nobody is left to own it. The screen saver inhibition is bounded on its own and left out
    /// when it is slow, so it cannot cost the logind lock; one taken late is dropped on its
    /// thread, and closing its connection ends it.
    pub(super) fn acquire(reason: &str) -> Result<Self> {
        let reason = reason.to_owned();
        let lock = {
            let reason = reason.clone();
            bounded(move || take_lock(&reason))
                .unwrap_or_else(|| Err(anyhow::anyhow!("the system bus did not answer in time")))?
        };
        let screen_saver = bounded(move || inhibit_screen_saver(&reason)).flatten();
        Ok(Self {
            _lock: lock,
            screen_saver,
        })
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(inhibition) = self.screen_saver.take() {
            // Best-effort: the service drops the inhibition anyway once the connection closes.
            let _ = inhibition.connection.call_method(
                Some(SCREEN_SAVER),
                SCREEN_SAVER_PATH,
                Some(SCREEN_SAVER),
                "UnInhibit",
                &(inhibition.cookie,),
            );
        }
    }
}

/// Takes logind's inhibitor lock over the system bus.
fn take_lock(reason: &str) -> Result<OwnedFd> {
    let system = system_bus().context("connecting to the system bus failed")?;
    let reply = system
        .call_method(
            Some(LOGIND),
            LOGIND_PATH,
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("idle", WHO, reason, "block"),
        )
        .context("logind refused the inhibitor lock")?;
    reply
        .body()
        .deserialize()
        .context("logind's reply holds no lock")
}

/// Inhibits the session's screen saver, or `None` when no session bus is reachable or nothing on
/// it serves one.
fn inhibit_screen_saver(reason: &str) -> Option<ScreenSaverInhibition> {
    let connection = session_bus().ok()?;
    let cookie: u32 = connection
        .call_method(
            Some(SCREEN_SAVER),
            SCREEN_SAVER_PATH,
            Some(SCREEN_SAVER),
            "Inhibit",
            &(WHO, reason),
        )
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    Some(ScreenSaverInhibition { connection, cookie })
}

/// Whether logind answers on the system bus, which is what a lock needs; a bus that does not
/// answer in time counts as no.
pub(super) fn available() -> bool {
    bounded(|| {
        system_bus()
            .and_then(|system| {
                system.call_method(
                    Some(LOGIND),
                    LOGIND_PATH,
                    Some("org.freedesktop.DBus.Peer"),
                    "Ping",
                    &(),
                )
            })
            .is_ok()
    })
    .unwrap_or(false)
}
