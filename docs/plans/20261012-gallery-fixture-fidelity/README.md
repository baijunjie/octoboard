# Gallery fixture fidelity

## Problem

The gallery's fixture daemon is a second implementation of the daemon's reply contract, written by hand and filled
from whatever each fixture happens to carry. Nothing holds it to what the real daemon would send, so a fixture that
is only partly filled in produces a reply the daemon never sends, and the gallery then shows a UI state that cannot
occur in the app. A defect looked at there is read as the app's when it is the fixture's.

This has now happened twice over one field. The change that gave a diff reply its sides' line counts taught the
fixture daemon the same condition and filled the fixtures reached through the change factories and through the main
worktree's change list; three changes written as hand-written literals in the two less-visited lists — the second
worktree's, and the branch comparison's — were left unfilled. Their diffs went on ending in a bar promising
unmodified lines they do not have, which is exactly the defect that change had just fixed. Nothing caught it: an
unfilled count is a legal value of the reply, so the type check, the tests and the renderer all passed it through.

Writing the rule down does not hold it. The memory entry stating that a fixture answers as fully as the real daemon
was written by the very change that then left three fixtures unfilled.

## Plan outline

Two independent changes, each closing the gap from a different side.

- **The fixture daemon refuses to answer a reply it cannot fill faithfully**, rather than sending a combination the
  real daemon never produces. A fixture missing what a reply needs then fails where it is used, visibly, instead of
  shifting the UI into an impossible state. This is the general net: it holds for the next field as well as this one.
- **A change fixture is written through a factory per kind of change**, each taking exactly what that kind needs, so
  an incomplete text change cannot be expressed at all and the type check rejects it.

They can land in either order and are verified separately.

## Key design decisions

- **Loud failure over a silent fallback.** The cost being avoided is a plausible-looking wrong state, not a crash; a
  fixture gap is a bug in the gallery's own data and should read as one.
- **A refusal is not a daemon error.** The gallery already has scenarios whose fixture answers a request with a
  daemon error, and those are a legitimate UI state under test. A fixture that cannot be answered must therefore fail
  in a way that is distinguishable from a daemon error at a glance, not be dressed as one.
- **Compile time where the types can say it, runtime where they cannot.** The factories decide what a fixture must
  carry; the fixture daemon's refusal covers what no factory signature can settle.
- **Scope is the Git mode's change fixtures.** The other fixture kinds are untouched by this topic.

## Open

- Whether the same refusal is worth extending to the other fixture kinds (project files, the viewer's own subjects).
- Whether to later derive one invariant check from the reply rules stated in the daemon's protocol document and run
  it over every scenario's replies, so that a new field is covered by adding one invariant rather than one guard test
  per field. Deliberately left out of this topic as the most expensive of the three options; worth revisiting if this
  class of defect appears again.

## Milestones

- 01 refuse-an-unfillable-reply (closed)
- 02 [A change fixture is written through a factory per kind](02-a-factory-per-kind-of-change.md)
