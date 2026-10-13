# 03 A scroller with no fade fails a check

> Goal: a scroll container that arrives without a fade is caught by the repository's own checks, not by someone
> noticing it later.
> Completion criteria: adding a scroll container with no fade to the UI package fails one of the commands the
> project already runs for quality; the failure names the surface and says what to do about it; the check passes on
> the tree once milestones 01 and 02 are done, and a surface that is deliberately exempt is exempt in one named
> place rather than by the check not seeing it.

## Handoff

From 01, which gave the fade to every scroller it owns. What is left over is scrollers this package does
not make itself, which the check has to either cover or exempt in the one named place it keeps
exemptions — a scroller the check simply cannot see is not exempt, it is invisible, and the criteria
require the check to pass on the tree:

- **HeroUI's own popovers scroll, and the fade can never go on them.** `.dropdown__popover`
  (`overflow-y-auto`, every menu and submenu `ActionMenu` renders) and `.select__popover`
  (`overflow-y-auto`, every `Select`: the worktree selector, the account and option selects, and the
  branch comparison's own selector) are scroll containers on elements this package renders and passes
  `className` to, with react-aria capping their height. Each is the element that animates away, so by
  the overview's key design decision the fade cannot go on it: it would have to go on a scroller inside,
  as the branch list inside its popover now does. Whether every such popover gains an inner scroller or
  they are exempt as a class is a decision this milestone has to make and record — it reaches every menu
  and select in the application, so it is the user's call rather than one to settle in passing.
- **The library's tab strips fade, but not like anything else here.** A HeroUI `Tabs.ListContainer`
  wraps its strip in a `ScrollShadow` of its own, over 64 px, with chevrons at an end that has more
  beyond it and no scroll padding at all — so a tab reached with the arrow keys lands under the fade.
  Two strips here have such a container: the settings dialog's column of sections and the Git mode's
  Uncommitted / Compare row. (The project browser's Files / Git row has no container, so it neither
  fades nor scrolls; giving it one would be a change, not a fix.) These are faded scrollers the check
  will see, over a size no convention here chose. Decide with the popovers above whether they are left
  as the library draws them or brought to the application's own size, and say so where the exemptions
  are listed. The settings dialog's column is the easier of the two: measured in the running gallery, it
  cannot overflow at all — `docked:self-start` leaves its wrapper at its content height, so the
  library's scroller inside it has `clientHeight == scrollHeight` at every window height and the fade
  never engages. So what that column needs from this milestone is a declared exemption, not a size.
- **One scroller scrolls sideways only**: the window's main column (`overflow-x-auto`, dropped at the
  docked breakpoint). The rule here is vertical, so it is an exemption to declare rather than a
  scroller to fix — unless the overview's open question on horizontal overflow is answered otherwise
  first.
- **Two scrollers belong to something else.** xterm's own `.xterm-viewport` inside the terminal pane
  (the wrapper this package renders is `overflow-hidden`; the scrollback viewport is xterm's DOM), and
  the report page, which is a document the console session's model wrote inside an `<iframe srcDoc>` and
  scrolls as a document. Neither has an element this package can put the fade on, so both are exemptions
  to declare with their reason rather than scrollers to fix.

## Technical design

- [ ] The check's subject is a scroll container in the UI package: something that can hold more than it shows and
      scrolls rather than clipping.
- [ ] Taking the fade from the shared capability, in either of its shapes, satisfies the check; a hand-built mask does
      not, and nor does HeroUI's `ScrollShadow` reached for directly, since 01a left one module as the only way in.
- [ ] An exemption is declared, carries its reason, and is listed where the rule is written rather than scattered at
      the sites.
- [ ] The check runs in a command the project already runs, so nobody has to remember a new one.

## Implementation plan

- [ ] Settle what the check reads — the source, the built styles, or the rendered result — against what can see a
      scroll container reliably and what cannot be fooled by a class name that happens to match.
- [ ] Give the check a failure that names the surface and points at the convention.
- [ ] Prove the check by breaking the rule: a scroll container added without a fade fails it, and the same container
      with the fade passes.
- [ ] The convention in development memory points at the check, so a reader learns the rule is enforced rather than
      merely written.

## Notes for the developer

**Reusable capabilities**

- The repository already runs type checking, unit tests and a build over the UI package; one of these is where the
  check belongs.
- Development memory already holds the convention this check enforces, including why the fade comes from one place.

**Development notes**

- A check of this kind is worth only what its own failure case proves; the project's testing convention is to break
  the rule a test is named for and watch it fail.
- Static checks in this repository do not look at line width or at comments, so a check that reads source has to be
  explicit about what it can and cannot see.

**Reference docs**

- `docs/memory/writing-automated-tests.md` — the testing conventions, including proving a test by breaking its rule.
- `docs/memory/fading-the-edge-of-a-scroller.md` — the convention being enforced.
