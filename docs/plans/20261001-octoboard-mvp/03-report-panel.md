# 03 Report Panel

> Goal: the report panel appears to the right of the hub session, able to receive pushed HTML pages, page through history,
> and submit forms back to the hub.
> Completion criteria: the panel refreshes to the newest page after the hub calls `show_page`; history pages can be paged back and
> forth and are read-only; the hub continues correctly after a form submission.

## Handoff

- **`show_page` is the one hub tool the catalogue does not yet carry.** The MCP server, the per-role tool lists and the
  daemon-side execution all exist; adding a tool means an entry in the catalogue and an arm in the dispatch, and the role
  description names the tools from the catalogue itself so it follows along.

## Technical design

- [ ] `Page { id, console_id, html, anchor_message_id, created_at }`; every page is archived, and `anchor_message_id` is kept
  as a field for now with no rewind linkage
- [ ] Rendering: a sandboxed iframe (`srcdoc`, separate origin, CSP restricting outbound requests)
- [ ] Bridge: only `octoboard.submit(data)` is exposed, passed out via `postMessage`

## Implementation

- [ ] `show_page(html)` archives and pushes the page, and the panel refreshes to the newest one; the panel is shown only in
  the hub session
- [ ] History paging (◀ ▶); forms and `submit` are disabled on history pages
- [ ] The data passed to `submit` is sent to the hub session as a user message, annotated with its source page, through the
  same path every other write into a running session takes — delivered straight away while the hub is mid-turn or idle, and
  queued only while it is waiting for the user

## Notes for developers

- **Reusable from earlier**: the write queue every message into a running session goes through, and the MCP tool
  catalogue a new tool is added to.
- **Reference**: `docs/mvp.md` section 7.
