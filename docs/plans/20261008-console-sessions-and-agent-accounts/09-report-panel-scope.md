# Report panel pages per console session

> Goal: a report page belongs to the console session that pushed it, not to the console, so two console sessions do
> not share one page history.
> Completion criteria: two console sessions in one console each push pages; selecting either shows only its own
> pages, with its own position and total, and a form submitted on one reaches that console session. Static checks and
> the test suite pass.

## Technical design

- [ ] A page records the console session that pushed it instead of the console it was pushed in.
- [ ] The panel shows the pages of the selected console session only. Its position-and-total count, its paging and
  its read-only badge are all per console session.
- [ ] "The newest page", which is the only page whose form may still be submitted, is the newest page **of that
  console session**.
- [ ] A form submission is delivered to the console session that pushed the page, found from the page itself. The
  lookup that found the console's one console session is gone, so there is no "this console has no console session"
  case left: a page always names its owner.
- [ ] Pages are deleted with the console session that pushed them, and so still with the console, which deletes its
  console sessions.

## Implementation plan

- [ ] Change the stored page record to name the console session, and change the queries that list pages and find the
  newest one to be scoped by it.
- [ ] Route a form submission by the page's owner, and drop the refusal that said the console had no console session.
- [ ] Scope the panel's state — which page is being viewed — per console session, so switching between two console
  sessions does not carry one's position over to the other.
- [ ] Extend deletion so that deleting a console session takes its pages.

## Notes for the developer

**Reusable capabilities**

- The panel's paging, its read-only badge and its isolation guarantees are unchanged; only the set of pages it is
  given changes.

**Development notes**

- The panel is on screen only while the selected session is a console session; that stays true, with "a console
  session" now meaning any of several.

**Reference docs**

- `docs/product/report-panel.md`, `docs/product/window-layout.md`
