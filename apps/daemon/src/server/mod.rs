//! The HTTP and WebSocket surface described in `apps/daemon/PROTOCOL.md`, bound to localhost only.
//!
//! This file holds the route table and the guard around it; each route's handler lives beside the
//! protocol it speaks:
//!
//! - [`control`]: the control socket, `/ws/control`.
//! - [`terminal`]: the terminal socket, `/ws/term/:session`.
//! - [`callbacks`]: the HTTP routes the agents' hooks and the daemon's own helpers post to.

mod callbacks;
mod control;
mod terminal;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    // The guard wraps the finished router, fallback included, so a route added to `routes` later
    // cannot end up outside it. A refused request never reaches a handler or a WebSocket upgrade.
    let key: Arc<str> = state.key.as_str().into();
    routes(state).layer(axum::middleware::from_fn_with_state(
        key,
        crate::access::guard,
    ))
}

fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws/control", get(control::control_ws))
        .route("/ws/term/:session", get(terminal::term_ws))
        .route("/hook/:session", post(callbacks::hook_callback))
        .route("/git-ref/:session", post(callbacks::git_ref_callback))
        .route(
            "/revert-gate/:session",
            post(callbacks::revert_gate_callback),
        )
        .route(
            "/context-window/:session",
            post(callbacks::context_window_callback),
        )
        .route("/mcp/:token", post(callbacks::mcp_call))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::test_support::{keyed, serve, status_of};
    use super::*;
    use crate::test_support::{app_state, PATIENCE, TEST_KEY};

    /// A refused request must not reach a handler, and an accepted one must: `/mcp/:token`
    /// answers 200 to a token it does not know, which no other layer would, while the upgrade
    /// routes answer 400 to a handshake that is not one — still not the 403 or 401 of the guard.
    #[tokio::test]
    async fn every_route_is_behind_the_origin_and_key_guard() {
        let port = serve("guard").await;
        let host = format!("Host: 127.0.0.1:{port}");
        let tests = tokio::task::spawn_blocking(move || {
            let routes = [
                "GET /ws/control",
                "GET /ws/term/s",
                "POST /hook/s",
                "POST /git-ref/s",
                "POST /revert-gate/s",
                "POST /context-window/s",
                "POST /mcp/x",
            ];
            for route in routes {
                let keyed = keyed(route);
                let forbidden = |headers: &[&str]| status_of(port, &keyed, headers) == 403;
                assert!(
                    forbidden(&[&host, "Origin: https://evil.example"]),
                    "{route}"
                );
                assert!(forbidden(&[&host, "Origin: null"]), "{route}");
                assert!(
                    forbidden(&["Host: evil.example", "Origin: http://evil.example"]),
                    "{route}"
                );
                assert!(forbidden(&["Host: evil.example"]), "{route}");
                assert!(forbidden(&[]), "{route}");
                assert!(!forbidden(&[&host]), "{route}");
                assert!(
                    !forbidden(&[&host, "Origin: http://localhost:5174"]),
                    "{route}"
                );
                assert!(!forbidden(&[&host, "Origin: tauri://localhost"]), "{route}");

                // Without the key a request is refused whatever else it states, and an origin
                // that is refused anyway is still told 403 rather than which key to try.
                let unauthorized = |request_line: &str, headers: &[&str]| {
                    status_of(port, request_line, headers) == 401
                };
                let wrong = format!("{route}?key={}", "0".repeat(64));
                let near = &TEST_KEY[..TEST_KEY.len() - 1];
                for refused in [
                    route.to_string(),
                    format!("{route}?"),
                    format!("{route}?key="),
                    format!("{route}?other={TEST_KEY}"),
                    format!("{route}?key={near}"),
                    format!("{route}?key={TEST_KEY}0"),
                    wrong,
                ] {
                    assert!(unauthorized(&refused, &[&host]), "{refused}");
                    assert!(
                        unauthorized(&refused, &[&host, "Origin: tauri://localhost"]),
                        "{refused}"
                    );
                    assert!(
                        unauthorized(&refused, &[&host, "Origin: http://localhost:5174"]),
                        "{refused}"
                    );
                    assert_eq!(
                        status_of(port, &refused, &[&host, "Origin: https://evil.example"]),
                        403,
                        "{refused}"
                    );
                }
                // The key may follow other parameters.
                let later = format!("{route}?x=1&key={TEST_KEY}");
                assert!(!unauthorized(&later, &[&host]), "{later}");
            }
            assert_eq!(status_of(port, &keyed("POST /mcp/x"), &[&host]), 200);
        });
        tests.await.expect("the checks ran");
    }

    /// The handshake a browser sends: a refusal must come before the upgrade, and an allowed
    /// origin, or none, must get through it.
    #[tokio::test]
    async fn a_websocket_handshake_is_refused_or_upgraded_by_origin() {
        let port = serve("handshake").await;
        let host = format!("Host: 127.0.0.1:{port}");
        tokio::task::spawn_blocking(move || {
            let handshake = |origin: Option<&str>| {
                let mut headers = vec![
                    host.as_str(),
                    "Connection: Upgrade",
                    "Upgrade: websocket",
                    "Sec-WebSocket-Version: 13",
                    "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
                ];
                headers.extend(origin);
                status_of(port, &keyed("GET /ws/control"), &headers)
            };
            assert_eq!(handshake(Some("Origin: https://evil.example")), 403);
            assert_eq!(handshake(Some("Origin: tauri://localhost")), 101);
            assert_eq!(handshake(None), 101);
        })
        .await
        .expect("the checks ran");
    }

    /// A path no route serves is still behind the guard: callers without the key learn nothing from
    /// it.
    #[tokio::test]
    async fn the_fallback_is_behind_the_guard() {
        let port = serve("fallback").await;
        let host = format!("Host: 127.0.0.1:{port}");
        tokio::task::spawn_blocking(move || {
            let nope = keyed("GET /nope");
            let hostile = status_of(port, &nope, &[&host, "Origin: https://evil.example"]);
            assert_eq!(hostile, 403);
            assert_eq!(status_of(port, "GET /nope", &[&host]), 401);
            assert_eq!(status_of(port, &nope, &[&host]), 404);
        })
        .await
        .expect("the checks ran");
    }

    /// A client drives a shell terminal end to end over the real sockets: it creates one over the
    /// control socket, types into it and resizes it over the terminal socket, finds its scrollback
    /// on attaching again, has one copy of the answers two attached clients give written, and
    /// closes it, which lets the terminal socket go.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_shell_terminal_is_driven_over_the_control_and_terminal_sockets() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (state, dir) = app_state("server-shell-terminal");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let project = crate::test_support::ScratchDir::new("server-shell-terminal-project");
        crate::test_support::insert_project(&state, &project);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = state.clone();
        let server = tokio::spawn(async move { axum::serve(listener, router(served)).await });

        let (control, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/control?key={TEST_KEY}"
        ))
        .await
        .unwrap();
        let (mut requests, mut events) = control.split();
        type Socket = tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >;
        async fn next_event(
            events: &mut futures_util::stream::SplitStream<Socket>,
        ) -> serde_json::Value {
            let frame = tokio::time::timeout(PATIENCE, events.next())
                .await
                .expect("an event in time")
                .unwrap()
                .unwrap();
            let Frame::Text(text) = frame else {
                panic!("a control frame that is not text: {frame:?}");
            };
            serde_json::from_str(&text).unwrap()
        }
        let snapshot = next_event(&mut events).await;
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["shell_terminals"], serde_json::json!([]));

        let create = r#"{"type":"create_shell_terminal","id":"create","project":"p1"}"#;
        requests.send(Frame::Text(create.into())).await.unwrap();
        let terminal = loop {
            let event = next_event(&mut events).await;
            if event["type"] == "shell_terminal_created" && event["id"] == "create" {
                break event["shell_terminal"]["id"].as_str().unwrap().to_string();
            }
        };

        let term_url = format!("ws://127.0.0.1:{port}/ws/term/{terminal}?key={TEST_KEY}");
        let (mut term, _) = tokio_tungstenite::connect_async(&term_url).await.unwrap();
        async fn read_until(term: &mut Socket, needle: &str) -> String {
            let mut seen = String::new();
            while !seen.contains(needle) {
                let frame = tokio::time::timeout(PATIENCE, term.next())
                    .await
                    .unwrap_or_else(|_| panic!("{needle:?} never arrived in {seen:?}"));
                match frame {
                    Some(Ok(Frame::Binary(bytes))) => {
                        seen.push_str(&String::from_utf8_lossy(&bytes))
                    }
                    // The replay's mark, which `replay_mark` reads.
                    Some(Ok(Frame::Text(_))) => {}
                    other => panic!(
                        "the terminal socket ended before {needle:?}: {other:?} after {seen:?}"
                    ),
                }
            }
            seen
        }
        async fn replay_mark(term: &mut Socket) -> serde_json::Value {
            let frame = tokio::time::timeout(PATIENCE, term.next())
                .await
                .expect("the replay's mark in time")
                .unwrap()
                .unwrap();
            let Frame::Text(text) = frame else {
                panic!("an attach that does not open with the replay's mark: {frame:?}");
            };
            let mark: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(mark["type"], "replay", "{mark}");
            mark
        }
        term.send(Frame::Binary(b"echo $((6*7))\r".to_vec()))
            .await
            .unwrap();
        read_until(&mut term, "42").await;
        term.send(Frame::Text(
            r#"{"type":"resize","cols":100,"rows":40}"#.into(),
        ))
        .await
        .unwrap();
        term.send(Frame::Binary(b"stty size\r".to_vec()))
            .await
            .unwrap();
        read_until(&mut term, "40 100").await;
        drop(term);

        let (mut again, _) = tokio_tungstenite::connect_async(&term_url).await.unwrap();
        let replay = read_until(&mut again, "40 100").await;
        assert!(replay.contains("42"), "{replay}");

        // Two clients answering the same output send the same answers: the first one's are
        // written, the other's copy is dropped, and so is a late answer to earlier output.
        let (mut other, _) = tokio_tungstenite::connect_async(&term_url).await.unwrap();
        let answered_at = replay_mark(&mut other).await["offset"].as_u64().unwrap();
        read_until(&mut other, "40 100").await;
        let reply = |data: &str, offset: u64| {
            Frame::Text(
                serde_json::json!({ "type": "reply", "data": data, "offset": offset }).to_string(),
            )
        };
        again
            .send(Frame::Binary(
                b"read -r answer; echo \"got[$answer]\"\r".to_vec(),
            ))
            .await
            .unwrap();
        read_until(&mut again, "got[$answer]").await;
        other.send(reply("first", answered_at)).await.unwrap();
        read_until(&mut again, "first").await;
        again.send(reply("copy", answered_at)).await.unwrap();
        again.send(reply("late", answered_at - 1)).await.unwrap();
        other.send(reply("\r", answered_at)).await.unwrap();
        let answered = read_until(&mut again, "]\r\n").await;
        assert!(answered.contains("got[first]"), "{answered}");
        drop(other);

        // A client attaching later is told its replay has been answered up to that point.
        let (mut third, _) = tokio_tungstenite::connect_async(&term_url).await.unwrap();
        let mark = replay_mark(&mut third).await;
        let behind = mark["offset"].as_u64().unwrap() - answered_at;
        assert_eq!(
            mark["answered"].as_u64().unwrap(),
            mark["length"].as_u64().unwrap() - behind,
            "{mark}"
        );
        drop(third);

        let close = format!(
            r#"{{"type":"close_shell_terminal","id":"close","shell_terminal":"{terminal}"}}"#
        );
        requests.send(Frame::Text(close)).await.unwrap();
        let mut deleted = false;
        loop {
            let event = next_event(&mut events).await;
            deleted |=
                event["type"] == "shell_terminal_deleted" && event["shell_terminal"] == terminal;
            if event["id"] == "close" {
                assert_eq!(event["type"], "ack", "{event}");
                break;
            }
        }
        assert!(deleted, "the deletion was not broadcast before the reply");
        let ended = tokio::time::timeout(PATIENCE, async {
            while let Some(Ok(frame)) = again.next().await {
                if matches!(frame, Frame::Close(_)) {
                    break;
                }
            }
        })
        .await;
        assert!(ended.is_ok(), "the terminal socket outlived its terminal");
        server.abort();
        drop(dir);
    }
}
