//! The routes the clean-up before uninstalling asks the running daemon on: what its features have
//! in effect outside Octoboard's own directories, and clearing all of it
//! (`crate::coordinator::system_changes`). Their callers are `octoboardd uninstall`, as its first
//! step, and the desktop application's own window, for its own daemon.
//!
//! The clean-up is done on the daemon's own machine. Both routes need the launch key, which only a
//! process of the daemon's user on this machine can read: a request the owner key admitted, as a
//! browser's and another device's are, is told `401`, and the control socket has no request for
//! either.

use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::coordinator::system_changes;
use crate::request_guard::key_matches;
use crate::state::AppState;

/// What is in effect (`SystemChangesListed`). Answers `200` with it as JSON and changes nothing; `401` to any key
/// but the launch key, and `500` where the store cannot be read.
pub(super) async fn list(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
) -> Response {
    if !key_matches(query.as_deref(), &state.key) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match tokio::task::spawn_blocking(move || system_changes::listed(&state)).await {
        Ok(Ok(listed)) => Json(listed).into_response(),
        Ok(Err(err)) => {
            tracing::error!(err = %format!("{err:#}"), "what is in effect could not be read for the clean-up");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(err) => {
            tracing::error!(%err, "listing for the clean-up did not finish");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Clears everything [`list`] names and turns the features behind it off, and answers `200` with
/// what became of each item (`SystemChangesCleared`) once all have ended; `401` to any key but
/// the launch key. The clearing runs as a task of its own, so that a caller that gives up
/// waiting does not leave a change of sharing cut short.
pub(super) async fn clear(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
) -> Response {
    if !key_matches(query.as_deref(), &state.key) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    tracing::info!("clearing what the daemon's features have in effect, for the clean-up");
    match tokio::spawn(async move { system_changes::cleared(&state).await }).await {
        Ok(cleared) => Json(cleared).into_response(),
        Err(err) => {
            tracing::error!(%err, "the clean-up did not finish");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::server::test_support::{keyed, response_of, serve_browser_state, serve_state};
    use crate::test_support::{console_with_project_and_accounts, PATIENCE, TEST_KEY};

    const ROUTES: [&str; 2] = ["POST /system-changes", "POST /system-changes/clear"];

    /// A state with one account that is recorded as sharing, served on the port and on the
    /// browser's way in with an owner key held.
    async fn serving(label: &str) -> (u16, u16, Arc<AppState>, crate::test_support::ScratchDir) {
        let (state, dir) = console_with_project_and_accounts(label);
        state
            .owner_key
            .enable(&crate::owner_key::path_in(&dir))
            .expect("the owner key");
        let port = serve_state(state.clone()).await;
        let browser = serve_browser_state(state.clone()).await;
        (port, browser, state, dir)
    }

    /// The first account of `state` that is not the default one's, recorded as sharing with its
    /// change incomplete: listed, and cleared with nothing to undo on disk.
    fn share_one(state: &AppState) -> String {
        let account = state
            .store
            .list_accounts()
            .unwrap()
            .into_iter()
            .next()
            .expect("an account");
        state
            .store
            .set_account_sharing(&account.id, true, true)
            .unwrap();
        account.id
    }

    /// The launch key lists what is in effect and changes nothing by it.
    #[tokio::test]
    async fn the_launch_key_lists_what_is_in_effect() {
        let (port, _browser, state, _dir) = serving("system-changes-route-list").await;
        let id = share_one(&state);
        let host = format!("Host: 127.0.0.1:{port}");
        let (status, body) = tokio::task::spawn_blocking(move || {
            response_of(port, &keyed("POST /system-changes"), &[&host])
        })
        .await
        .unwrap();
        assert_eq!(status, 200, "{body}");
        let listed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let [change] = listed["changes"].as_array().unwrap().as_slice() else {
            panic!("{body}");
        };
        assert_eq!(change["kind"], "account_sharing");
        assert_eq!(change["account"], id.as_str());
        assert!(AgentId::from_wire_name(change["agent"].as_str().unwrap()).is_some());
        assert_eq!(system_changes::list(&state.store).unwrap().len(), 1);
    }

    /// The launch key clears, and is answered what became of each item.
    #[tokio::test]
    async fn the_launch_key_clears_and_is_answered_each_item() {
        let (port, _browser, state, _dir) = serving("system-changes-route-clear").await;
        let id = share_one(&state);
        let host = format!("Host: 127.0.0.1:{port}");
        let (status, body) = tokio::task::spawn_blocking(move || {
            response_of(port, &keyed("POST /system-changes/clear"), &[&host])
        })
        .await
        .unwrap();
        assert_eq!(status, 200, "{body}");
        let cleared: serde_json::Value = serde_json::from_str(&body).unwrap();
        let [result] = cleared["results"].as_array().unwrap().as_slice() else {
            panic!("{body}");
        };
        assert_eq!(result["change"]["account"], id.as_str());
        assert!(result.get("failure").is_none(), "{body}");
        assert_eq!(cleared["failed"], serde_json::json!([]));
        assert!(system_changes::list(&state.store).unwrap().is_empty());
    }

    /// A browser or another device holds the owner key, never the launch key: on the browser's
    /// way in, where the owner key passes the guard, both routes still refuse it, and nothing is
    /// cleared. The port refuses the owner key on every route.
    #[tokio::test]
    async fn the_owner_key_can_neither_list_nor_clear() {
        let (port, browser, state, dir) = serving("system-changes-route-owner").await;
        share_one(&state);
        let owner = crate::owner_key::read_in(&dir).expect("the owner key");
        tokio::task::spawn_blocking(move || {
            for listener in [port, browser] {
                let host = format!("Host: 127.0.0.1:{listener}");
                for route in ROUTES {
                    let with = |key: &str| {
                        response_of(listener, &format!("{route}?key={key}"), &[&host]).0
                    };
                    assert_eq!(with(&owner), 401, "{route} on {listener}");
                    assert_eq!(with(&"0".repeat(64)), 401, "{route} on {listener}");
                }
                assert_eq!(
                    response_of(listener, "POST /system-changes/clear", &[&host]).0,
                    401
                );
            }
            // The owner key is a good one where it is taken, and the launch key lists there too.
            let host = format!("Host: 127.0.0.1:{browser}");
            assert_eq!(
                response_of(browser, &format!("GET /key-check?key={owner}"), &[&host]).0,
                204
            );
            assert_eq!(
                response_of(browser, &keyed("POST /system-changes"), &[&host]).0,
                200
            );
        })
        .await
        .unwrap();
        assert_eq!(system_changes::list(&state.store).unwrap().len(), 1);
    }

    /// A control connection has no request that lists or clears: the daemon answers either name
    /// as it answers any request it cannot read, and nothing is cleared.
    #[tokio::test]
    async fn a_control_connection_has_no_request_that_lists_or_clears() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let (port, _browser, state, _dir) = serving("system-changes-route-control").await;
        share_one(&state);
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{port}/ws/control?key={TEST_KEY}"
        ))
        .await
        .expect("the control connection");
        for name in ["list_system_changes", "clear_system_changes"] {
            let request = serde_json::json!({ "type": name, "id": name });
            socket
                .send(Message::text(request.to_string()))
                .await
                .unwrap();
            // What the daemon cannot read as a request it answers with `unreadable_request`, which
            // names the type it does not know; the snapshot and any other frame come first.
            let answer = tokio::time::timeout(PATIENCE, async {
                loop {
                    let text = match socket.next().await {
                        Some(Ok(Message::Text(text))) => text,
                        Some(Ok(_)) => continue,
                        other => {
                            panic!("the connection ended before {name} was answered: {other:?}")
                        }
                    };
                    let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if frame["type"] == "error" {
                        break frame;
                    }
                }
            })
            .await
            .expect("an answer");
            assert_eq!(answer["code"], "unreadable_request", "{answer}");
            assert!(
                answer["message"].as_str().unwrap_or("").contains(name),
                "{answer}"
            );
        }
        assert_eq!(system_changes::list(&state.store).unwrap().len(), 1);
    }

    /// The bodies of the two requests a client could once send no longer read as requests.
    #[test]
    fn the_removed_requests_do_not_parse() {
        for name in ["list_system_changes", "clear_system_changes"] {
            let text = serde_json::json!({ "type": name, "id": "1" }).to_string();
            assert!(
                serde_json::from_str::<crate::protocol::Request>(&text).is_err(),
                "{name}"
            );
        }
    }
}
