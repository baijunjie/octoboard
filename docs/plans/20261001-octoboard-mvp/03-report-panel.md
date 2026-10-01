# 03 Report Panel

> Goal: the report panel appears to the right of the hub session, able to receive pushed HTML pages, page through history,
> and submit forms back to the hub.
> Done when: the panel refreshes to the newest page after the hub calls `show_page`; history pages can be paged back and
> forth and are read-only; the hub continues correctly after a form submission.

## Technical design

- [ ] `Page { id, console_id, html, anchor_message_id, created_at }`; every page is archived, and `anchor_message_id` is kept
  as a field for now with no rewind linkage
- [ ] Rendering: a sandboxed iframe (`srcdoc`, separate origin, CSP restricting outbound requests)
- [ ] Bridge: only `octoboard.submit(data)` is exposed, passed out via `postMessage`

## Implementation

- [ ] `show_page(html)` archives and pushes the page, and the panel refreshes to the newest one; the panel is shown only in
  the hub session
- [ ] History paging (◀ ▶); forms and `submit` are disabled on history pages
- [ ] The data passed to `submit` is sent to the hub session as a user message, annotated with its source page; while the hub
  is busy, the queued delivery from 02 applies

## Notes for developers

- **Reusable from earlier**: the queued message delivery and the `show_page` tool channel from 02.
- **Reference**: `docs/mvp.md` section 7.
