//! The routes about the keys a request carries: the key check and the owner-key rotation.

use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::owner_key::RotateError;
use crate::request_guard::key_matches;
use crate::state::AppState;

/// The check of a key: the guard has accepted the one the request carries, the launch key or the
/// owner key, so there is nothing to do but say so with `204`. A wrong one never gets here, it is
/// told `401`. A page tells a key the daemon refuses from a daemon it cannot reach by it.
pub(super) async fn key_check() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// The request to rotate the owner key: a new one replaces it in its file and in memory, the old
/// one is refused from then on, and every connection it admitted is closed. Answers `204`. Needs
/// the launch key, which the guard has not told from the owner key: a page that holds only the
/// owner key cannot replace it, so a request that carries any other is `401`. A daemon that holds
/// no owner key (it serves no browser, or could not read or write the file) answers
/// `409`, and a key that cannot be written `500`, the old key standing; both carry a JSON body
/// with the `reason` and the `message`.
pub(super) async fn rotate_owner_key(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
) -> Response {
    if !key_matches(query.as_deref(), &state.key) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let owner_key = state.owner_key.clone();
    let rotated = tokio::task::spawn_blocking(move || owner_key.rotate())
        .await
        .map_err(|err| tracing::error!(%err, "the owner key rotation did not finish"));
    match rotated {
        Ok(Ok(())) => {
            tracing::info!("the owner key was rotated");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(error)) => {
            let status = match &error {
                RotateError::NotHeld => StatusCode::CONFLICT,
                RotateError::Write(err) => {
                    tracing::warn!(%err, "writing the new owner key failed; the old one stands");
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            };
            refused(status, &error)
        }
        Err(()) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// The answer to a rotation that did not happen: `status` and a JSON body naming why.
fn refused(status: StatusCode, error: &RotateError) -> Response {
    let body = serde_json::json!({ "reason": error.code(), "message": error.message() });
    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::test_support::{
        keyed, response_of, serve, serve_browser_state, serve_state, status_of,
    };
    use crate::test_support::{app_state, app_state_with_key, PATIENCE, TEST_KEY};

    /// Serves the browser socket's router over a state that holds an owner key kept in its
    /// scratch directory, and gives back the port, the state and the directory.
    async fn serve_with_owner_key(
        label: &str,
    ) -> (u16, Arc<AppState>, crate::test_support::ScratchDir) {
        let (state, dir) = app_state(label);
        state
            .owner_key
            .enable(&crate::owner_key::path_in(&dir))
            .expect("the owner key");
        let port = serve_browser_state(state.clone()).await;
        (port, state, dir)
    }

    /// The TCP port never takes the owner key, on any route, a WebSocket upgrade included, while
    /// the browser socket's router does; the launch key opens both.
    #[tokio::test]
    async fn the_owner_key_is_refused_on_the_port_and_accepted_on_the_socket() {
        let (browser, state, dir) = serve_with_owner_key("server-owner-key-port").await;
        let port = serve_state(state.clone()).await;
        let owner = crate::owner_key::read_in(&dir).expect("the owner key");
        tokio::task::spawn_blocking(move || {
            for (listener, takes_owner_key) in [(port, false), (browser, true)] {
                let host = format!("Host: 127.0.0.1:{listener}");
                let status = |line: &str| status_of(listener, line, &[&host]);
                let owner_status = if takes_owner_key { 204 } else { 401 };
                assert_eq!(
                    status(&owner_request(&owner, "GET /key-check")),
                    owner_status,
                    "{listener}"
                );
                assert_eq!(status(&keyed("GET /key-check")), 204, "{listener}");
                for route in ["GET /ws/control", "GET /ws/term/s", "POST /mcp/x"] {
                    let refused = status(&owner_request(&owner, route)) == 401;
                    assert_eq!(refused, !takes_owner_key, "{route} on {listener}");
                }
            }
        })
        .await
        .expect("the checks ran");
    }

    /// A real WebSocket handshake with the owner key is refused with 401 by the TCP port, on the
    /// control and the terminal route alike, and upgraded by the browser socket's router on the
    /// control route.
    #[tokio::test]
    async fn the_owner_key_cannot_open_a_websocket_on_the_port() {
        use tokio_tungstenite::tungstenite::Error;

        let (browser, state, dir) = serve_with_owner_key("server-owner-key-ws").await;
        let port = serve_state(state.clone()).await;
        let owner = crate::owner_key::read_in(&dir).expect("the owner key");

        for route in ["/ws/control", "/ws/term/s"] {
            let refused = tokio_tungstenite::connect_async(format!(
                "ws://127.0.0.1:{port}{route}?key={owner}"
            ))
            .await
            .expect_err("the port refuses the owner key");
            match refused {
                Error::Http(response) => assert_eq!(response.status().as_u16(), 401, "{route}"),
                other => panic!("{route}: not an HTTP refusal: {other:?}"),
            }
        }
        let launch = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/control?key={TEST_KEY}"
        ))
        .await;
        assert!(launch.is_ok(), "the port takes the launch key");
        let accepted = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{browser}/ws/control?key={owner}"
        ))
        .await;
        assert!(accepted.is_ok(), "the socket's router takes the owner key");
    }

    fn owner_request(owner: &str, request_line: &str) -> String {
        format!("{request_line}?key={owner}")
    }

    /// A key check answers 204 to the launch key and to the owner key, 401 to any other.
    #[tokio::test]
    async fn the_key_check_accepts_the_launch_key_and_the_owner_key() {
        let (port, _state, dir) = serve_with_owner_key("server-key-check").await;
        let owner = crate::owner_key::read_in(&dir).expect("the owner key");
        tokio::task::spawn_blocking(move || {
            let host = format!("Host: 127.0.0.1:{port}");
            let status = |line: &str| status_of(port, line, &[&host]);
            assert_eq!(status(&keyed("GET /key-check")), 204);
            assert_eq!(status(&owner_request(&owner, "GET /key-check")), 204);
            assert_eq!(
                status(&owner_request(&"0".repeat(64), "GET /key-check")),
                401
            );
            assert_eq!(status("GET /key-check"), 401);
        })
        .await
        .expect("the checks ran");
    }

    /// A daemon with no owner key (the desktop application's) answers a rotation with 409.
    #[tokio::test]
    async fn a_daemon_without_an_owner_key_refuses_to_rotate() {
        let port = serve("no-owner-key").await;
        tokio::task::spawn_blocking(move || {
            let host = format!("Host: 127.0.0.1:{port}");
            let (status, body) = response_of(port, &keyed("POST /owner-key/rotate"), &[&host]);
            assert_eq!(status, 409);
            assert!(body.contains("no_owner_key"), "{body}");
        })
        .await
        .expect("the checks ran");
    }

    /// The rotation needs the launch key: the owner key passes the guard but is told 401, and
    /// nothing changes. With the launch key it answers 204, the old key is then refused and the
    /// new one, the one in the file, accepted.
    #[tokio::test]
    async fn the_owner_key_rotates_only_on_the_launch_key() {
        let (port, _state, dir) = serve_with_owner_key("server-rotate").await;
        let old = crate::owner_key::read_in(&dir).expect("the owner key");
        tokio::task::spawn_blocking(move || {
            let host = format!("Host: 127.0.0.1:{port}");
            let status = |line: &str| status_of(port, line, &[&host]);
            assert_eq!(status("POST /owner-key/rotate"), 401);
            assert_eq!(status(&owner_request(&old, "POST /owner-key/rotate")), 401);
            assert_eq!(status(&owner_request(&old, "GET /key-check")), 204);
            assert_eq!(
                crate::owner_key::read_in(&dir).as_deref(),
                Some(old.as_str())
            );

            assert_eq!(status(&keyed("POST /owner-key/rotate")), 204);
            let new = crate::owner_key::read_in(&dir).expect("the new owner key");
            assert_ne!(new, old);
            assert_eq!(status(&owner_request(&old, "GET /key-check")), 401);
            assert_eq!(status(&owner_request(&new, "GET /key-check")), 204);
            // The launch key is unchanged.
            assert_eq!(status(&keyed("GET /key-check")), 204);
        })
        .await
        .expect("the checks ran");
    }

    /// A daemon started again over the same data directory has a new launch key and accepts the
    /// same owner key: the address a browser bookmarked still opens it, and the old launch key
    /// does not.
    #[tokio::test]
    async fn a_restart_over_the_same_data_directory_accepts_the_same_owner_key() {
        let (first_state, data) = app_state("server-owner-restart-first");
        first_state
            .owner_key
            .enable(&crate::owner_key::path_in(&data))
            .expect("the owner key");
        let owner = crate::owner_key::read_in(&data).expect("the owner key");
        let first_launch = TEST_KEY.to_string();

        let second_launch = "f".repeat(64);
        let (second_state, _second_dir) =
            app_state_with_key("server-owner-restart-second", &second_launch);
        second_state
            .owner_key
            .enable(&crate::owner_key::path_in(&data))
            .expect("the owner key");
        let port = serve_browser_state(second_state.clone()).await;

        tokio::task::spawn_blocking(move || {
            let host = format!("Host: 127.0.0.1:{port}");
            let status =
                |key: &str| status_of(port, &owner_request(key, "GET /key-check"), &[&host]);
            assert_eq!(status(&owner), 204, "the owner key");
            assert_eq!(status(&second_launch), 204, "the new launch key");
            assert_eq!(status(&first_launch), 401, "the earlier launch key");
        })
        .await
        .expect("the checks ran");
    }

    /// A rotation whose new key cannot be written is 500 with the reason in its body and changes
    /// nothing: the old key is still accepted and still in the file.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_rotation_that_cannot_write_is_500_and_keeps_the_old_key() {
        use std::os::unix::fs::PermissionsExt;

        let (port, _state, dir) = serve_with_owner_key("server-rotate-fails").await;
        let old = crate::owner_key::read_in(&dir).expect("the owner key");
        let kept = dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let host = format!("Host: 127.0.0.1:{port}");
            let status = |line: &str| status_of(port, line, &[&host]);
            // A directory that takes no new file; a process that bypasses the mode (root) rotates.
            std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o500)).unwrap();
            let (rotated, body) = response_of(port, &keyed("POST /owner-key/rotate"), &[&host]);
            std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o700)).unwrap();
            if rotated == 204 {
                return;
            }
            assert_eq!(rotated, 500);
            assert!(body.contains("write_failed"), "{body}");
            assert_eq!(
                crate::owner_key::read_in(&kept).as_deref(),
                Some(old.as_str())
            );
            assert_eq!(status(&owner_request(&old, "GET /key-check")), 204);
        })
        .await
        .expect("the checks ran");
    }

    /// A rotation closes every connection the old owner key opened, at once and without the
    /// client doing anything, while one the launch key opened stays open.
    #[tokio::test]
    async fn a_rotation_closes_the_connections_the_old_key_opened() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (port, _state, dir) = serve_with_owner_key("server-rotate-open").await;
        let old = crate::owner_key::read_in(&dir).expect("the owner key");
        let (mut owners, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control?key={old}"))
                .await
                .expect("a control socket on the owner key");
        let (mut launch, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/control?key={TEST_KEY}"
        ))
        .await
        .expect("a control socket on the launch key");

        tokio::task::spawn_blocking(move || {
            let host = format!("Host: 127.0.0.1:{port}");
            assert_eq!(
                status_of(port, &keyed("POST /owner-key/rotate"), &[&host]),
                204
            );
            assert_eq!(
                status_of(port, &owner_request(&old, "GET /key-check"), &[&host]),
                401
            );
        })
        .await
        .expect("the checks ran");

        // Whatever the daemon had already sent may come first; then the connection ends.
        tokio::time::timeout(PATIENCE, async {
            loop {
                match owners.next().await {
                    None | Some(Err(_)) | Some(Ok(Frame::Close(_))) => break,
                    Some(Ok(_)) => {}
                }
            }
        })
        .await
        .expect("the owner key's connection was closed");

        launch.send(Frame::Ping(vec![1])).await.unwrap();
        loop {
            let frame = tokio::time::timeout(PATIENCE, launch.next())
                .await
                .expect("an answer in time")
                .expect("the launch key's connection is still open")
                .unwrap();
            match frame {
                Frame::Pong(_) => break,
                Frame::Close(_) => panic!("the daemon closed the launch key's connection"),
                _ => {}
            }
        }
    }
}
