//! Who may talk to the daemon: a middleware that turns away requests a browser page on another
//! site could make. The `Origin` and `Host` rules and the key compare are
//! [`crate::request_guard`]'s, which the holders' listeners use too; what is the daemon's own is
//! the key.
//!
//! A request needs both: the key is a secret that every request carries as its `key` query
//! parameter, except a read of a file of the embedded browser UI, which holds nothing secret
//! and is fetched by a page's own asset requests (see [`crate::server::ui`]). It is the launch key,
//! generated for each daemon launch ([`generate_key`](crate::request_guard::generate_key)), which
//! the daemon hands only to what it starts or to the application that started it (see
//! `apps/daemon/PROTOCOL.md`, "Who may connect"), or, on the browser socket alone
//! ([`crate::browser_socket`]), the owner key ([`crate::owner_key`]) that a daemon serving the
//! browser keeps for the owner's browser across launches; a page that merely knows the port cannot
//! supply either. The TCP port never takes the owner key: another user of the host can bind that
//! port while the daemon has let it go, and a key the browser sends there would be theirs. The
//! owner key is in no argument, environment or file the launch key is in. No process Octoboard
//! starts holds the launch key in its arguments or environment, which every user of the machine can
//! list or an agent writes into files of its own. A session's helpers read it from the owner-only key file in the session's scratch
//! directory (`--key-file`, or `OCTOBOARD_DAEMON_CREDENTIAL_PATH` for the git hook helpers of a
//! project session, which fall back on `daemon.key` once that file is gone); other owner-only
//! files, such as `claude-settings.json`, `holders/*.json` and `daemon.key`, hold keys too. It
//! targets web pages, not local processes: any process of the same user can read the launch key,
//! and could drive the daemon directly anyway.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::owner_key::{OwnerKey, Revocation};
use crate::request_guard::{headers_allowed, key_matches};
use crate::server::ui::{self, Files};

/// What the guard of one listener checks a request against: the daemon's launch key, the owner key
/// where that listener takes it (the browser socket), and the embedded UI files it serves, which
/// need no key (see [`crate::server::ui`]).
#[derive(Clone)]
pub struct Access {
    pub key: Arc<str>,
    pub owner_key: Option<Arc<OwnerKey>>,
    pub ui: Files,
    /// The body of a `401`: on the TCP port of a daemon that serves the browser on its socket, it
    /// says where a browser goes instead, since a page opened through a forward to the port is
    /// refused there.
    pub refusal: &'static str,
}

/// The body of a `401`.
pub const REFUSED: &str = "missing or wrong key";

/// The body of a `401` on the TCP port of a daemon that serves the browser on its socket instead.
pub const REFUSED_AT_PORT: &str =
    "missing or wrong key. A browser reaches this daemon through its \
     own socket, not this port: run `octoboardd address` on the server for the SSH forward to use.";

/// Which key let a request in, for a handler that keeps a connection open: one the owner key
/// admitted closes once that key is rotated away ([`Admission::revoked`]); the launch key lasts as
/// long as the daemon. Every request past the guard carries one, as a request extension.
#[derive(Clone, Default)]
pub struct Admission(Option<Revocation>);

impl Admission {
    /// Resolves when the owner key that admitted the request has been rotated since, and never
    /// for a request the launch key admitted.
    pub async fn revoked(&mut self) {
        match &mut self.0 {
            Some(revocation) => revocation.revoked().await,
            None => std::future::pending().await,
        }
    }
}

/// Answers 403 — before any handler or WebSocket upgrade runs — to a request whose `Origin` or
/// `Host` [`headers_allowed`] turns away, and 401 to one that carries neither the launch key nor,
/// where this listener takes it, the owner key, unless it asks for an embedded UI file this
/// listener serves. The owner key is read on every request, so a rotation takes effect on the next
/// one, and a connection it admitted is told of the rotation through its [`Admission`].
pub async fn guard(State(access): State<Access>, mut request: Request, next: Next) -> Response {
    if !headers_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response();
    }
    if ui::file(access.ui, request.method(), request.uri().path()).is_some() {
        return next.run(request).await;
    }
    let query = request.uri().query();
    let admission = if key_matches(query, &access.key) {
        Admission::default()
    } else {
        match access
            .owner_key
            .as_ref()
            .and_then(|owner| owner.admit(query))
        {
            Some(revocation) => Admission(Some(revocation)),
            None => return (StatusCode::UNAUTHORIZED, access.refusal).into_response(),
        }
    };
    request.extensions_mut().insert(admission);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ScratchDir, PATIENCE};

    fn held_key(label: &str) -> (OwnerKey, String, ScratchDir) {
        let dir = ScratchDir::new(label);
        let held = OwnerKey::default();
        held.enable(&crate::owner_key::path_in(&dir)).unwrap();
        let key = crate::owner_key::read_in(&dir).expect("the owner key");
        (held, key, dir)
    }

    /// What the owner key admitted is told of its rotation; what the launch key admitted never is.
    /// The clock is paused, so the wait that never ends runs out at once.
    #[tokio::test(start_paused = true)]
    async fn only_what_the_owner_key_admitted_is_revoked_by_a_rotation() {
        let (held, key, _dir) = held_key("access-admission");
        let query = format!("key={key}");
        let mut owners = Admission(held.admit(Some(&query)));
        let mut launch = Admission::default();

        held.rotate().unwrap();
        tokio::time::timeout(PATIENCE, owners.revoked())
            .await
            .expect("the owner key's admission is revoked");
        assert!(
            tokio::time::timeout(PATIENCE, launch.revoked())
                .await
                .is_err(),
            "the launch key's admission was revoked"
        );
    }
}
