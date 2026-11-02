//! The tools for the pages a session shows the user.

use serde_json::json;

use super::schemas::object_schema;
use super::ToolDef;

pub(super) const LIST_PAGES: ToolDef = ToolDef {
    name: "list_pages",
    description: "The pages this session has pushed with `show_page`, \
                  oldest first, read-only: each one's id (`page`), `created_at` \
                  (milliseconds since the epoch), `html_bytes` and `title` (its `<title>`; \
                  null when it has none). Only this session's own pages are listed. Name a \
                  `page` to read that one in full, `html` included. Use it to find the id of \
                  an earlier page, which tells a form submission apart from one of another \
                  page, or to build on what an earlier page showed.",
    schema: || {
        object_schema(
            json!({
                "page": {
                    "type": "string",
                    "description": "A page's id, as `show_page` returned it. Left out, \
                                    every page is listed without its HTML.",
                },
            }),
            &[],
        )
    },
};
pub(super) const SHOW_PAGE: ToolDef = ToolDef {
    name: "show_page",
    description: "Push a page to the report panel the user sees beside this session. Every page \
                  pushed is kept, and the user can page back through earlier ones, until this \
                  session is bound to a project session (`bind_to_session`, or handed to one with \
                  `transfer_session`): that withdraws them all. `html` is a static, self-contained \
                  HTML document: scripts and event handlers are removed and never run, and nothing \
                  external loads (no `<link>` elements, frames or external URLs), so inline any \
                  styles, stick to generic font families rather than a custom typeface, and give \
                  an image as a `data:` URL. Use it for anything better shown than typed into the \
                  terminal: a table, a comparison, a set of choices. A page may include a native \
                  HTML form: when the user submits it, its fields are sent back to this session as \
                  a message, each field's `name` becoming a key, so give every field you want back \
                  a `name`. For a choice use `<button name=\"choice\" value=\"...\">`; the pressed \
                  button's value comes back. `required` and `pattern` validation works. The call's \
                  result carries the pushed page's id; keep it, since it is how a later submission \
                  is told to have come from this page rather than one pushed after it \
                  (`list_pages` lists the ids again). Only the newest page accepts a submission: \
                  pushing a new page makes the previous page's form read-only, so push a fresh \
                  form rather than expecting an answer from an old one.",
    schema: || {
        object_schema(
            json!({
                "html": {
                    "type": "string",
                    "description": "The page's full HTML document.",
                },
            }),
            &["html"],
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
