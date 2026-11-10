# 01 Component pages with forms

> Goal: replace HTML report pages with component-tree pages rendered natively, carrying the components that asking the
> user a question needs, with a submission that is never sent twice.
> Completion criteria: a session pushes a page of Markdown, sections and a form through `show_page`; the panel renders
> it with the app's own components in both appearances; an invalid tree is refused with an error naming what is wrong
> and stores nothing; submitting locks the form, and the session gets exactly one message however often the user
> presses submit and from however many windows; a success leaves the page read-only showing the submitted values, in
> every window and after a reload; a failure that certainly did not deliver unlocks the form for a retry, and one that
> may have delivered does not; a page's content loads nothing from the network; pages stored as HTML before the upgrade
> are gone, no HTML rendering path remains, and no agent-facing text still describes pages as HTML. Verified by daemon
> tests for validation, storage and the one-submission rule (including two concurrent submissions and each kind of
> failure), and UI tests and gallery scenarios for rendering and each submission state.

## Technical design

- [ ] The page contract: a page is a JSON object with an optional `title` and a tree of components; each component
      names its kind and carries that kind's own properties. A schema defines it, and the protocol's page record
      carries it in place of `html`, together with the page's submission.
- [ ] Layout and content components: a section (with an optional heading), and a Markdown block in which raw HTML is
      not admitted, an image is shown only from a `data:` URL, and an external link opens in the system browser.
- [ ] Input components: the full set of form controls — single-line text, multi-line text, number, date and time,
      single choice, multiple choice, a drop-down, a toggle, a range — each with a field name, a label, an optional
      default, and the validation that applies to it (required, pattern, length, minimum and maximum).
- [ ] Submitting components: a submit button, and a group of choice buttons, each of which submits the form with one
      press and adds its field name and value to the submission.
- [ ] Conditional display: any component may be shown only while a named field has a given value, so a part of the page
      appears or disappears as the user picks; a hidden field is not submitted.
- [ ] `show_page` takes the component tree in place of `html` and validates it before storing; a refusal names the
      failing part of the tree and carries that part's schema. Its description is short: what a page is for, the rules
      it has today that an agent must know before pushing (binding or transfer withdraws the pages; only the newest page
      accepts a submission), and to read the guide tool before the first use.
- [ ] A guide tool returns the detailed usage and schema of the catalogue by topic (this milestone: pages and forms,
      including what a submission looks like); later milestones add their topics. It is offered to the same sessions as
      `show_page`, so not to workers.
- [ ] `list_pages` keeps `title` (from the page's `title`), returns the tree where it returned `html`, and replaces
      `html_bytes` with the tree's size.
- [ ] The console session's instructions and the coordinator role's description describe a page as a component tree
      read up in the guide tool, no longer as an HTML document.
- [ ] A page's submission is stored with the page: none, or the submitted fields. `submit_page` marks the page
      submitted before delivering and refuses a page already marked, as it refuses one that is not its session's
      newest; a delivery that certainly failed clears the mark, one whose acceptance is unconfirmed keeps it. The
      submission, and the clearing of its mark, reach every window.
- [ ] A submission still arrives in the session as a user message: a line naming the page, then one `key: value` line
      per field, with the field names the page gave and multiple choices joined as today.
- [ ] Stored pages: the store keeps the tree and the submission instead of HTML, and the upgrade discards every page
      stored as HTML.

## Implementation plan

- [ ] Rendering: each component maps to the app's own primitives; the Markdown block uses the app's existing Markdown
      rendering with raw HTML and remote images turned off.
- [ ] Submission flow: submit → client-side validation → form locked (pending) → `submit_page` → on success the page
      shows the stored submission read-only; on a certain failure the error is shown as today and the form unlocks;
      on an unconfirmed acceptance the error says the message may have arrived, and the page stays submitted.
- [ ] History pages stay read-only as today, now as a rendering state rather than disabling controls in a frame.
- [ ] Remove the HTML path: the in-frame sanitizer, the frame with its content security policy and document
      transform, the page bridge, the relays of Escape, F6 and the window shortcuts, the frame's right-click
      suppression, and the floating pane's pointer handling wired onto the frame — everything that existed only
      because the page lived in a frame.

## Notes for the developer

- **Reusable capabilities**: the app's Markdown renderer (GFM, math) and its UI primitives (buttons with a pending
  state, form controls); the daemon's tool catalogue schema helpers; the protocol records and the client types generated
  from them; the existing error surfacing for a failed submission and message delivery for writes into a session.
- **Development notes**: the panel, history paging, deletion, withdrawal on becoming a worker and the newest-page rule
  stay as they are; only what a page is and how it is shown and submitted change. Replacing `html` in the page record
  is a breaking change to the protocol, so it goes through the protocol's generation. Component strings Octoboard
  supplies (pending, submitted, read-only, the unconfirmed-delivery notice) go into the UI catalogues for every
  language. Drop the rules that a report page stays on a light surface and keeps its own language and writing direction.
- **Reference docs**: `docs/product/report-panel.md`, `docs/product/appearance.md`, `docs/product/language.md`,
  `docs/product/message-delivery.md`, `docs/ui-standard.md`, `apps/daemon/PROTOCOL.md`.
