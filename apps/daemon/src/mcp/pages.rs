//! The tools with which a session that faces the user, a console session or a project session that
//! is unbound or a lead session, pushes pages to its report panel, `show_page`, reads back the ones
//! it pushed, `list_pages`, and withdraws them, `delete_pages`. A session reads and deletes only
//! its own pages.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::protocol::{now_millis, Event, Page, Session};
use crate::state::AppState;

use super::args::{optional_string, required_str};

pub(super) fn show_page(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let html = required_str(arguments, "html")?.to_string();
    let page = Page {
        id: Uuid::new_v4().to_string(),
        console_session_id: session.id.clone(),
        html,
        // No agent exposes a message id to put here yet; the rewind linkage that would read it is
        // not built.
        anchor_message_id: None,
        created_at: now_millis(),
    };
    state.store.insert_page(&page)?;
    state.broadcast(Event::PageCreated { page: page.clone() });
    Ok(json!({ "page": page.id }))
}

/// The pages the caller pushed, oldest first, and only those: a page of another session is no more
/// readable here than it is in that session's report panel. Without `page` each is listed
/// by id, time, size and title, since a page carries a whole document; with it, that one page in
/// full.
pub(super) fn list_pages(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    if let Some(wanted) = optional_string(arguments, "page") {
        // A page of another session reads as no page at all, not as one that is not the caller's.
        let page = state
            .store
            .get_page(&wanted)?
            .filter(|page| page.console_session_id == session.id)
            .ok_or_else(|| anyhow!("you have pushed no page `{wanted}`"))?;
        return Ok(json!({
            "page": page.id,
            "created_at": page.created_at,
            "title": page_title(&page.html),
            "html": page.html,
        }));
    }
    let listed: Vec<Value> = state
        .store
        .list_pages(&session.id)?
        .iter()
        .map(|page| {
            json!({
                "page": page.id,
                "created_at": page.created_at,
                "html_bytes": page.html.len(),
                "title": page_title(&page.html),
            })
        })
        .collect();
    Ok(json!({ "pages": listed }))
}

/// Deletes pages the caller pushed, named in `pages` or all of them with `all`, and tells the
/// panel of the ones the store actually removed. Nothing is deleted when any named id is not the
/// caller's own: it reads as no page at all, as it does in `list_pages`, so the refusal does not
/// reveal another session's page.
pub(super) fn delete_pages(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let all = arguments
        .get("all")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| anyhow!("`all` must be true or false"))
        })
        .transpose()?
        .unwrap_or(false);
    let named: Option<Vec<String>> = arguments
        .get("pages")
        .map(|value| {
            value
                .as_array()
                .and_then(|ids| {
                    ids.iter()
                        .map(|id| id.as_str().map(str::to_string))
                        .collect()
                })
                .ok_or_else(|| anyhow!("`pages` must be an array of page ids"))
        })
        .transpose()?;
    let result = match (all, named) {
        (true, None) => state.store.delete_pages(&session.id, None)?,
        (false, Some(ids)) if !ids.is_empty() => {
            state.store.delete_pages(&session.id, Some(&ids))?
        }
        _ => bail!("name the pages to delete in `pages`, or pass `all: true`, not both or neither"),
    };
    let deleted = result.map_err(|id| anyhow!("you have pushed no page `{id}`"))?;
    if !deleted.is_empty() {
        state.broadcast(Event::PagesDeleted {
            console_session_id: session.id.clone(),
            pages: deleted.clone(),
        });
    }
    Ok(json!({ "deleted": deleted }))
}

/// The text of a page's `<title>`, when it has a non-blank one.
fn page_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = start + lower[start..].find('>')? + 1;
    let end = open_end + lower[open_end..].find("</title")?;
    // Lowercasing ASCII keeps every byte offset, so the same range of the original is the title.
    let title = html[open_end..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use crate::mcp::exec::call;
    use crate::mcp::exec::tests::{arguments, lead_state, peer_state};
    use crate::protocol::Event;
    use serde_json::{json, Map};

    /// A console session lists, and reads in full, the pages it pushed, and no other session's.
    #[tokio::test]
    async fn a_console_session_lists_only_its_own_pages() {
        let (state, _dir) = peer_state("peer-list-pages");
        let push = |caller: &'static str, html: &'static str| {
            let state = state.clone();
            async move {
                call(
                    &state,
                    caller,
                    "show_page",
                    &arguments(json!({ "html": html })),
                )
                .await
                .unwrap()["page"]
                    .as_str()
                    .unwrap()
                    .to_string()
            }
        };
        let first = push("a", "<html><head><title> Plan\n b </title></head></html>").await;
        let second = push("a", "<p>no title</p>").await;
        let other = push("b", "<p>b's</p>").await;

        let listed = call(&state, "a", "list_pages", &Map::new()).await.unwrap();
        let pages = listed["pages"].as_array().unwrap();
        let ids: Vec<&str> = pages
            .iter()
            .map(|page| page["page"].as_str().unwrap())
            .collect();
        // Pushed in the same millisecond they may tie, and then the order is the ids'.
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&first.as_str()) && ids.contains(&second.as_str()));
        let page_of = |id: &str| pages.iter().find(|page| page["page"] == id).unwrap();
        assert_eq!(page_of(&first)["title"], "Plan b");
        assert!(page_of(&second)["title"].is_null());
        assert_eq!(page_of(&second)["html_bytes"], "<p>no title</p>".len());
        assert!(page_of(&first).get("html").is_none());

        let one = call(
            &state,
            "a",
            "list_pages",
            &arguments(json!({ "page": second })),
        )
        .await
        .unwrap();
        assert_eq!(one["html"], "<p>no title</p>");
        let err = call(
            &state,
            "a",
            "list_pages",
            &arguments(json!({ "page": other })),
        )
        .await
        .expect_err("another session's page");
        assert!(err.to_string().contains("no page"), "{err}");
    }

    /// A project session that faces the user pushes, lists and deletes pages like a console
    /// session does, a lead session included, and a session bound to another has none of the
    /// three tools.
    #[tokio::test]
    async fn only_a_project_session_facing_the_user_has_the_page_tools() {
        let (state, _dir) = lead_state("peer-project-pages");
        let page = |caller: &'static str, html: &'static str| {
            let state = state.clone();
            async move {
                call(
                    &state,
                    caller,
                    "show_page",
                    &arguments(json!({ "html": html })),
                )
                .await
            }
        };
        // `starter` is a lead session, `other-project` an unbound one.
        for caller in ["starter", "other-project"] {
            let id = page(caller, "<p>mine</p>").await.unwrap()["page"]
                .as_str()
                .unwrap()
                .to_string();
            let listed = call(&state, caller, "list_pages", &Map::new())
                .await
                .unwrap();
            assert_eq!(listed["pages"][0]["page"], id.as_str());
            let page = state.store.get_page(&id).unwrap().unwrap();
            assert_eq!(page.console_session_id, caller);
        }
        let err = page("mine", "<p>x</p>").await.expect_err("bound");
        assert!(err.to_string().contains("is not a tool"), "{err}");
        for tool in ["list_pages", "delete_pages"] {
            let err = call(&state, "mine", tool, &arguments(json!({ "all": true })))
                .await
                .expect_err("bound");
            assert!(err.to_string().contains("is not a tool"), "{err}");
        }
    }

    /// Deleting names pages or passes `all`; it takes only the caller's own pages, refuses a call
    /// naming any other (nothing is deleted then), says so to the panels, and moves "the newest"
    /// back to the page before.
    #[tokio::test]
    async fn a_session_deletes_only_its_own_pages() {
        let (state, _dir) = peer_state("peer-delete-pages");
        let mut events = state.subscribe();
        // Pages are ordered by the millisecond they were pushed at, so each push waits for the next
        // one: three pushed within a millisecond would tie and fall back to their random ids.
        let push = |caller: &'static str| {
            let state = state.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                call(
                    &state,
                    caller,
                    "show_page",
                    &arguments(json!({ "html": "<p>x</p>" })),
                )
                .await
                .unwrap()["page"]
                    .as_str()
                    .unwrap()
                    .to_string()
            }
        };
        let first = push("starter").await;
        let second = push("starter").await;
        let third = push("starter").await;
        let foreign = push("other-project").await;
        let delete = |caller: &'static str, args: serde_json::Value| {
            let state = state.clone();
            async move { call(&state, caller, "delete_pages", &arguments(args)).await }
        };
        while events.try_recv().is_ok() {}

        // One foreign id refuses the whole call, as an unknown one does, and deletes nothing.
        for unknown in [foreign.as_str(), "no-such-page"] {
            let err = delete("starter", json!({ "pages": [first, unknown] }))
                .await
                .expect_err("not the caller's");
            assert!(err.to_string().contains("no page"), "{err}");
        }
        assert_eq!(state.store.list_pages("starter").unwrap().len(), 3);
        assert!(events.try_recv().is_err());

        // Neither argument, both, an empty list and a malformed one are refused.
        for bad in [
            json!({}),
            json!({ "all": false }),
            json!({ "all": true, "pages": [first] }),
            json!({ "pages": [] }),
            json!({ "pages": "x" }),
            json!({ "pages": [1] }),
        ] {
            delete("starter", bad).await.expect_err("bad arguments");
        }
        assert_eq!(state.store.list_pages("starter").unwrap().len(), 3);

        // The newest goes, and the one before it is the newest: a form submits from that one.
        let done = delete("starter", json!({ "pages": [third, third] }))
            .await
            .unwrap();
        assert_eq!(done["deleted"], json!([third]));
        let Event::PagesDeleted {
            console_session_id,
            pages,
        } = events.try_recv().unwrap()
        else {
            panic!("the panels must be told");
        };
        assert_eq!(console_session_id, "starter");
        assert_eq!(pages, std::slice::from_ref(&third));
        assert_eq!(
            state.store.newest_page_id("starter").unwrap().as_deref(),
            Some(second.as_str())
        );

        // `all` takes every page of the caller and none of another session's.
        let done = delete("starter", json!({ "all": true })).await.unwrap();
        assert_eq!(done["deleted"], json!([first, second]));
        assert!(state.store.list_pages("starter").unwrap().is_empty());
        assert_eq!(state.store.list_pages("other-project").unwrap().len(), 1);
        let Event::PagesDeleted { pages, .. } = events.try_recv().unwrap() else {
            panic!("the panels must be told");
        };
        assert_eq!(pages, [first, second]);

        // With nothing left it deletes nothing and says nothing.
        let done = delete("starter", json!({ "all": true })).await.unwrap();
        assert_eq!(done["deleted"], json!([]));
        assert!(events.try_recv().is_err());
    }
}
