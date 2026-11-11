//! Who may talk to the daemon: a middleware that turns away requests a browser page on another
//! site could make. The `Origin` and `Host` rules and the key compare are
//! [`crate::request_guard`]'s, which the holders' listeners use too; what is the daemon's own is
//! the key.
//!
//! A request needs both: the key is a secret generated for each daemon launch
//! ([`generate_key`](crate::request_guard::generate_key)) that every request carries as its `key`
//! query parameter, except a read of a file of the embedded browser UI, which holds nothing secret
//! and is fetched by a page's own asset requests (see [`crate::server::ui`]). The daemon hands it
//! only to what it starts or to the application that started it (see `apps/daemon/PROTOCOL.md`,
//! "Who may connect"), so a page that merely knows the port cannot supply it. No process Octoboard
//! starts holds it in its arguments or environment, which every user of the machine can list or an
//! agent writes into files of its own. A session's helpers read it from the owner-only key file in
//! the session's scratch directory (`--key-file`, or `OCTOBOARD_DAEMON_CREDENTIAL_PATH` for the git
//! hook helpers of a project session, which fall back on `daemon.key` once that file is gone);
//! other owner-only files, such as `claude-settings.json`, `holders/*.json` and `daemon.key`, hold
//! keys too. It targets web pages, not local processes: any process of the same user can read it,
//! and could drive the daemon directly anyway.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::request_guard::{headers_allowed, key_matches};
use crate::server::ui::{self, Files};

/// What the guard checks a request against: the daemon's key, and the embedded UI files, which
/// need no key (see [`crate::server::ui`]).
#[derive(Clone)]
pub struct Access {
    pub key: Arc<str>,
    pub ui: Files,
}

/// Answers 403 — before any handler or WebSocket upgrade runs — to a request whose `Origin` or
/// `Host` [`headers_allowed`] turns away, and 401 to one that does not carry `key`, unless it asks
/// for an embedded UI file.
pub async fn guard(State(access): State<Access>, request: Request, next: Next) -> Response {
    if !headers_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response();
    }
    if ui::file(access.ui, request.method(), request.uri().path()).is_some() {
        return next.run(request).await;
    }
    if !key_matches(request.uri().query(), &access.key) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong key").into_response();
    }
    next.run(request).await
}
