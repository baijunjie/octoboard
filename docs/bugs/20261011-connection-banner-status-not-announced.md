> Severity: Moderate

# The connection banner's status region is built the one way that is not announced

## Symptom

The strip that says the daemon connection is down carries its `role="status"` on an element rendered together with
the text already inside it — the one shape this project records as not reliably announced — so a screen reader
user is told nothing when the connection drops.

## Reproduction steps

1. Read `packages/ui/src/components/ConnectionBanner.tsx`: the `role="status"` sits on the strip, which renders
   only while the connection is `reconnecting` or `closed`, with its message as a child of that same element. The
   always-mounted wrapper around it carries no role.
2. Confirm nothing defers the first write: `StatusAnnouncer` (`packages/ui/src/components/StatusAnnouncer.tsx`),
   which exists for exactly this, is used by `GitView`, `ProjectBrowser`, `FileViewer` and `CodeSurface` only.
3. To hear it rather than read it: launch the packaged application with VoiceOver on, kill the daemon by PID, and
   listen for anything as the strip appears — including when the strip's own text changes from "Reconnecting…" to
   "Disconnected from the daemon."

## Expected vs. actual

- Expected: the connection dropping is announced. `docs/memory/writing-ui-components.md` states the rule and the
  reason: "a state that arises without the user acting (connection, a terminal problem) is announced from a
  `role="status"` element", and "a `role="status"` region meant to announce text that comes and goes stays mounted,
  empty when there is nothing to say, and only its contents change: one rendered conditionally, already holding its
  text when it is inserted, is not reliably announced (VoiceOver in WebKit among others)". The user confirmed on
  the spot (2026-10-11) that this is worth filing.
- Actual: the region is rendered conditionally with its text already in it, and no `StatusAnnouncer` defers the
  first write — the shape the rule rules out. What VoiceOver actually says is Unknown: step 3 was not run, as it
  needs a person listening.

## Environment

- `main` at `cc193ad`; read from the source, not heard.
- The banner is the same component on the main screen and on the startup screen that has no snapshot yet; on the
  main screen the component is always mounted and only the strip inside it comes and goes.
- It predates the stacking work of 2026-10-11, which left the region where it was; that work did add the
  always-mounted wrapper the rule asks for, so the gap is now one element wide.

## Scope of impact

- Every screen reader user, every time the daemon connection drops: the strip is the only notice, and the toast
  that accompanies a daemon exit is a separate element with its own role.
- Workaround: none for a listener. The text is reachable by moving the reading cursor to the banner region.

## Leads

- Verified: the always-mounted wrapper added by the stacking work (`data-react-aria-top-layer`) is a plain `div`
  with no role, and the strip inside it carries `role="status"`, `data-region="banner"` and the message together.
- Verified: `StatusAnnouncer` already solves this elsewhere and documents why a deferred first write is needed; its
  own doc comment says the visible form should then be `aria-hidden` and carry no role of its own, so moving the
  announcement is not just a matter of hoisting the attribute.
- Inferred: the fix is to announce the connection state through a `StatusAnnouncer` mounted for the window's
  lifetime and to drop `role="status"` from the strip. Not built.
- Unknown: whether the status is announced anyway on some combination of platform and screen reader, and whether
  the strip's second state change (reconnecting → disconnected) is announced when the first is not.

## Acceptance criteria

- [ ] With a screen reader, the connection dropping is announced as it happens, and so is the change from
      retrying to disconnected.
- [ ] Nothing is announced twice: the visible strip no longer carries a role of its own if the announcement moves.
- [ ] Nothing is announced while the connection is up, and a drop that is recovered before the announcement is
      written stays silent.
- [ ] The strip keeps its F6 region stop, its Retry's focus behaviour and its top-layer marking (see "What an open
      dialog, menu or popover covers" in `docs/product/window-layout.md`).
