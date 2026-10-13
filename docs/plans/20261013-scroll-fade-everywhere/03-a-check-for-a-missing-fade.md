# 03 A scroller with no fade fails a check

> Goal: a scroll container that arrives without a fade is caught by the repository's own checks, not by someone
> noticing it later.
> Completion criteria: adding a scroll container with no fade to the UI package fails one of the commands the
> project already runs for quality; the failure names the surface and says what to do about it; the check passes on
> the tree once milestones 01 and 02 are done, and a surface that is deliberately exempt is exempt in one named
> place rather than by the check not seeing it.

## Technical design

- [ ] The check's subject is a scroll container in the UI package: something that can hold more than it shows and
      scrolls rather than clipping.
- [ ] Taking the fade from the shared capability, in either of its forms, satisfies the check; a hand-built mask
      does not, since the convention is that the fade comes from one place.
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
- `docs/memory/writing-ui-components.md` — the convention being enforced.
