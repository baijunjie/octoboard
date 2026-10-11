# The fixture daemon refuses an unfillable reply

> Goal: a fixture that does not carry what a reply needs fails where it is used, instead of being answered with a
> combination the real daemon never sends.
> Completion criteria: a change fixture whose patch has hunks and whose two sides are both readable files, but which
> carries no whole text, makes the scenario that opens it fail visibly and makes a unit test fail; every existing
> scenario is unaffected and the package's type check and tests stay clean.

## Technical design

- [ ] State the condition the fixture daemon holds itself to for a change reply: where the real daemon would count a
      side, the fixture must carry what the count is taken from, and where it does not, the reply is refused rather
      than sent with the field unfilled.
- [ ] Give the refusal a shape that cannot be mistaken for a daemon error, so the scenarios that deliberately answer
      with a daemon error keep reading as the UI states they are.
- [ ] Write the condition next to the code that holds it, as the reason it exists — a reply the daemon never sends is
      worse than no reply, because it is believed.

## Implementation plan

- [ ] Fail at the point where a reply's side is built, so that every request kind answering with sides is covered by
      one condition rather than one per request.
- [ ] Keep the fixture daemon's existing allowances explicit, each with its reason: a side read from the worktree
      disk with no body carries no count, a link or a submodule carries none, and a patch with no hunk counts
      neither side. These are what the real daemon does, not gaps.
- [ ] Extend the existing guard test over the sample repository's changes so the refusal is exercised, not only the
      filled fixtures.

## Notes for the developer

**Reusable capabilities**

- The gallery's fixture daemon already holds the condition under which a side is counted; this milestone turns that
  condition from a fallback into a refusal rather than introducing a new one.
- The guard test over the sample repository's changes already walks every change fixture and names what it covered;
  reuse it rather than adding a second walk.

**Development notes**

- The gallery is the surface used to look at a UI state before staging it through a daemon, so a refusal must be
  legible to whoever opens the scenario, not only to a test run.
- Keep the fixture daemon free of dependencies on the viewer's own helpers where it stands in for the daemon's own
  counting; the duplication there is deliberate and documented.

**Reference docs**

- `apps/daemon/PROTOCOL.md`, "Changes and comparisons" — what a real reply carries, and under which conditions.
- `docs/product/project-pane-git-mode.md`, "Expanding the collapsed lines" — the behaviour the counts drive.
- `docs/memory/verifying-the-desktop-ui.md` — the entry on a fixture daemon answering as fully as the real one.
