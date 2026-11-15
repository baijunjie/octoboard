//! The routes the helpers of a launch call: the same paths and bodies as the daemon's own helper
//! routes (`crate::server`), on the holder's port, with the launch's key. A helper reaches the
//! daemon through the holder, which is there when the daemon is not.
//!
//! The holder checks the key before anything is relayed: it has to be one the holder issued, and
//! belong to the session the path names (for the MCP route, whose path holds the session's token
//! and not its id, any launch's key does, unless the launch is bound to a token of its own, and the
//! daemon resolves the token). The check is a middleware ([`keyed`]) that runs before any body is
//! taken. What a request asks is the daemon's to judge, so the holder reads no body, beyond taking
//! each request its key admitted whole and reading a refused one out, without holding it, before it
//! answers (`crate::request_body`).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, RawQuery, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use bytes::Bytes;

use super::launches::Launches;
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
        // Inside the key check, so that only a request it lets in has its body held; on the routes
        // alone, so that a path none of them takes is not held either.
        .route_layer(axum::middleware::from_fn(crate::request_body::buffer))
        // On the routes alone, which name the session or token the key is checked against; a path
        // none of them takes falls through to `404`. A method a route's path does not take is
        // checked too, as on the daemon, and is answered `405` only past the key.
        .route_layer(axum::middleware::from_fn_with_state(helpers.clone(), keyed))
        .with_state(helpers)
        // Around the finished router, so a route added later cannot end up outside it.
        .layer(axum::middleware::from_fn(origin_and_host))
        // Outside it, so that the 403 and 401 too are answered only once the request's body is
        // read, and without holding it.
        .layer(axum::middleware::from_fn(crate::request_body::drain_unread))
}

/// Turns away what a browser page on another site could send, whatever the route and the key.
async fn origin_and_host(request: Request, next: Next) -> Response {
    if !headers_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response();
    }
    next.run(request).await
}

/// Answers `401` to a request whose key is missing, is none the holder issued, or is another
/// session's: on a route about a session, the key has to be that session's launch's; on the MCP
/// route, a launch bound to an MCP token is good for that token alone, so that one launch cannot
/// use another session's. It runs before [`buffer`](crate::request_body::buffer), so a refused
/// request's body is never held: [`drain_unread`](crate::request_body::drain_unread), outside,
/// reads it out and throws it away.
async fn keyed(
    State(helpers): State<Arc<Helpers>>,
    Path(params): Path<HashMap<String, String>>,
    RawQuery(query): RawQuery,
    request: Request,
    next: Next,
) -> Response {
    let launch = supplied_key(query.as_deref()).and_then(|key| helpers.launches.find(key));
    let admitted = launch.is_some_and(|launch| {
        match (params.get("session"), params.get("token")) {
            (Some(session), _) => launch.session == *session,
            (None, Some(token)) => launch
                .mcp_token
                .as_deref()
                .is_none_or(|bound| same_secret(token, bound)),
            // Every route names one or the other.
            (None, None) => false,
        }
    });
    if !admitted {
        return (StatusCode::UNAUTHORIZED, "missing or wrong key").into_response();
    }
    next.run(request).await
}

/// What a report is answered with, which is what the daemon's route answers a well-formed one
/// with. The helpers that report ignore the answer, and a request the daemon cannot read is not
/// the holder's to refuse.
fn queued(helpers: &Helpers, route: RelayRoute, session: String, body: Bytes) -> Response {
    helpers.relays.report(route, session, body.to_vec());
    StatusCode::OK.into_response()
}

async fn report_hook(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    body: Bytes,
) -> Response {
    queued(&helpers, RelayRoute::Hook, session, body)
}

async fn report_git_ref(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    body: Bytes,
) -> Response {
    queued(&helpers, RelayRoute::GitRef, session, body)
}

async fn report_context_window(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    body: Bytes,
) -> Response {
    queued(&helpers, RelayRoute::ContextWindow, session, body)
}

async fn ask_revert_gate(
    State(helpers): State<Arc<Helpers>>,
    Path(session): Path<String>,
    body: Bytes,
) -> Response {
    asked(&helpers, RelayRoute::RevertGate, session, body).await
}

async fn ask_mcp(
    State(helpers): State<Arc<Helpers>>,
    Path(token): Path<String>,
    body: Bytes,
) -> Response {
    asked(&helpers, RelayRoute::Mcp, token, body).await
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

    /// Helper routes over a holder's key and relay state with no daemon linked, for the tests that
    /// drive the router in memory: what is queued stays queued.
    fn unserved_helpers() -> Arc<Helpers> {
        Arc::new(Helpers {
            launches: Launches::default(),
            relays: Relays::new(PATIENCE),
        })
    }

    /// The status a request is answered with while its body is still held back: its head states a
    /// body of exactly the limit, so only reading the body can find it over; no answer comes
    /// before a byte is sent, and then one more than the limit is sent.
    async fn status_with_an_over_limit_body(
        helpers: &Arc<Helpers>,
        request_line: &str,
        headers: &[&str],
    ) -> StatusCode {
        use crate::request_body::REQUEST_BODY_LIMIT;
        use crate::test_support::body_first::Pending;

        let mut pending = Pending::start(
            router(helpers.clone()),
            request_line,
            headers,
            REQUEST_BODY_LIMIT,
        );
        pending.assert_unanswered(request_line).await;
        pending.send(&vec![b'x'; REQUEST_BODY_LIMIT + 1]);
        pending.answered_while_held().await
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
        // The key of the session itself gets through. Its report, of a body of its own, is the
        // first frame the daemon is handed: had a refused request been relayed, it would be queued
        // ahead of it and come first, whenever the relay task got to it.
        assert!(post(port, &key, "/hook/s1", b"first")
            .await
            .unwrap()
            .is_empty());
        let FromHolder::Relay { body, waits, .. } = next(&mut frames).await else {
            panic!("not a relay");
        };
        assert_eq!((body.as_slice(), waits), (&b"first"[..], false));
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

    /// The helper routes' refusals and fallback wait for the request's body, as the daemon's do,
    /// and a report is queued only once it is in. The router is driven in memory with a body the
    /// test holds back; no daemon is linked, so what is queued stays queued.
    #[tokio::test]
    async fn no_answer_goes_out_before_the_request_body_is_read() {
        let helpers = unserved_helpers();
        let key = helpers.launches.issue("t1", "s1", None).unwrap();
        let host = "Host: 127.0.0.1:4000";
        for (request_line, headers, status) in [
            (format!("POST /hook/s1?key={key}0"), vec![host], 401),
            (
                format!("POST /hook/s1?key={key}"),
                vec![host, "Origin: https://evil.example"],
                403,
            ),
            (format!("POST /nowhere?key={key}"), vec![host], 404),
            (format!("POST /hook/s1?key={key}"), vec![host], 200),
        ] {
            let answered = crate::test_support::body_first::status_once_the_body_is_in(
                router(helpers.clone()),
                &request_line,
                &headers,
                || assert_eq!(helpers.relays.queued(), 0, "{request_line}"),
            )
            .await;
            assert_eq!(answered, status, "{request_line}");
        }
        // Only the report that was let in was queued.
        assert_eq!(helpers.relays.queued(), 1);
    }

    /// A request the holder does not let in never has its body held: it is refused once the body
    /// has been read out, however much of one it sends, where a body the holder took would be
    /// answered `413` as soon as it passed the limit. That covers a key that is missing, wrong,
    /// another session's, or bound to another MCP token (`401`), a path no route takes (`404`), a
    /// method no route takes (`401` unkeyed, as the key check wraps the route's `405` too), and a
    /// foreign Origin (`403`, whatever the key). Only the key the route admits gets the `413`, so
    /// the body it sends is the one held. Nothing is relayed.
    #[tokio::test]
    async fn a_request_the_holder_refuses_has_its_body_read_out_not_held() {
        let helpers = unserved_helpers();
        let key = helpers
            .launches
            .issue("t1", "s1", Some("its-token"))
            .unwrap();
        let other = helpers.launches.issue("t2", "s2", None).unwrap();
        let host = "Host: 127.0.0.1:4000";
        let plain = [host];
        let foreign = [host, "Origin: https://evil.example"];

        let mut cases: Vec<(String, &[&str], StatusCode)> = Vec::new();
        for path in [
            "/hook/s1",
            "/git-ref/s1",
            "/context-window/s1",
            "/revert-gate/s1",
        ] {
            for refused in [
                String::new(),
                format!("?key={key}0"),
                format!("?key={other}"),
            ] {
                cases.push((
                    format!("POST {path}{refused}"),
                    &plain,
                    StatusCode::UNAUTHORIZED,
                ));
            }
            cases.push((
                format!("POST {path}?key={key}"),
                &plain,
                StatusCode::PAYLOAD_TOO_LARGE,
            ));
        }
        for refused in [
            "/mcp/its-token".to_string(),
            format!("/mcp/its-token?key={key}0"),
            format!("/mcp/another-token?key={key}"),
        ] {
            cases.push((format!("POST {refused}"), &plain, StatusCode::UNAUTHORIZED));
        }
        cases.push((
            format!("POST /mcp/its-token?key={key}"),
            &plain,
            StatusCode::PAYLOAD_TOO_LARGE,
        ));
        // A launch bound to no token is good for any token.
        cases.push((
            format!("POST /mcp/another-token?key={other}"),
            &plain,
            StatusCode::PAYLOAD_TOO_LARGE,
        ));
        cases.push((
            format!("POST /nowhere?key={key}"),
            &plain,
            StatusCode::NOT_FOUND,
        ));
        cases.push(("GET /hook/s1".to_string(), &plain, StatusCode::UNAUTHORIZED));
        cases.push((
            format!("GET /hook/s1?key={key}"),
            &plain,
            StatusCode::PAYLOAD_TOO_LARGE,
        ));
        // The Origin check comes before the key: a refusal of its own whatever the key.
        for request_line in [
            format!("POST /hook/s1?key={key}"),
            format!("POST /hook/s1?key={key}0"),
            "POST /hook/s1".to_string(),
            format!("POST /mcp/token?key={key}"),
            format!("POST /nowhere?key={key}"),
        ] {
            cases.push((request_line, &foreign, StatusCode::FORBIDDEN));
        }

        for (request_line, headers, status) in cases {
            assert_eq!(
                status_with_an_over_limit_body(&helpers, &request_line, headers).await,
                status,
                "{request_line}"
            );
        }
        assert_eq!(helpers.relays.queued(), 0, "nothing was relayed");
    }

    /// A method no route takes is answered `405` once its small body is in, when its key is good.
    #[tokio::test]
    async fn a_wrong_method_with_a_good_key_is_answered_405_after_its_body() {
        let helpers = unserved_helpers();
        let key = helpers.launches.issue("t1", "s1", None).unwrap();
        let answered = crate::test_support::body_first::status_once_the_body_is_in(
            router(helpers.clone()),
            &format!("GET /hook/s1?key={key}"),
            &["Host: 127.0.0.1:4000"],
            || {},
        )
        .await;
        assert_eq!(answered, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(helpers.relays.queued(), 0, "nothing was relayed");
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
        // The helper hangs up once its call has been relayed to the daemon.
        let (relayed, hang_up) = std::sync::mpsc::channel::<()>();
        let call = tokio::task::spawn_blocking(move || {
            let stream = crate::loopback::connect(port, PATIENCE).expect("connected");
            let exchanging = stream.try_clone().expect("a second handle");
            let waiting = std::thread::spawn(move || {
                crate::loopback::exchange(&exchanging, port, &key, "/mcp/token", b"{}")
            });
            hang_up.recv().ok();
            stream.shutdown(std::net::Shutdown::Both).ok();
            waiting.join().ok();
        });
        let FromHolder::Relay { req, waits, .. } = next(&mut frames).await else {
            panic!("not a relay");
        };
        assert!(waits);
        relayed.send(()).unwrap();
        let FromHolder::RelayGone { req: gone } = next(&mut frames).await else {
            panic!("not a relay_gone");
        };
        assert_eq!(gone, req);
        call.await.expect("the helper ran");
    }
}
