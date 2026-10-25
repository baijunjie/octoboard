//! The tools with which a console session pushes pages to its report panel, `show_page`, and reads
//! back the ones it pushed, `list_pages`. A session reads only its own pages.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::protocol::{now_millis, Event, Page, Session};
use crate::state::AppState;

use super::args::{optional_string, required_str};

pub(super) fn show_page(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let html = required_str(arguments, "html")?.to_string();
    let page = Page {
        id: Uuid::new_v4().to_string(),
        console_session_id: console_session.id.clone(),
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

/// The pages the caller pushed, oldest first, and only those: a page of another console session is
/// no more readable here than it is in that session's report panel. Without `page` each is listed
/// by id, time, size and title, since a page carries a whole document; with it, that one page in
/// full.
pub(super) fn list_pages(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    if let Some(wanted) = optional_string(arguments, "page") {
        // A page of another session reads as no page at all, not as one that is not the caller's.
        let page = state
            .store
            .get_page(&wanted)?
            .filter(|page| page.console_session_id == console_session.id)
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
        .list_pages(&console_session.id)?
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
    use crate::mcp::exec::tests::{arguments, peer_state};
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
}
