//! The tools with which a session that faces the user, a console session or a project session that
//! is unbound or a coordinator with a parent, reads up on the pages of its report panel,
//! `page_guide`, pushes them, `show_page`, reads back the ones it pushed, `list_pages`, and
//! withdraws them, `delete_pages`. A session reads and deletes only its own pages. A session that
//! becomes a worker has them all withdrawn ([`crate::coordinator::pages::withdraw_pages`]).

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::coordinator::pages::announce_deleted;
use crate::protocol::{Event, Page, PageContent, PageContentError, Session};
use crate::session_tools::page_guide as guide;
use crate::state::AppState;
use crate::time::now_millis;

use crate::mcp::lookup::{optional_bool, optional_string, optional_string_list, required_str};

pub(in crate::mcp) fn show_page(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    // The arguments are the page: its `title` and its `body`. Nothing is stored or announced for a
    // page that does not match the schemas of its kinds.
    let content = PageContent::parse(&Value::Object(arguments.clone())).map_err(refused_page)?;
    let page = Page {
        id: Uuid::new_v4().to_string(),
        session_id: session.id.clone(),
        content,
        submission: None,
        submission_revision: 0,
        created_at: now_millis(),
    };
    // The role is checked in the insert itself: this call's `session` was read before a binding
    // that is being written now, and a page stored behind that binding would be offered, with its
    // form, by a session that has no tool to read what it sends.
    if !state.store.insert_page_unless_worker(&page)? {
        bail!("this session has the role of a worker, which pushes no pages to the report panel");
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
                "the page was withdrawn while it was being pushed (this session became a worker, \
                 or its pages were deleted)"
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

/// The refusal of a page: where it is wrong, and the schema of the part that is, so that the agent
/// can correct it without another call.
fn refused_page(error: PageContentError) -> anyhow::Error {
    let schema = serde_json::to_string_pretty(&error.schema()).unwrap_or_default();
    let of = match error.kind {
        Some(kind) => format!("the `{kind}` component"),
        None => "the page object".to_string(),
    };
    anyhow!("the page was not pushed: {error}\n\nThe schema of {of}:\n{schema}")
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

/// What `page_guide` answers: the guide of the topic asked for and the schemas of the kinds of
/// component it covers.
pub(in crate::mcp) fn page_guide(arguments: &Map<String, Value>) -> Result<Value> {
    let wanted = required_str(arguments, "topic")?;
    let topic = guide::topic(wanted).ok_or_else(|| {
        let known: Vec<&str> = guide::TOPICS.iter().map(|topic| topic.name).collect();
        anyhow!("no topic `{wanted}`; the topics are {}", known.join(", "))
    })?;
    Ok(guide::read(topic))
}

/// The pages the caller pushed, oldest first, and only those: a page of another session is no more
/// readable here than it is in that session's report panel. Without `page` each is listed
/// by id, time, size and title, since a page carries a whole component tree; with it, that one
/// page in full, with its submission.
pub(in crate::mcp) fn list_pages(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    if let Some(wanted) = optional_string(arguments, "page") {
        // A page of another session reads as no page at all, not as one that is not the caller's.
        let page = state
            .store
            .get_page(&wanted)?
            .filter(|page| page.session_id == session.id)
            .ok_or_else(|| anyhow!("you have pushed no page `{wanted}`"))?;
        return Ok(json!({
            "page": page.id,
            "created_at": page.created_at,
            "title": page.content.title,
            "content": page.content,
            "submission": page.submission,
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
                "content_bytes": serde_json::to_string(&page.content).map_or(0, |text| text.len()),
                "title": page.content.title,
            })
        })
        .collect();
    Ok(json!({ "pages": listed }))
}

/// Deletes pages the caller pushed, named in `pages` or all of them with `all`, and tells the
/// panel of the ones the store actually removed. Nothing is deleted when any named id is not the
/// caller's own: it reads as no page at all, as it does in `list_pages`, so the refusal does not
/// reveal another session's page.
pub(in crate::mcp) fn delete_pages(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let all = optional_bool(arguments, "all")?.unwrap_or(false);
    let named = optional_string_list(arguments, "pages")?;
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

#[cfg(test)]
mod tests {
    use crate::mcp::dispatch::call;
    use crate::mcp::test_support::{arguments, bound_coordinator_state, peer_state};
    use crate::protocol::{Event, Role};
    use serde_json::{json, Map, Value};

    /// The arguments of a `show_page` of one Markdown component.
    fn page(text: &str) -> Value {
        json!({ "body": [{ "type": "markdown", "text": text }] })
    }

    /// A console session lists, and reads in full, the pages it pushed, and no other session's.
    #[tokio::test]
    async fn a_console_session_lists_only_its_own_pages() {
        let (state, _dir) = peer_state("peer-list-pages");
        let push = |caller: &'static str, args: Value| {
            let state = state.clone();
            async move {
                call(&state, caller, "show_page", &arguments(args))
                    .await
                    .unwrap()["page"]
                    .as_str()
                    .unwrap()
                    .to_string()
            }
        };
        let mut titled = page("plan");
        titled["title"] = json!("Plan b");
        let first = push("a", titled.clone()).await;
        let second = push("a", page("no title")).await;
        let other = push("b", page("b's")).await;

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
        assert_eq!(
            page_of(&second)["content_bytes"],
            page("no title").to_string().len()
        );
        assert!(page_of(&first).get("content").is_none());
        assert!(page_of(&first).get("html_bytes").is_none());

        let one = call(
            &state,
            "a",
            "list_pages",
            &arguments(json!({ "page": second })),
        )
        .await
        .unwrap();
        assert_eq!(one["content"], page("no title"));
        assert!(one["submission"].is_null());
        assert!(one["title"].is_null());
        let titled_page = call(
            &state,
            "a",
            "list_pages",
            &arguments(json!({ "page": first })),
        )
        .await
        .unwrap();
        assert_eq!(titled_page["title"], "Plan b");
        assert_eq!(titled_page["content"], titled);
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

    /// A page of Markdown, a section and a form is stored as the content it was pushed with,
    /// announced to the panels with that content and no submission, and its id answered.
    #[tokio::test]
    async fn a_valid_page_is_stored_and_announced() {
        let (state, _dir) = peer_state("peer-show-page");
        let mut events = state.subscribe();
        let sent = json!({
            "title": "Decide",
            "body": [
                { "type": "markdown", "text": "## Pick" },
                { "type": "section", "children": [
                    { "type": "radio", "name": "color", "label": "Colour",
                      "options": [{ "value": "red" }, { "value": "blue", "label": "Blue" }] },
                ] },
                { "type": "choices", "name": "action", "options": [{ "value": "ok" }] },
            ],
        });
        let result = call(&state, "a", "show_page", &arguments(sent.clone()))
            .await
            .unwrap();
        let id = result["page"].as_str().unwrap();
        assert_eq!(result.as_object().unwrap().len(), 1);
        let stored = state.store.get_page(id).unwrap().unwrap();
        assert_eq!(serde_json::to_value(&stored.content).unwrap(), sent);
        assert!(stored.submission.is_none());
        let Event::PageCreated { page } = events.try_recv().unwrap() else {
            panic!("the panels must be told");
        };
        assert_eq!(page.id, id);
        assert_eq!(page.content, stored.content);
        assert!(events.try_recv().is_err());
    }

    /// A page that breaks the schemas is refused with the path of the failing part and the schema
    /// of its kind, whatever the arguments are, and nothing is stored or announced.
    #[tokio::test]
    async fn an_invalid_page_is_refused_with_its_path_and_schema_and_stores_nothing() {
        let (state, _dir) = peer_state("peer-refused-page");
        let mut events = state.subscribe();
        let refused = |args: Value| {
            let state = state.clone();
            async move {
                call(&state, "a", "show_page", &arguments(args))
                    .await
                    .expect_err("an invalid page")
                    .to_string()
            }
        };

        let message = refused(json!({ "body": [
            { "type": "section", "children": [
                { "type": "radio", "name": "c", "label": "C",
                  "options": [{ "value": "a" }, { "label": "no value" }] },
                ] },
            { "type": "submit" },
        ] }))
        .await;
        assert!(
            message.contains("`body[0].children[0].options[1]`"),
            "{message}"
        );
        assert!(
            message.contains("missing required property `value`"),
            "{message}"
        );
        assert!(
            message.contains("The schema of the `radio` component"),
            "{message}"
        );
        assert!(
            message.contains("\"additionalProperties\": false"),
            "{message}"
        );
        assert!(message.contains("\"options\""), "{message}");

        // A failure at the top is read against the page object's schema; the old `html` argument
        // is named as the unknown property it now is.
        let message = refused(json!({ "html": "<p>x</p>" })).await;
        assert!(message.contains("`html`: unknown property"), "{message}");
        assert!(
            message.contains("The schema of the page object"),
            "{message}"
        );
        let message = refused(json!({})).await;
        assert!(
            message.contains("missing required property `body`"),
            "{message}"
        );
        let message =
            refused(json!({ "body": [{ "type": "text", "name": "a", "label": "A" }] })).await;
        assert!(
            message.contains("needs a `submit` or `choices`"),
            "{message}"
        );

        let listed = call(&state, "a", "list_pages", &Map::new()).await.unwrap();
        assert_eq!(listed["pages"], json!([]));
        assert!(events.try_recv().is_err(), "nothing was announced");
    }

    /// `page_guide` answers each topic with its prose and schemas, and refuses one it has not.
    #[tokio::test]
    async fn the_page_guide_answers_by_topic() {
        let (state, _dir) = peer_state("peer-page-guide");
        let guide = |topic: &str| {
            let state = state.clone();
            let args = arguments(json!({ "topic": topic }));
            async move { call(&state, "a", "page_guide", &args).await }
        };
        let forms = guide("forms").await.unwrap();
        assert_eq!(forms["topic"], "forms");
        assert!(forms["guide"].as_str().unwrap().contains("# Forms"));
        assert!(forms["schemas"]["checkboxes"]["properties"]["options"].is_object());
        let pages = guide("pages").await.unwrap();
        assert!(pages["schemas"]["page"]["properties"]["body"].is_object());
        let charts = guide("charts").await.unwrap();
        assert!(charts["schemas"]["chart"]["properties"]["option"].is_object());
        let err = guide("maps").await.expect_err("no such topic");
        assert!(
            err.to_string()
                .contains("the topics are pages, forms, report-components, charts"),
            "{err}"
        );
        let err = call(&state, "a", "page_guide", &Map::new())
            .await
            .expect_err("no topic");
        assert!(err.to_string().contains("topic"), "{err}");
    }

    /// A submitted page is listed with its submission when it is read in full.
    #[tokio::test]
    async fn a_page_read_in_full_carries_its_submission() {
        let (state, _dir) = peer_state("peer-page-submission");
        let id = call(&state, "a", "show_page", &arguments(page("x")))
            .await
            .unwrap()["page"]
            .as_str()
            .unwrap()
            .to_string();
        let submission = json!({ "who": "Ada", "topics": ["a", "b"] });
        state
            .store
            .mark_page_submitted(&id, submission.as_object().unwrap())
            .unwrap();
        let read = call(&state, "a", "list_pages", &arguments(json!({ "page": id })))
            .await
            .unwrap();
        assert_eq!(read["submission"], submission);
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
                crate::coordinator::pages::withdraw_pages(state, session).unwrap();
            }));
        });
        let refused = call(&state, "starter", "show_page", &arguments(page("x"))).await;
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
    /// session does, a coordinator with a parent included, and a session bound to another has none
    /// of the three tools.
    #[tokio::test]
    async fn only_a_project_session_facing_the_user_has_the_page_tools() {
        let (state, _dir) = bound_coordinator_state("peer-project-pages");
        let push = |caller: &'static str, text: &'static str| {
            let state = state.clone();
            async move { call(&state, caller, "show_page", &arguments(page(text))).await }
        };
        // `starter` is a coordinator with a parent, `other-project` an unbound one.
        for caller in ["starter", "other-project"] {
            let id = push(caller, "mine").await.unwrap()["page"]
                .as_str()
                .unwrap()
                .to_string();
            let listed = call(&state, caller, "list_pages", &Map::new())
                .await
                .unwrap();
            assert_eq!(listed["pages"][0]["page"], id.as_str());
            let page = state.store.get_page(&id).unwrap().unwrap();
            assert_eq!(page.session_id, caller);
        }
        let err = push("mine", "x").await.expect_err("bound");
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
        // Pages are ordered by the millisecond they were pushed at, so each push waits for the
        // millisecond clock to move on: three pushed within a millisecond would tie and fall back
        // to their random ids.
        let push = |caller: &'static str| {
            let state = state.clone();
            async move {
                crate::test_support::next_millisecond().await;
                call(&state, caller, "show_page", &arguments(page("x")))
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
        let Event::PagesDeleted { session_id, pages } = events.try_recv().unwrap() else {
            panic!("the panels must be told");
        };
        assert_eq!(session_id, "starter");
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
            .set_binding("other-project", None, "starter", Role::Worker, true, false)
            .unwrap()
            .unwrap();
        let mut events = state.subscribe();

        let err = super::show_page(&state, &before_binding, &arguments(page("x")))
            .expect_err("bound by now");
        assert!(err.to_string().contains("role of a worker"), "{err}");
        assert!(state.store.list_pages("other-project").unwrap().is_empty());
        assert!(events.try_recv().is_err(), "no page was announced");

        // A coordinator with a parent is bound too, and keeps its pages.
        let (state, _dir) = bound_coordinator_state("peer-push-coordinator");
        let coordinator = state.session_record("starter").unwrap();
        super::show_page(&state, &coordinator, &arguments(page("x"))).unwrap();
        assert_eq!(state.store.list_pages("starter").unwrap().len(), 1);
    }

    /// At start the daemon withdraws the pages of every session with the role of a worker and of
    /// no other: those of a coordinator with a parent and of an unbound session stay.
    #[test]
    fn the_start_sweep_withdraws_the_pages_left_on_bound_sessions() {
        let (state, _dir) = bound_coordinator_state("peer-sweep-pages");
        // `starter` is a coordinator with a parent, `mine` is bound to it, `other-project` is
        // unbound.
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

        crate::coordinator::pages::withdraw_stray_pages(&state).unwrap();

        assert!(state.store.list_pages("mine").unwrap().is_empty());
        assert_eq!(state.store.list_pages("starter").unwrap().len(), 1);
        assert_eq!(state.store.list_pages("other-project").unwrap().len(), 1);
        let Event::PagesDeleted { session_id, pages } = events.try_recv().unwrap() else {
            panic!("the panels must be told");
        };
        assert_eq!(session_id, "mine");
        assert_eq!(pages.len(), 2);
        assert!(events.try_recv().is_err());

        // Nothing left to withdraw: nothing said.
        crate::coordinator::pages::withdraw_stray_pages(&state).unwrap();
        assert!(events.try_recv().is_err());
    }
}
