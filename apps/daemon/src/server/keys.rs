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

/// The request to rotate the owner key: a new one replaces it in its file and in memory, and the
/// old one is refused from then on, though a connection made with it stays. Answers `204`. Needs
/// the launch key, which the guard has not told from the owner key: a page that holds only the
/// owner key cannot replace it, so a request that carries any other is `401`. A daemon that holds
/// no owner key (it follows the desktop application, or could not read or write the file) answers
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
    use crate::server::test_support::{keyed, response_of, serve, serve_state, status_of};
    use crate::test_support::{app_state, app_state_with_key, PATIENCE, TEST_KEY};

    /// Serves the router over a state that holds an owner key kept in its scratch directory, and
    /// gives back the port, the state and the directory.
    async fn serve_with_owner_key(
        label: &str,
    ) -> (u16, Arc<AppState>, crate::test_support::ScratchDir) {
        let (state, dir) = app_state(label);
        state
            .owner_key
            .enable(&crate::owner_key::path_in(&dir))
            .expect("the owner key");
        let port = serve_state(state.clone()).await;
        (port, state, dir)
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
        let port = serve_state(second_state.clone()).await;

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

    /// A connection opened with the owner key is not closed by the rotation: it still answers a
    /// ping afterwards.
    #[tokio::test]
    async fn a_rotation_leaves_an_open_connection_open() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Frame;

        let (port, _state, dir) = serve_with_owner_key("server-rotate-open").await;
        let old = crate::owner_key::read_in(&dir).expect("the owner key");
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control?key={old}"))
                .await
                .expect("a control socket on the owner key");

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

        socket.send(Frame::Ping(vec![1])).await.unwrap();
        loop {
            let frame = tokio::time::timeout(PATIENCE, socket.next())
                .await
                .expect("an answer in time")
                .expect("the connection is still open")
                .unwrap();
            match frame {
                Frame::Pong(_) => break,
                Frame::Close(_) => panic!("the daemon closed the connection"),
                _ => {}
            }
        }
    }
}
