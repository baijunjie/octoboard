//! The browser's way in, in the server form: a socket in the data directory, `browser.sock`, that
//! only the daemon's user can reach. The owner key is accepted on it alone, and the embedded UI is
//! served there; the owner forwards it from their own machine with
//! `ssh -L 127.0.0.1:<port>:<socket path> <server>`, so the browser still opens
//! `http://127.0.0.1:<port>`.
//!
//! A loopback TCP port cannot do this job: every user of the host can bind one, and a port the
//! daemon gives up for a moment (a planned restart, a service manager's restart) can be taken by
//! another user, whom the browser's reconnect, through the owner's own forward, would then hand
//! the owner key. A socket's path is a file: nobody but its user can create, replace or connect to
//! one in a directory only that user can enter, so the socket is the daemon's for as long as the
//! data directory is private. [`bind`] checks that it is, and that no directory above it lets
//! anyone else rename it away, before it listens; where either fails, or the host has no such
//! sockets, the daemon serves no browser.
//!
//! A stale socket file left by a daemon that died is replaced only under the instance lock, which
//! the caller holds, so it is never a live daemon's socket. The file is never removed at exit: the
//! next daemon may already be listening on it by the time this one is done.
//!
//! See `apps/daemon/PROTOCOL.md`, "Who may connect", for what each listener accepts.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::platform::local_socket::{self, Listener};

/// The socket's file name in the data directory.
const FILE_NAME: &str = "browser.sock";

/// Whether this host can serve the browser on a socket of this kind.
pub const AVAILABLE: bool = local_socket::AVAILABLE;

/// The socket the daemon whose data directory is `data` serves the browser on.
pub fn path_in(data: &Path) -> PathBuf {
    data.join(FILE_NAME)
}

/// Listens on the browser socket of the data directory `data`, once the directory and every one
/// above it are found private enough that nobody else can reach or replace it. A stale socket file
/// there is replaced; the caller must hold the instance lock.
pub fn bind(data: &Path) -> Result<Listener> {
    local_socket::check_private_tree(data).with_context(|| {
        format!(
            "{} is not private to this user; `chmod 700` it, and keep the directories above it \
             writable by nobody else",
            data.display()
        )
    })?;
    let path = path_in(data);
    Listener::bind_replacing_stale(&path).map_err(|err| {
        // The system refuses a socket path longer than its address holds as an invalid argument.
        let hint = if err.kind() == std::io::ErrorKind::InvalidInput {
            " (a socket's path has to be shorter than about 100 bytes)"
        } else {
            ""
        };
        anyhow::Error::new(err).context(format!("listening on {}{hint}", path.display()))
    })
}

/// Serves `router` on `listener` until the returned task is dropped: each connection on a task of
/// its own, WebSocket upgrades included, as `axum::serve` serves the TCP port. A connection already
/// open runs on past the drop, as one on the TCP port does once `axum::serve` is gone.
pub fn serve(listener: Listener, router: axum::Router) -> ServeTask {
    ServeTask(tokio::spawn(async move {
        loop {
            let stream = match listener.accept().await {
                Ok(stream) => stream,
                Err(err) => {
                    tracing::warn!(%err, "accepting a browser socket connection failed");
                    // A failure such as running out of descriptors passes; a busy loop does not
                    // help it pass.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let service = hyper_util::service::TowerToHyperService::new(router.clone());
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                if let Err(err) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .with_upgrades()
                    .await
                {
                    tracing::debug!(%err, "a browser socket connection ended with an error");
                }
            });
        }
    }))
}

/// The accept loop [`serve`] runs; ended when dropped.
pub struct ServeTask(tokio::task::JoinHandle<()>);

impl Drop for ServeTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Whether the daemon whose data directory is `data` accepts `key` on its browser socket, asked as
/// a browser would ask (`GET /key-check`), naming `port` as the address's port. An error when the
/// socket cannot be reached or answers anything else.
pub fn key_accepted(data: &Path, port: u16, key: &str, timeout: Duration) -> std::io::Result<bool> {
    let mut stream = local_socket::connect(&path_in(data), timeout)?;
    let request = format!(
        "GET /key-check?key={key} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let status = std::str::from_utf8(&response)
        .ok()
        .and_then(|text| text.split(' ').nth(1))
        .and_then(|status| status.parse::<u16>().ok());
    match status {
        Some(204) => Ok(true),
        Some(401) => Ok(false),
        other => Err(std::io::Error::other(format!(
            "the browser socket answered {}",
            other.map_or_else(
                || "nothing readable".to_owned(),
                |status| status.to_string()
            )
        ))),
    }
}

/// How long `octoboardd address` waits on the daemon's answer to its key check.
const CHECK_WAIT: Duration = Duration::from_secs(5);

/// What `octoboardd address` says on stderr beside the address of a daemon that serves the browser
/// on its socket (`data`'s), the address being for `port` with `key`: a warning when the daemon
/// does not accept `key` there, as after `owner.key` was changed behind its back, or the socket
/// cannot be asked, and the command that forwards the socket.
pub fn address_notes(port: u16, data: &Path, key: &str) -> Vec<String> {
    let mut notes = Vec::new();
    match key_accepted(data, port, key, CHECK_WAIT) {
        Ok(true) => {}
        Ok(false) => notes.push(format!(
            "the running {} refuses the key in this address (owner.key may have been changed \
             since it started); `{} address --rotate` makes one it accepts",
            crate::APP_NAME,
            crate::APP_NAME
        )),
        Err(err) => notes.push(format!(
            "whether the running {} accepts this address could not be checked on {}: {err}",
            crate::APP_NAME,
            path_in(data).display()
        )),
    }
    // The user the daemon's files belong to, which `USER` need not name (under `su` or `sudo`).
    let user = crate::platform::fs::account_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|user| !user.is_empty());
    notes.push(format!(
        "forward it to your own machine with `{}`, then open the address there",
        forward_command(port, &path_in(data), user.as_deref())
    ));
    notes
}

/// What `octoboardd address` says on stderr beside the address of a daemon that serves no browser
/// on a host where one would be served on a socket: no page opens at that address.
pub fn no_browser_note() -> String {
    format!(
        "this {app} serves no browser page: it follows the desktop application, its build embeds \
         no UI, or its data directory is not private to its user or its browser socket could not \
         be opened, as its log says; the address is for programs that take its launch key",
        app = crate::APP_NAME
    )
}

/// The command a browser's machine forwards the socket with, for `octoboardd address`: the local
/// port is `port`, the remote end is the socket at `socket`, and the login is `user`'s on a
/// `<server>` the owner names. `ExitOnForwardFailure` makes `ssh` give up rather than run on when
/// something else already holds the local port, which would then be what the browser reaches; the
/// forward binds `127.0.0.1` alone, since with no address `ssh` binds `::1` too and counts a
/// forward that got only one of the two as made, the other being someone else's.
pub fn forward_command(port: u16, socket: &Path, user: Option<&str>) -> String {
    let login = match user {
        Some(user) => format!("{user}@<server>"),
        None => "<server>".to_owned(),
    };
    format!(
        "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:{port}:{} {login}",
        shell_quoted(&socket.to_string_lossy()),
    )
}

/// `text` as one word for a POSIX shell: as it is when it holds nothing the shell would read,
/// else in single quotes.
fn shell_quoted(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%,=".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ScratchDir, PATIENCE};

    #[test]
    fn the_forward_command_names_the_port_the_socket_and_the_login() {
        assert_eq!(
            forward_command(
                3369,
                Path::new("/home/alice/.octoboard/browser.sock"),
                Some("alice")
            ),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:/home/alice/.octoboard/browser.sock \
             alice@<server>"
        );
        assert_eq!(
            forward_command(4000, Path::new("/srv/o b/browser.sock"), None),
            "ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:4000:'/srv/o b/browser.sock' <server>"
        );
        assert_eq!(shell_quoted("it's"), r"'it'\''s'");
    }

    /// A data directory others can enter gets no socket, and nothing is created in it.
    #[cfg(unix)]
    #[test]
    fn a_data_directory_others_can_enter_gets_no_socket() {
        use crate::platform::local_socket::test_api::set_mode;

        let dir = ScratchDir::new("bsock-open");
        let data = dir.join("data");
        std::fs::create_dir(&data).unwrap();
        set_mode(&data, 0o750);
        let Err(refused) = bind(&data) else {
            panic!("a directory others can enter is refused");
        };
        assert!(format!("{refused:#}").contains("chmod 700"), "{refused:#}");
        assert!(!path_in(&data).exists());
    }

    /// A private data directory gets a socket only its user may use. The temporary directory's
    /// own ancestors may let others in (a shared volume), which no test can change: there the
    /// refusal is checked instead.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_private_data_directory_gets_an_owner_only_socket() {
        use crate::platform::local_socket::test_api::{mode_of, set_mode};

        let dir = ScratchDir::new("bsock-private");
        let data = dir.join("data");
        std::fs::create_dir(&data).unwrap();
        set_mode(&data, 0o700);
        match local_socket::check_private_tree(&data) {
            Ok(()) => {
                let _listener = bind(&data).expect("a private directory is served");
                assert_eq!(mode_of(&path_in(&data)) & 0o777, 0o600);
            }
            Err(_) => {
                assert!(bind(&data).is_err());
                assert!(!path_in(&data).exists());
            }
        }
    }

    /// A router that answers a key check as the daemon's does: 204 to the key `good`, 401 to any
    /// other.
    #[cfg(unix)]
    fn key_check_router() -> axum::Router {
        use axum::http::StatusCode;
        use axum::routing::get;

        axum::Router::new().route(
            "/key-check",
            get(
                |axum::extract::RawQuery(query): axum::extract::RawQuery| async move {
                    if query.as_deref() == Some("key=good") {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::UNAUTHORIZED
                    }
                },
            ),
        )
    }

    /// The key check over the socket tells an accepted key from a refused one, as the router
    /// answers them, through the same accept loop the daemon serves the browser with.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_key_check_goes_over_the_socket() {
        let dir = ScratchDir::new("bsock-check");
        let listener = Listener::bind_replacing_stale(&path_in(&dir)).unwrap();
        let _served = serve(listener, key_check_router());
        let data = dir.to_path_buf();
        let answers = tokio::task::spawn_blocking(move || {
            (
                key_accepted(&data, 3369, "good", PATIENCE).unwrap(),
                key_accepted(&data, 3369, "bad", PATIENCE).unwrap(),
            )
        })
        .await
        .unwrap();
        assert_eq!(answers, (true, false));
    }

    /// A socket that answers a key check with anything but 204 or 401 cannot say whether the key is
    /// accepted: that is an error, not a refusal, and neither is a socket nobody listens on.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_socket_that_answers_otherwise_or_not_at_all_cannot_be_asked() {
        use axum::http::StatusCode;
        use axum::routing::get;

        let dir = ScratchDir::new("bsock-odd");
        let data = dir.to_path_buf();
        let missing = tokio::task::spawn_blocking({
            let data = data.clone();
            move || key_accepted(&data, 3369, "good", PATIENCE)
        })
        .await
        .unwrap();
        assert!(missing.is_err(), "no socket there");

        let listener = Listener::bind_replacing_stale(&path_in(&dir)).unwrap();
        let _served = serve(
            listener,
            axum::Router::new().route(
                "/key-check",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            ),
        );
        let odd = tokio::task::spawn_blocking(move || key_accepted(&data, 3369, "good", PATIENCE))
            .await
            .unwrap();
        assert!(odd.is_err(), "a 500 is no answer to the question");
    }

    /// What `octoboardd address` says beside the address, asked of a daemon on its socket: when the
    /// daemon accepts the key there is only the forward command; when it refuses the key, a warning
    /// comes first; when the socket cannot be asked, a warning naming the socket comes first. The
    /// forward command is there each time.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_notes_beside_an_address_warn_of_a_key_the_daemon_refuses() {
        let dir = ScratchDir::new("bsock-notes");
        let data = dir.to_path_buf();
        let notes_for = |key: &'static str| {
            let data = data.clone();
            tokio::task::spawn_blocking(move || address_notes(3369, &data, key))
        };
        let assert_forward = |note: &str| {
            assert!(
                note.contains("ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:3369:"),
                "{note}"
            );
            assert!(note.contains("browser.sock"), "{note}");
            assert!(note.contains("<server>"), "{note}");
        };

        // Nothing listens yet.
        let notes = notes_for("good").await.unwrap();
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(
            notes[0].contains(&path_in(&data).display().to_string()),
            "the warning says which socket: {notes:?}"
        );
        assert_forward(&notes[1]);

        let listener = Listener::bind_replacing_stale(&path_in(&dir)).unwrap();
        let _served = serve(listener, key_check_router());

        let notes = notes_for("good").await.unwrap();
        assert_eq!(
            notes.len(),
            1,
            "an accepted key warns of nothing: {notes:?}"
        );
        assert_forward(&notes[0]);

        let notes = notes_for("bad").await.unwrap();
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("refuses"), "{notes:?}");
        assert!(notes[0].contains("--rotate"), "{notes:?}");
        assert_forward(&notes[1]);
    }

    /// A word is handed to the shell as it is only when the shell would read nothing in it.
    #[test]
    fn a_word_the_shell_would_read_is_quoted() {
        assert_eq!(
            shell_quoted("/home/a/.octoboard/browser.sock"),
            "/home/a/.octoboard/browser.sock"
        );
        assert_eq!(shell_quoted(""), "''");
        assert_eq!(shell_quoted("a b"), "'a b'");
        assert_eq!(shell_quoted("$HOME/x"), "'$HOME/x'");
        assert_eq!(shell_quoted("a;b"), "'a;b'");
        assert_eq!(shell_quoted("~/x"), "'~/x'");
    }

    /// The daemon's own router, served on a real socket: the owner key, the launch key and no key
    /// are told apart; a WebSocket upgrade goes through; and a rotation ends the connection the
    /// owner key opened, while the one the launch key opened stays.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_browser_router_serves_the_owner_key_over_the_socket() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = crate::test_support::app_state("bsock-router");
        assert!(state.owner_key.start(true, &dir));
        let owner = crate::owner_key::read_in(&dir).expect("the owner key");
        let path = path_in(&dir);
        let listener = Listener::bind_replacing_stale(&path).unwrap();
        let _served = serve(listener, crate::server::browser_router(state.clone()));

        let checked = {
            let (data, owner) = (dir.to_path_buf(), owner.clone());
            tokio::task::spawn_blocking(move || {
                let ask = |key: &str| key_accepted(&data, 3369, key, PATIENCE).unwrap();
                (
                    ask(&owner),
                    ask(crate::test_support::TEST_KEY),
                    ask(&"0".repeat(64)),
                )
            })
            .await
            .unwrap()
        };
        assert_eq!(checked, (true, true, false));

        async fn upgraded(
            path: &Path,
            key: &str,
        ) -> tokio_tungstenite::WebSocketStream<tokio::net::UnixStream> {
            let stream = tokio::net::UnixStream::connect(path).await.unwrap();
            let url = format!("ws://localhost:3369/ws/control?key={key}");
            tokio_tungstenite::client_async(url, stream)
                .await
                .expect("a WebSocket upgrade over the socket")
                .0
        }
        let mut owners = upgraded(&path, &owner).await;
        let mut launch = upgraded(&path, crate::test_support::TEST_KEY).await;
        for socket in [&mut owners, &mut launch] {
            let hello = tokio::time::timeout(PATIENCE, socket.next())
                .await
                .expect("the daemon's hello in time")
                .unwrap()
                .unwrap();
            assert!(
                matches!(&hello, Frame::Text(text) if text.contains("hello")),
                "{hello:?}"
            );
        }

        let rotating = state.clone();
        tokio::task::spawn_blocking(move || rotating.owner_key.rotate().unwrap())
            .await
            .unwrap();
        tokio::time::timeout(PATIENCE, async {
            loop {
                match owners.next().await {
                    None | Some(Err(_)) | Some(Ok(Frame::Close(_))) => break,
                    Some(Ok(_)) => {}
                }
            }
        })
        .await
        .expect("the owner key's connection was closed by the rotation");

        launch.send(Frame::Ping(vec![1])).await.unwrap();
        loop {
            let frame = tokio::time::timeout(PATIENCE, launch.next())
                .await
                .expect("an answer in time")
                .expect("the launch key's connection is still open")
                .unwrap();
            match frame {
                Frame::Pong(_) => break,
                Frame::Close(_) => panic!("the launch key's connection was closed"),
                _ => {}
            }
        }
        let data = dir.to_path_buf();
        let old_refused = tokio::task::spawn_blocking(move || {
            key_accepted(&data, 3369, &owner, PATIENCE).unwrap()
        })
        .await
        .unwrap();
        assert!(!old_refused, "the rotated key is refused over the socket");
    }

    /// Dropping the task ends the accepting: nothing connects to the socket afterwards.
    #[cfg(unix)]
    #[tokio::test]
    async fn dropping_the_serve_task_stops_the_accepting() {
        let dir = ScratchDir::new("bsock-drop");
        let path = path_in(&dir);
        let listener = Listener::bind_replacing_stale(&path).unwrap();
        let served = serve(listener, axum::Router::new());
        assert!(local_socket::connect(&path, PATIENCE).is_ok());
        drop(served);
        let refused = crate::test_support::within(PATIENCE, move || {
            local_socket::connect(&path, PATIENCE).is_err()
        })
        .await;
        assert!(
            refused,
            "the socket still accepted after the task was dropped"
        );
    }
}
