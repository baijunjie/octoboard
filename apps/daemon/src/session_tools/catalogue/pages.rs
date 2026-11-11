//! The tools for the pages a session shows the user.

use serde_json::json;

use crate::session_tools::page_guide::TOPICS;

use super::schemas::object_schema;
use super::ToolDef;

pub(super) const LIST_PAGES: ToolDef = ToolDef {
    name: "list_pages",
    description: "The pages this session has pushed with `show_page`, \
                  oldest first, read-only: each one's id (`page`), `created_at` \
                  (milliseconds since the epoch), `content_bytes` (the size of its content as \
                  JSON) and `title` (null when it has none). Only this session's own pages are \
                  listed. Name a `page` to read that one in full: its `content`, the component \
                  tree as it was pushed, and its `submission`, what its form was submitted with \
                  (null until it is). Use it to find the id of an earlier page, which tells a \
                  form submission apart from one of another page, or to build on what an earlier \
                  page showed.",
    schema: || {
        object_schema(
            json!({
                "page": {
                    "type": "string",
                    "description": "A page's id, as `show_page` returned it. Left out, \
                                    every page is listed without its content.",
                },
            }),
            &[],
        )
    },
};

pub(super) const SHOW_PAGE: ToolDef = ToolDef {
    name: "show_page",
    description: "Push a page to the report panel the user sees beside this session, for \
                  anything better shown than typed into the terminal: a table, a comparison, a \
                  set of choices, a question with a form. A page is a tree of components, \
                  `body`, with an optional `title`; the app draws it, so there is no HTML or \
                  styling to write. Read `page_guide` before the first use: it has the \
                  components and their schemas. A page that does not match them is refused with \
                  the location of the problem and stores nothing. Every page pushed is kept, and \
                  the user can page back through earlier ones, until this session is bound to a \
                  project session (`bind_to_session`, or handed to one with `transfer_session`): \
                  that withdraws them all. Only the newest page accepts a form submission, and \
                  only once: the fields come back to this session as a message. The call's \
                  result carries the pushed page's id (`list_pages` lists the ids again).",
    schema: || {
        object_schema(
            json!({
                "title": {
                    "type": "string",
                    "minLength": 1,
                    "description": "The page's title.",
                },
                "body": {
                    "type": "array",
                    "minItems": 1,
                    "description": "The page's components, top to bottom. Each is an object \
                                    whose `type` names its kind; see `page_guide`.",
                    "items": { "type": "object" },
                },
            }),
            &["body"],
        )
    },
};

pub(super) const PAGE_GUIDE: ToolDef = ToolDef {
    name: "page_guide",
    description: "How to build a page for `show_page`, by topic: `pages` (the page object, \
                  sections, Markdown, conditional display, what renders how), `forms` \
                  (input fields, validation, submit buttons, the one-submission rule and what a \
                  submission message looks like), `report-components` (columns, tabs, \
                  callouts, key-value lists, stats, code, diffs, copyable text, sortable \
                  tables), and `charts` (the chart component and the part of an ECharts option \
                  it takes). Answers with the topic's guide and the JSON \
                  Schema of each kind of component it covers. Read it before the first page \
                  you push.",
    schema: || {
        let topics: Vec<&str> = TOPICS.iter().map(|topic| topic.name).collect();
        let described: Vec<String> = TOPICS
            .iter()
            .map(|topic| format!("`{}`: {}", topic.name, topic.summary))
            .collect();
        object_schema(
            json!({
                "topic": {
                    "type": "string",
                    "enum": topics,
                    "description": described.join("; "),
                },
            }),
            &["topic"],
        )
    },
};

pub(super) const DELETE_PAGES: ToolDef = ToolDef {
    name: "delete_pages",
    description: "Withdraw pages this session pushed with `show_page` from its report panel, for \
                  good: the user's panel drops them at once. When the last one goes, a \
                  project session's Report panel and its toggle go with it, and a console \
                  session's panel shows its empty state. Name them in `pages` (ids, as \
                  `show_page` and `list_pages` give them), or pass `all: true` for every page \
                  this session pushed; exactly one of the two. Only this session's own pages \
                  can be deleted: an id that is not one of them is refused, the same way as an \
                  id that names no page, and then nothing is deleted. Deleting the newest page \
                  makes the one before it the newest, the one a form submission is accepted \
                  from. Answers with the `deleted` ids.",
    schema: || {
        object_schema(
            json!({
                "pages": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "The ids of the pages to delete.",
                },
                "all": {
                    "type": "boolean",
                    "description": "True to delete every page this session pushed. Not with \
                                    `pages`.",
                },
            }),
            &[],
        )
    },
};
