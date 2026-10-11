# A change fixture is written through a factory per kind

> Goal: an incomplete text change cannot be written into the Git mode's change fixtures, because the only way to
> write one requires what it must carry.
> Completion criteria: every change in the sample repository's lists is built through a factory; omitting a text
> change's versions is a type error, demonstrated once by a deliberate omission that the type check rejects; the
> gallery's scenarios show exactly what they showed before, and the package's type check and tests stay clean.

## Technical design

- [ ] One factory per kind of change the fixtures express, each taking exactly what that kind needs: a text change
      takes both versions whole, and derives from them what the reply counts.
- [ ] The kinds to cover are the ones the fixtures already hold: a text change, a rename, a change with a side the
      daemon will not deliver, a binary change, a type change, an untracked file, a deletion, a side outside the
      project, and a change in conflict.
- [ ] A factory signature admits no half-filled shape: what a kind does not have — a count for a side read from the
      worktree disk, say — is absent by the kind's definition rather than by an omission at the call site.

## Implementation plan

- [ ] Take the existing change factories as the starting point and extend the set until no change in the sample
      repository's lists is written as a bare literal.
- [ ] Convert the hand-written entries list by list, including the second worktree's and the branch comparison's,
      which are the ones a change reaches last.
- [ ] Keep each scenario showing what it shows today: this milestone changes how the fixtures are written, not what
      the gallery presents.

## Notes for the developer

**Reusable capabilities**

- The Git mode's change fixtures already build their long files and the patches between them from one generator, and
  already have factories for an expandable change and for a rename that nests; those are the pattern to extend.
- The guard test over the sample repository's changes covers the conversion: it names every change it checked, so a
  change dropped or reshaped by mistake fails it.

**Development notes**

- A patch written by hand is what several fixtures exist to show (a rename's headers, a type change's two sections, a
  missing newline marker); a factory must keep those expressible rather than generating every patch.
- The gallery's scenarios select changes by their path and wording, so renaming or reordering a fixture reaches the
  scenario steps as well.

**Reference docs**

- `docs/product/project-pane-git-mode.md`, "What it shows" — the kinds of change a diff has to present.
- `apps/daemon/PROTOCOL.md`, "Changes and comparisons" — what each kind's reply carries.
