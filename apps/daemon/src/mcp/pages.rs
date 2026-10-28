//! The tools with which a session that faces the user, a console session or a project session that
//! is unbound or a lead session, pushes pages to its report panel, `show_page`, reads back the ones
//! it pushed, `list_pages`, and withdraws them, `delete_pages`. A session reads and deletes only
//! its own pages. A session that becomes a bound project session has them all withdrawn
//! ([`withdraw_pages`]).

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
    // The role is checked in the insert itself: this call's `session` was read before a binding
    // that is being written now, and a page stored behind that binding would be offered, with its
    // form, by a session that has no tool to read what it sends.
    if !state.store.insert_page_unless_bound(&page)? {
        bail!(
            "this session has the role of a bound session, which pushes no pages to the report \
             panel"
        );
    }
    #[cfg(test)]
    BEFORE_PAGE_CREATED.with(|hook| {
        if let Some(hook) = hook.borrow().as_ref() {
            hook(state, &session.id);
        }
    });
    state.broadcast(Event::PageCreated { page: page.clone() });
    // A binding or transfer, or a concurrent `delete_pages`, may withdraw the session's pages
    // between the insert and the broadcast above, and its `pages_deleted` would then reach the
    // panel before this `page_created`, which would show a page that is gone. Tell the panel
    // again, and refuse, rather than hand out the id of a page that no longer exists. A store that
    // cannot answer leaves the page stored and announced as it is: the push has succeeded, and
    // failing it now would hide an id that is live.
    match state.store.get_page(&page.id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            announce_deleted(state, &session.id, std::slice::from_ref(&page.id));
            bail!(
                "the page was withdrawn while it was being pushed (this session became a bound \
                 one, or its pages were deleted)"
            );
        }
        Err(err) => {
            tracing::warn!(
                page = %page.id,
                %err,
                "checking that a pushed page still exists failed"
            );
        }
    }
    Ok(json!({ "page": page.id }))
}

/// What a test runs after `show_page` stores a page and before it announces it, in place of the
/// binding, transfer or `delete_pages` that would withdraw the session's pages at that point.
#[cfg(test)]
type BeforePageCreated = Box<dyn Fn(&Arc<AppState>, &str)>;

#[cfg(test)]
thread_local! {
    static BEFORE_PAGE_CREATED: std::cell::RefCell<Option<BeforePageCreated>> =
        const { std::cell::RefCell::new(None) };
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
    announce_deleted(state, &session.id, &deleted);
    Ok(json!({ "deleted": deleted }))
}

/// Withdraws every page `session_id` pushed, as `delete_pages` with `all` does, for a session that
/// has just lost the report panel's tools by becoming a bound project session: its pages would
/// stay on the panel, and be submittable, with no way left for it to list or delete them.
pub(crate) fn withdraw_pages(state: &Arc<AppState>, session_id: &str) -> Result<()> {
    // With `None` the store names no id, so its `Err(id)` arm, a named id that is not the
    // session's, cannot happen.
    if let Ok(deleted) = state.store.delete_pages(session_id, None)? {
        announce_deleted(state, session_id, &deleted);
    }
    Ok(())
}

/// Withdraws the pages of every session that has the role of a bound one now: those a binding
/// change could not withdraw (a failed withdrawal, a page stored while the binding was being
/// written) and those pushed before pages were withdrawn on binding. Run once when the daemon
/// starts. One session's failure is logged and does not stop the others.
pub(crate) fn withdraw_stray_pages(state: &Arc<AppState>) -> Result<()> {
    for session_id in state.store.session_ids_with_pages_and_bound_role()? {
        if let Err(err) = withdraw_pages(state, &session_id) {
            tracing::warn!(
                session = %session_id, %err, "withdrawing the pages of a bound session failed"
            );
        }
    }
    Ok(())
}

/// Tells the panel which of a session's pages the store removed; nothing when none was.
fn announce_deleted(state: &Arc<AppState>, session_id: &str, deleted: &[String]) {
    if !deleted.is_empty() {
        state.broadcast(Event::PagesDeleted {
            console_session_id: session_id.to_string(),
            pages: deleted.to_vec(),
        });
    }
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

    /// A page withdrawn between its insert and its announcement, whose `pages_deleted` therefore
    /// reaches the panel before its `page_created`, is announced as deleted again and the push
    /// refused, so the panel does not keep a page that is gone and the session is not handed its
    /// id.
    #[tokio::test]
    async fn a_page_withdrawn_while_being_pushed_is_refused_and_announced_deleted() {
        let (state, _dir) = peer_state("peer-withdrawn-push");
        let mut events = state.subscribe();
        super::BEFORE_PAGE_CREATED.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|state, session| {
                super::withdraw_pages(state, session).unwrap();
            }));
        });
        let refused = call(
            &state,
            "starter",
            "show_page",
            &arguments(json!({ "html": "<p>x</p>" })),
        )
        .await;
        super::BEFORE_PAGE_CREATED.with(|hook| *hook.borrow_mut() = None);
        let err = refused.expect_err("the page was withdrawn");
        assert!(err.to_string().contains("was withdrawn while"), "{err}");
        let listed = call(&state, "starter", "list_pages", &Map::new())
            .await
            .unwrap();
        assert_eq!(listed["pages"], json!([]));

        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            match event {
                Event::PageCreated { page } => seen.push(format!("created {}", page.id)),
                Event::PagesDeleted { pages, .. } => seen.push(format!("deleted {pages:?}")),
                _ => {}
            }
        }
        assert_eq!(seen.len(), 3, "{seen:?}");
        let id = seen[1].strip_prefix("created ").expect("created second");
        assert_eq!(seen[0], format!("deleted {:?}", [id]));
        assert_eq!(seen[2], seen[0]);
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

    /// A push whose caller was read before the session was bound stores no page: the role is
    /// checked where the page is written, so a binding that lands between the dispatcher's check
    /// and the insert cannot leave a page behind it.
    #[test]
    fn a_push_racing_a_binding_leaves_no_page_behind() {
        let (state, _dir) = peer_state("peer-push-race");
        let before_binding = state.session_record("other-project").unwrap();
        state
            .store
            .set_binding("other-project", None, "starter", false, true, false)
            .unwrap()
            .unwrap();
        let mut events = state.subscribe();

        let err = super::show_page(
            &state,
            &before_binding,
            &arguments(json!({ "html": "<p>x</p>" })),
        )
        .expect_err("bound by now");
        assert!(err.to_string().contains("role of a bound session"), "{err}");
        assert!(state.store.list_pages("other-project").unwrap().is_empty());
        assert!(events.try_recv().is_err(), "no page was announced");

        // A lead session is bound too, and keeps its pages.
        let (state, _dir) = lead_state("peer-push-lead");
        let lead = state.session_record("starter").unwrap();
        super::show_page(&state, &lead, &arguments(json!({ "html": "<p>x</p>" }))).unwrap();
        assert_eq!(state.store.list_pages("starter").unwrap().len(), 1);
    }

    /// At start the daemon withdraws the pages of every session with the role of a bound one, and
    /// no other's: a lead session's and an unbound session's stay.
    #[test]
    fn the_start_sweep_withdraws_the_pages_left_on_bound_sessions() {
        let (state, _dir) = lead_state("peer-sweep-pages");
        // `starter` is a lead session, `mine` is bound to it, `other-project` is unbound.
        for (id, session) in [
            ("l-1", "starter"),
            ("b-1", "mine"),
            ("b-2", "mine"),
            ("u-1", "other-project"),
        ] {
            state
                .store
                .insert_page(&crate::test_support::test_page(id, session, 1))
                .unwrap();
        }
        let mut events = state.subscribe();

        super::withdraw_stray_pages(&state).unwrap();

        assert!(state.store.list_pages("mine").unwrap().is_empty());
        assert_eq!(state.store.list_pages("starter").unwrap().len(), 1);
        assert_eq!(state.store.list_pages("other-project").unwrap().len(), 1);
        let Event::PagesDeleted {
            console_session_id,
            pages,
        } = events.try_recv().unwrap()
        else {
            panic!("the panels must be told");
        };
        assert_eq!(console_session_id, "mine");
        assert_eq!(pages.len(), 2);
        assert!(events.try_recv().is_err());

        // Nothing left to withdraw: nothing said.
        super::withdraw_stray_pages(&state).unwrap();
        assert!(events.try_recv().is_err());
    }
}
