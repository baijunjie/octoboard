//! Who may talk to the daemon: a middleware that turns away requests a browser page on another
//! site could make. The `Origin` and `Host` rules and the key compare are
//! [`crate::request_guard`]'s, which the holders' listeners use too; what is the daemon's own is
//! the key.
//!
//! A request needs both: the key is a secret generated for each daemon launch
//! ([`generate_key`](crate::request_guard::generate_key)) that every request carries as its `key`
//! query parameter. The daemon hands it only to what it
//! starts or to the application that started it (see `apps/daemon/PROTOCOL.md`, "Who may
//! connect"), so a page that merely knows the port cannot supply it. The daemon's own arguments
//! and environment do not hold it, but its children's do (the agent CLIs' command lines, the
//! `--key` of the `octoboardd` helpers, `OCTOBOARD_DAEMON_KEY`), and so do the owner-only files of
//! a session's scratch directory. It targets web pages, not local processes: any process of the
//! same user can read it, and could drive the daemon directly anyway.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::request_guard::{headers_allowed, key_matches};

/// Answers 403 — before any handler or WebSocket upgrade runs — to a request whose `Origin` or
/// `Host` [`headers_allowed`] turns away, and 401 to one that does not carry `key`.
pub async fn guard(State(key): State<Arc<str>>, request: Request, next: Next) -> Response {
    if !headers_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response();
    }
    if !key_matches(request.uri().query(), &key) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong key").into_response();
    }
    next.run(request).await
}
