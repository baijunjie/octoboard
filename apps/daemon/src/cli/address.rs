//! `address`: the address to open the running daemon at in a browser, its key included, and
//! `address --rotate`, which replaces the owner key first.

use std::path::Path;

use anyhow::{Context, Result};

use crate::instance_lock;
use crate::owner_record::Owner;

use super::background::{not_running, request_over_loopback, Asked, NOT_RUNNING, RESTART_WAIT};
use super::succession::{find_daemon, Found};

/// Prints the address to open the running daemon at in a browser, its key included, as one line on
/// stdout: `http://127.0.0.1:<port>/?key=<key>`, a loopback address a browser treats as a secure
/// context, and the one address the forward `octoboardd address` prints binds on the owner's
/// machine. Exits 3 when no daemon runs. The key is the owner key when the daemon's record says it
/// accepts one and `owner.key` holds one, which outlives the daemon, else the launch key from the
/// daemon's owner-only record. Only the daemon's own user obtains either.
///
/// With `rotate`, the owner key is replaced first (see [`rotate_owner_key`]) and the address
/// printed is the new one's, except where the daemon does not take an owner key: it is the launch
/// key's, and the new owner key is for the next daemon that serves the browser.
pub(super) fn print_address(rotate: bool) -> Result<i32> {
    print_address_of(rotate, false)
}

/// [`print_address`], waiting through a daemon that holds the lock but has not recorded itself when
/// `through_unrecorded`: it is a daemon this command saw start, whose record is yet to come.
fn print_address_of(rotate: bool, through_unrecorded: bool) -> Result<i32> {
    let owner = match find_daemon(RESTART_WAIT, through_unrecorded)? {
        Found::NotRunning { .. } => {
            if rotate {
                return match rotate_without_daemon(
                    &crate::paths::data_dir(),
                    instance_lock::is_running,
                )? {
                    Some(status) => Ok(status),
                    // A daemon started while the file was being replaced, and may have read the
                    // earlier key before it was: rotate through the daemon, which keeps the file
                    // and its memory alike. It has only just started, so its record may be still
                    // to come: wait through a daemon that has not recorded itself yet.
                    None => print_address_of(true, true),
                };
            }
            eprintln!("{}", not_running());
            return Ok(NOT_RUNNING);
        }
        Found::Running { owner, .. } => owner,
        Found::Unrecorded => {
            eprintln!(
                "{} is running, but its address is not known (it may still be starting or \
                 stopping)",
                crate::APP_NAME
            );
            return Ok(1);
        }
    };
    let data = crate::paths::data_dir();
    let Some(key) = owner.recorded_key() else {
        eprintln!("{}", address_key_not_recorded(&owner));
        return Ok(1);
    };
    if rotate && owner.takes_owner_key {
        return rotate_owner_key(&owner, &key, &data);
    }
    if rotate {
        return rotate_for_daemon_without_owner_key(&owner, &key, &data);
    }
    let (address, lasting) = address_to_print(&owner, &key, &data);
    if !lasting {
        eprintln!(
            "the address printed lasts only until {} restarts, as owner.key is missing or holds no \
             usable key; `{} address --rotate` makes a lasting one",
            crate::APP_NAME,
            crate::APP_NAME
        );
    }
    println!("{address}");
    if owner.takes_owner_key {
        let shown = crate::owner_key::read_in(&data).unwrap_or_else(|| key.clone());
        note_browser_forward(&owner, &data, &shown);
    } else if crate::browser_socket::AVAILABLE {
        eprintln!("{}", crate::browser_socket::no_browser_note());
    }
    Ok(0)
}

/// Says on stderr how the address of `owner`, a daemon that serves the browser on its socket, is
/// reached from the owner's machine, and whether the daemon accepts `key` there.
fn note_browser_forward(owner: &Owner, data: &Path, key: &str) {
    for note in crate::browser_socket::address_notes(owner.port, data, key) {
        eprintln!("{note}");
    }
}

/// The address `address` prints for `owner`, and whether it is one that outlives the daemon: with
/// the owner key when the daemon accepts one and `data` holds one, else with the launch key `key`.
/// A daemon that takes no owner key has no lasting address to lose, so it counts as one.
fn address_to_print(owner: &Owner, key: &str, data: &Path) -> (String, bool) {
    let owner_key = owner
        .takes_owner_key
        .then(|| crate::owner_key::read_in(data))
        .flatten();
    let lasting = owner_key.is_some() || !owner.takes_owner_key;
    (
        browser_address(owner.port, owner_key.as_deref().unwrap_or(key)),
        lasting,
    )
}

/// `address --rotate` for a running daemon that takes no owner key: replaces the owner key in
/// `data` for the next daemon that serves the browser, says so on stderr, and prints the launch-key
/// address of the running one.
fn rotate_for_daemon_without_owner_key(owner: &Owner, key: &str, data: &Path) -> Result<i32> {
    crate::owner_key::replace_in(data).context("replacing the owner key")?;
    eprintln!(
        "the running {} does not take the owner key (it follows the desktop application, is an \
         earlier build, serves no browser, or could not read or write owner.key at its start); the \
         next daemon that serves the browser takes the new one",
        crate::APP_NAME
    );
    println!("{}", browser_address(owner.port, key));
    Ok(0)
}

/// Why `address` cannot print the address of a daemon whose launch key is not recorded.
fn address_key_not_recorded(owner: &Owner) -> String {
    format!(
        "{} is running (pid {}, port {}), but its key is not recorded: it may be an earlier \
         build, which a restart with this build replaces, or one that could not record it, \
         as its log says",
        crate::APP_NAME,
        owner.identity.pid.0,
        owner.port
    )
}

/// Asks the running daemon `owner` names to replace its owner key, presenting its launch key
/// `key`, and prints the address with the new one. A refusal or a failure is said on stderr, with
/// status 1 and nothing on stdout.
fn rotate_owner_key(owner: &Owner, key: &str, data: &Path) -> Result<i32> {
    let failed = |what: String| {
        eprintln!("the owner key was not rotated: {what}");
        Ok(1)
    };
    match request_over_loopback(owner, key, "/owner-key/rotate") {
        Ok(Asked::Accepted) => {}
        Ok(Asked::Refused(message)) => return failed(format!("the daemon refused: {message}")),
        Ok(Asked::Gone) => return failed("the daemon no longer accepts connections".to_owned()),
        Err(err) => return failed(format!("{err:#}")),
    }
    match crate::owner_key::read_in(data) {
        Some(new) => {
            println!("{}", browser_address(owner.port, &new));
            note_browser_forward(owner, data, &new);
            Ok(0)
        }
        None => failed("the daemon took the request, but owner.key holds no key".to_owned()),
    }
}

/// `address --rotate` with no daemon running: replaces the owner key in the data directory `data`
/// for the next daemon that serves the browser, unless there is no data directory, which it does
/// not create. Prints nothing on stdout, and exits 3 as `address` does.
///
/// The instance lock cannot be held while the file is written (it is the daemon's for its
/// lifetime, and a probe holds it for a moment), so `running` is asked again afterwards: `None`
/// when a daemon holds the lock by then, which the caller rotates through instead.
fn rotate_without_daemon(data: &Path, running: impl Fn() -> Result<bool>) -> Result<Option<i32>> {
    if !data.is_dir() {
        eprintln!("{}", not_running());
        return Ok(Some(NOT_RUNNING));
    }
    crate::owner_key::replace_in(data).context("replacing the owner key")?;
    if running()? {
        return Ok(None);
    }
    eprintln!(
        "{} is not running; the next daemon that serves the browser takes the new owner key",
        crate::APP_NAME
    );
    Ok(Some(NOT_RUNNING))
}

fn browser_address(port: u16, key: &str) -> String {
    format!("http://127.0.0.1:{port}/?key={key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cli::background::tests::{answer_once, owner_on, KEY};

    #[test]
    fn the_browser_address_is_the_loopback_address_at_the_port_with_the_key() {
        assert_eq!(
            browser_address(7531, KEY),
            format!("http://127.0.0.1:7531/?key={KEY}")
        );
    }

    const OWNER_KEY: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn owner_taking_key_on(port: u16) -> Owner {
        owner_on(port).with_owner_key(true)
    }

    /// The owner key is printed only for a daemon that accepts one and only when the file holds a
    /// key; every other case gets the launch key.
    #[test]
    fn the_address_has_the_owner_key_only_where_the_daemon_takes_it() {
        let dir = crate::test_support::ScratchDir::new("address-owner-key");
        let launch = format!("http://127.0.0.1:7531/?key={KEY}");
        let owner = format!("http://127.0.0.1:7531/?key={OWNER_KEY}");
        // No file.
        assert_eq!(
            address_to_print(&owner_taking_key_on(7531), KEY, &dir),
            (launch.clone(), false)
        );
        crate::owner_key::write_in(&dir, OWNER_KEY);
        assert_eq!(
            address_to_print(&owner_taking_key_on(7531), KEY, &dir),
            (owner, true)
        );
        // A daemon without the token (desktop application, earlier build).
        assert_eq!(
            address_to_print(&owner_on(7531), KEY, &dir),
            (launch.clone(), true)
        );
        // A file that holds no key.
        std::fs::write(crate::owner_key::path_in(&dir), "nonsense\n").unwrap();
        assert_eq!(
            address_to_print(&owner_taking_key_on(7531), KEY, &dir),
            (launch, false)
        );
    }

    /// The rotation goes to `/owner-key/rotate` with the launch key, and on a `204` the address
    /// printed is of the key the daemon wrote.
    #[test]
    fn a_rotation_asks_with_the_launch_key_and_reads_the_new_key() {
        let dir = crate::test_support::ScratchDir::new("rotate-asked");
        crate::owner_key::write_in(&dir, OWNER_KEY);
        let (port, served) = answer_once("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        let status = rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap();
        assert_eq!(status, 0);
        assert_eq!(
            served.join().unwrap(),
            format!("POST /owner-key/rotate?key={KEY} HTTP/1.1\r\n")
        );
    }

    /// A refusal or a failed request is status 1, and the owner key is not read.
    #[test]
    fn a_refused_or_failed_rotation_is_status_1() {
        let dir = crate::test_support::ScratchDir::new("rotate-refused");
        crate::owner_key::write_in(&dir, OWNER_KEY);
        let refusal = r#"{"message":"this daemon holds no owner key"}"#;
        for response in [
            "HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n".to_owned(),
            "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\n".to_owned(),
            format!(
                "HTTP/1.1 409 Conflict\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{refusal}",
                refusal.len()
            ),
        ] {
            let (port, served) = answer_once(response.clone());
            assert_eq!(
                rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap(),
                1,
                "{response}"
            );
            served.join().unwrap();
        }
        // A daemon that took the request but left no key behind.
        std::fs::remove_file(crate::owner_key::path_in(&dir)).unwrap();
        let (port, served) = answer_once("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        assert_eq!(
            rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap(),
            1
        );
        served.join().unwrap();
    }

    /// With no daemon, a rotation replaces the file when the data directory exists and creates
    /// nothing when it does not; both are status 3.
    #[test]
    fn a_rotation_with_no_daemon_replaces_the_file_only_in_an_existing_data_directory() {
        let dir = crate::test_support::ScratchDir::new("rotate-no-daemon");
        crate::owner_key::write_in(&dir, OWNER_KEY);
        assert_eq!(
            rotate_without_daemon(&dir, || Ok(false)).unwrap(),
            Some(NOT_RUNNING)
        );
        let new = crate::owner_key::read_in(&dir).expect("a key");
        assert_ne!(new, OWNER_KEY);

        let missing = dir.join("no-such-data-directory");
        assert_eq!(
            rotate_without_daemon(&missing, || Ok(false)).unwrap(),
            Some(NOT_RUNNING)
        );
        assert!(!missing.exists());
    }

    /// A daemon that takes no owner key is told so and the file is replaced for the next one; the
    /// address is its launch key's.
    #[test]
    fn a_rotation_for_a_daemon_without_an_owner_key_replaces_the_file_and_exits_0() {
        let dir = crate::test_support::ScratchDir::new("rotate-daemon-without");
        crate::owner_key::write_in(&dir, OWNER_KEY);
        assert_eq!(
            rotate_for_daemon_without_owner_key(&owner_on(7531), KEY, &dir).unwrap(),
            0
        );
        let new = crate::owner_key::read_in(&dir).expect("a key");
        assert_ne!(new, OWNER_KEY);
    }

    /// A daemon that holds the lock once the file is written is not told the next one takes the
    /// key: the caller rotates through it.
    #[test]
    fn a_daemon_that_started_during_a_rotation_without_one_is_rotated_through() {
        let dir = crate::test_support::ScratchDir::new("rotate-raced");
        assert_eq!(rotate_without_daemon(&dir, || Ok(true)).unwrap(), None);
    }

    /// A failure the daemon gives a message for, as a failed write of the new owner key, is status 1
    /// as well; the message goes to stderr.
    #[test]
    fn a_failed_rotation_with_the_daemons_message_is_status_1() {
        let dir = crate::test_support::ScratchDir::new("rotate-failed-message");
        crate::owner_key::write_in(&dir, OWNER_KEY);
        let body = r#"{"reason":"write_failed","message":"writing owner.key failed"}"#;
        let response = format!(
            "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (port, served) = answer_once(response);
        assert_eq!(
            rotate_owner_key(&owner_taking_key_on(port), KEY, &dir).unwrap(),
            1
        );
        served.join().unwrap();
    }
}
