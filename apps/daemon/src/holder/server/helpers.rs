//! The routes the helpers of a launch call: the same paths and bodies as the daemon's own helper
//! routes (`crate::server`), on the holder's port, with the launch's key. A helper reaches the
//! daemon through the holder, which is there when the daemon is not.
//!
//! The holder checks the key before anything is relayed: it has to be one the holder issued, and
//! belong to the session the path names (for the MCP route, whose path holds the session's token
//! and not its id, any launch's key does, and the daemon resolves the token). What a request
//! asks is the daemon's to judge, so the holder reads no body.

use std::sync::Arc;

use axum::extract::{Path, RawQuery, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use bytes::Bytes;

use super::launches::{Launch, Launches};
use super::relay::{Failure, Relays};
use crate::holder::protocol::RelayRoute;
use crate::request_guard::{headers_allowed, same_secret, supplied_key};

/// What the helper routes work on.
pub(super) struct Helpers {
    pub(super) launches: Launches,
    pub(super) relays: Arc<Relays>,
}

pub(super) fn router(helpers: Arc<Helpers>) -> Router {
    Router::new()
        .route("/hook/:session", post(report_hook))
        .route("/git-ref/:session", post(report_git_ref))
        .route("/context-window/:session", post(report_context_window))
        .route("/revert-gate/:session", post(ask_revert_gate))
        .route("/mcp/:token", post(ask_mcp))
        .with_state(helpers)
        // Around the finished router, so a route added later cannot end up outside it.
        .layer(axum::middleware::from_fn(origin_and_host))
}

/// Turns away what a browser page on another site could send, whatever the route and the key.
async fn origin_and_host(request: Request, next: Next) -> Response {
    if !headers_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response();
    }
    next.run(request).await
}

/// A request refused for its key: missing, wrong, or another session's.
struct Refused;

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, "missing or wrong key").into_response()
    }
}

/// The launch the request's key belongs to, and for a request about `session`, only if the key is
/// that session's.
fn authenticate(
    helpers: &Helpers,
    query: Option<&str>,
    session: Option<&str>,
) -> Result<Launch, Refused> {
    let launch = supplied_key(query)
        .and_then(|key| helpers.launches.find(key))
        .ok_or(Refused)?;
    match session {
        Some(session) if launch.session != session => Err(Refused),
        _ => Ok(launch),
    }
}

/// What a report is answered with, which is what the daemon's route answers a well-formed one
/// with. The helpers that report ignore the answer, and a request the daemon cannot read is not
/// the holder's to refuse.
fn queued(
    helpers: &Helpers,
    route: RelayRoute,
    session: String,
    query: Option<&str>,
    body: Bytes,
) -> Response {
    match authenticate(helpers, query, Some(&session)) {
        Ok(_) => {
            helpers.relays.report(route, session, body.to_vec());
            StatusCode::OK.into_response()
        }
        Err(refused) => refused.into_response(),
    }
}

async fn report_hook(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    queued(&helpers, RelayRoute::Hook, session, query.as_deref(), body)
}

async fn report_git_ref(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    queued(
        &helpers,
        RelayRoute::GitRef,
        session,
        query.as_deref(),
        body,
    )
}

async fn report_context_window(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    queued(
        &helpers,
        RelayRoute::ContextWindow,
        session,
        query.as_deref(),
        body,
    )
}

async fn ask_revert_gate(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    match authenticate(&helpers, query.as_deref(), Some(&session)) {
        Ok(_) => asked(&helpers, RelayRoute::RevertGate, session, body).await,
        Err(refused) => refused.into_response(),
    }
}

async fn ask_mcp(
    State(helpers): State<Arc<Helpers>>,
    Path(token): Path<String>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    match authenticate(&helpers, query.as_deref(), None) {
        // A launch's key is good for its own MCP token alone, so that one launch cannot use
        // another session's.
        Ok(launch)
            if launch
                .mcp_token
                .as_deref()
                .is_none_or(|bound| same_secret(&token, bound)) =>
        {
            asked(&helpers, RelayRoute::Mcp, token, body).await
        }
        Ok(_) => (StatusCode::UNAUTHORIZED, "missing or wrong key").into_response(),
        Err(refused) => refused.into_response(),
    }
}

/// Puts a question to the daemon and answers with its answer, or with the status a helper reads
/// as "no daemon" (`crate::loopback::NO_DAEMON`) or "the daemon was lost with the call in flight"
/// (`crate::loopback::DAEMON_LOST`). The request is dropped, and with it the question, when the
/// helper hangs up.
async fn asked(helpers: &Helpers, route: RelayRoute, param: String, body: Bytes) -> Response {
    match helpers.relays.ask(route, param, body.to_vec()).await {
        Ok(answer) => {
            let status = StatusCode::from_u16(answer.status).unwrap_or(StatusCode::BAD_GATEWAY);
            let has_body = !answer.body.is_empty();
            let mut response = (status, answer.body).into_response();
            if has_body {
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
            }
            response
        }
        Err(Failure::NoDaemon) => (
            StatusCode::from_u16(crate::loopback::NO_DAEMON).expect("a status"),
            "no daemon is running",
        )
            .into_response(),
        Err(Failure::Lost) => (
            StatusCode::from_u16(crate::loopback::DAEMON_LOST).expect("a status"),
            "the daemon was lost with the call in flight",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    use axum::extract::ws::Message;
    use tokio::sync::mpsc;

    use super::*;
    use crate::holder::protocol::FromHolder;
    use crate::test_support::PATIENCE;

    struct Served {
        port: u16,
        helpers: Arc<Helpers>,
    }

    /// The helper routes on a loopback port, over a holder's key and relay state that no holder
    /// owns, so a test controls whether a daemon is linked.
    async fn serve(relay_deadline: Duration) -> Served {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let helpers = Arc::new(Helpers {
            launches: Launches::default(),
            relays: Relays::new(relay_deadline),
        });
        let app = router(helpers.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        Served { port, helpers }
    }

    fn link(helpers: &Helpers, generation: u64) -> mpsc::Receiver<Message> {
        let (sink, frames) = mpsc::channel(64);
        helpers.relays.attach(generation, sink);
        frames
    }

    async fn next(frames: &mut mpsc::Receiver<Message>) -> FromHolder {
        let Message::Text(text) = tokio::time::timeout(PATIENCE, frames.recv())
            .await
            .expect("a frame in time")
            .expect("a frame")
        else {
            panic!("not a text frame");
        };
        serde_json::from_str(&text).expect("a message")
    }

    /// What a helper does: a blocking POST with the key, answered with the body or the error
    /// status.
    async fn post(
        port: u16,
        key: &str,
        path: &'static str,
        body: &'static [u8],
    ) -> std::io::Result<Vec<u8>> {
        let key = key.to_string();
        tokio::task::spawn_blocking(move || crate::loopback::post(port, &key, path, body, PATIENCE))
            .await
            .expect("the helper ran")
    }

    /// The status of a request sent with exactly the headers given. **Blocks**, so it is called
    /// off the runtime, whose one thread serves the request.
    fn status_of(port: u16, request_line: &str, headers: &[&str]) -> u16 {
        let mut request = format!("{request_line} HTTP/1.1\r\n");
        for header in headers {
            request.push_str(header);
            request.push_str("\r\n");
        }
        request.push_str("Content-Length: 2\r\n\r\n{}");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(request.as_bytes()).expect("write");
        let mut status_line = String::new();
        BufReader::new(stream)
            .read_line(&mut status_line)
            .expect("read");
        status_line
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse().ok())
            .expect("a status line")
    }

    #[tokio::test]
    async fn a_key_that_is_missing_wrong_or_another_sessions_is_refused() {
        let served = serve(PATIENCE).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        let other = served.helpers.launches.issue("t2", "s2", None).unwrap();
        let mut frames = link(&served.helpers, 1);
        let port = served.port;

        let wrong = "0".repeat(key.len());
        for path in [
            "/hook/s1",
            "/git-ref/s1",
            "/context-window/s1",
            "/revert-gate/s1",
        ] {
            for refused in [&wrong[..], "", &other[..]] {
                let key = refused.to_string();
                let err = tokio::task::spawn_blocking(move || {
                    crate::loopback::post(port, &key, path, b"{}", PATIENCE)
                })
                .await
                .unwrap()
                .expect_err("refused");
                assert_eq!(crate::loopback::error_status(&err), Some(401), "{path}");
            }
        }
        let err = tokio::task::spawn_blocking(move || {
            crate::loopback::post(port, &wrong, "/mcp/token", b"{}", PATIENCE)
        })
        .await
        .unwrap()
        .expect_err("refused");
        assert_eq!(crate::loopback::error_status(&err), Some(401));
        // Off the runtime's one thread, which serves the request.
        let no_key = tokio::task::spawn_blocking(move || {
            status_of(port, "POST /hook/s1", &[&format!("Host: 127.0.0.1:{port}")])
        })
        .await
        .unwrap();
        assert_eq!(no_key, 401, "no key at all");
        // Nothing of any of it was relayed.
        assert!(
            tokio::time::timeout(Duration::from_millis(200), frames.recv())
                .await
                .is_err()
        );

        // The key of the session itself gets through.
        assert!(post(port, &key, "/hook/s1", b"{}")
            .await
            .unwrap()
            .is_empty());
        assert!(matches!(
            next(&mut frames).await,
            FromHolder::Relay { waits: false, .. }
        ));
    }

    #[tokio::test]
    async fn a_key_bound_to_an_mcp_token_is_good_for_that_token_alone() {
        let served = serve(PATIENCE).await;
        let key = served
            .helpers
            .launches
            .issue("t1", "s1", Some("its-token"))
            .unwrap();
        let mut frames = link(&served.helpers, 1);
        let port = served.port;

        let err = post(port, &key, "/mcp/another-token", b"{}")
            .await
            .expect_err("refused");
        assert_eq!(crate::loopback::error_status(&err), Some(401));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), frames.recv())
                .await
                .is_err(),
            "nothing of it was relayed"
        );

        let relays = served.helpers.relays.clone();
        let answering = tokio::spawn(async move {
            let FromHolder::Relay { req, param, .. } = next(&mut frames).await else {
                panic!("not a relay");
            };
            relays.answer(
                req,
                super::super::relay::Answer {
                    status: 200,
                    body: "null".to_string(),
                },
            );
            param
        });
        assert_eq!(
            post(port, &key, "/mcp/its-token", b"{}").await.unwrap(),
            b"null"
        );
        assert_eq!(answering.await.unwrap(), "its-token");
    }

    #[tokio::test]
    async fn a_revoked_key_is_refused() {
        let served = serve(PATIENCE).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        assert!(post(served.port, &key, "/hook/s1", b"{}").await.is_ok());
        served.helpers.launches.revoke("t1");
        let err = post(served.port, &key, "/hook/s1", b"{}")
            .await
            .expect_err("refused");
        assert_eq!(crate::loopback::error_status(&err), Some(401));
    }

    #[tokio::test]
    async fn a_request_from_a_page_or_a_foreign_host_is_refused_whatever_its_key() {
        // No daemon is linked, so a question that gets past the guard is answered "no daemon"
        // once this deadline passes.
        let served = serve(Duration::from_millis(100)).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        let port = served.port;
        let host = format!("Host: 127.0.0.1:{port}");
        tokio::task::spawn_blocking(move || {
            for route in [
                "POST /hook/s1",
                "POST /git-ref/s1",
                "POST /context-window/s1",
                "POST /revert-gate/s1",
                "POST /mcp/token",
            ] {
                let keyed = format!("{route}?key={key}");
                assert_eq!(
                    status_of(port, &keyed, &[&host, "Origin: https://evil.example"]),
                    403,
                    "{route}"
                );
                assert_eq!(status_of(port, &keyed, &[&host, "Origin: null"]), 403);
                assert_eq!(
                    status_of(
                        port,
                        &keyed,
                        &["Host: evil.example", "Origin: http://evil.example"]
                    ),
                    403
                );
                assert_eq!(status_of(port, &keyed, &[]), 403, "no Host");
                // Claude Code's own HTTP hook states no Origin and gets through to the key.
                assert_ne!(status_of(port, &keyed, &[&host]), 403, "{route}");
                assert_ne!(
                    status_of(port, &keyed, &[&host, "Origin: tauri://localhost"]),
                    403
                );
            }
        })
        .await
        .expect("the checks ran");
    }

    #[tokio::test]
    async fn reports_are_answered_at_once_with_no_daemon_and_reach_the_next_one_in_order() {
        let served = serve(PATIENCE).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        for path in ["/hook/s1", "/git-ref/s1", "/context-window/s1"] {
            let answered = post(served.port, &key, path, b"{}")
                .await
                .expect("answered");
            assert!(answered.is_empty(), "{path}");
        }
        assert_eq!(served.helpers.relays.queued(), 3);

        let mut frames = link(&served.helpers, 1);
        for expected in [
            RelayRoute::Hook,
            RelayRoute::GitRef,
            RelayRoute::ContextWindow,
        ] {
            let FromHolder::Relay {
                route,
                param,
                waits,
                ..
            } = next(&mut frames).await
            else {
                panic!("not a relay");
            };
            assert_eq!((route, param.as_str(), waits), (expected, "s1", false));
        }
    }

    #[tokio::test]
    async fn a_question_is_relayed_with_its_session_or_token_and_gets_the_daemons_answer() {
        let served = serve(PATIENCE).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        let mut frames = link(&served.helpers, 1);
        let relays = served.helpers.relays.clone();
        let answering = tokio::spawn(async move {
            for answer in ["null", r#"{"ok":true}"#] {
                let FromHolder::Relay { req, .. } = next(&mut frames).await else {
                    panic!("not a relay");
                };
                relays.answer(
                    req,
                    super::super::relay::Answer {
                        status: 200,
                        body: answer.to_string(),
                    },
                );
            }
            frames
        });
        assert_eq!(
            post(served.port, &key, "/revert-gate/s1", b"{}")
                .await
                .unwrap(),
            b"null"
        );
        // The token is passed on as it is: it is the daemon that knows whose it is.
        assert_eq!(
            post(served.port, &key, "/mcp/the-token", b"{}")
                .await
                .unwrap(),
            br#"{"ok":true}"#
        );
        answering.await.unwrap();
    }

    #[tokio::test]
    async fn a_question_with_no_daemon_fails_as_no_daemon_after_the_deadline() {
        let served = serve(Duration::from_millis(200)).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        let started = std::time::Instant::now();
        for path in ["/revert-gate/s1", "/mcp/token"] {
            let err = post(served.port, &key, path, b"{}")
                .await
                .expect_err("no daemon");
            assert!(crate::loopback::is_no_daemon(&err), "{path}: {err}");
        }
        assert!(started.elapsed() >= Duration::from_millis(400));
    }

    #[tokio::test]
    async fn a_link_lost_with_a_question_in_flight_fails_it_at_once() {
        let served = serve(Duration::from_secs(60)).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        let mut frames = link(&served.helpers, 1);
        let relays = served.helpers.relays.clone();
        tokio::spawn(async move {
            next(&mut frames).await;
            relays.detach(1);
        });
        let err = post(served.port, &key, "/mcp/token", b"{}")
            .await
            .expect_err("lost");
        assert!(crate::loopback::is_daemon_lost(&err), "{err}");
    }

    #[tokio::test]
    async fn a_helper_that_hangs_up_tells_the_daemon_its_call_is_gone() {
        let served = serve(PATIENCE).await;
        let key = served.helpers.launches.issue("t1", "s1", None).unwrap();
        let mut frames = link(&served.helpers, 1);
        let port = served.port;
        let call = tokio::task::spawn_blocking(move || {
            let stream = crate::loopback::connect(port, PATIENCE).expect("connected");
            let exchanging = stream.try_clone().expect("a second handle");
            let waiting = std::thread::spawn(move || {
                crate::loopback::exchange(&exchanging, port, &key, "/mcp/token", b"{}")
            });
            std::thread::sleep(Duration::from_millis(300));
            stream.shutdown(std::net::Shutdown::Both).ok();
            waiting.join().ok();
        });
        let FromHolder::Relay { req, waits, .. } = next(&mut frames).await else {
            panic!("not a relay");
        };
        assert!(waits);
        let FromHolder::RelayGone { req: gone } = next(&mut frames).await else {
            panic!("not a relay_gone");
        };
        assert_eq!(gone, req);
        call.await.expect("the helper ran");
    }
}
