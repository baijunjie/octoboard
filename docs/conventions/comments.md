# Comment conventions

The project-specific rules for code comments and for what committed text may refer to, beyond the `agent-docs` skill's
"Comment conventions".

## General principles

- None beyond [P1](principles.md): comment density follows the surrounding code, and a change updates the comments it
  made stale.

## Content

- **Nothing that holds only on one machine goes into a committed comment or doc**: local paths, environment values,
  install locations. Write paths relative to the repository root and an environment variable's name, not its value.

## What a comment may refer to

- **A comment never refers to `docs/memory/`**, whose entries are loose: anything needed from one is written into the
  comment.
- **A development plan or a bug ticket is referred to only while it exists, marked `TODO`**; code and tests never
  depend on one, a lasting explanation goes into the comment or a long-lived doc, and commit messages and PR
  descriptions never refer to a plan or a ticket, since they outlive it and cannot be updated.
- **File names and section headings of every doc a comment may refer to stay stable, because comments refer to them**;
  renaming one means updating the referring comments in the same change.

## TODO

- **A `TODO` waiting on an upstream project cites its issue as `TODO(org/repo#n, <url>)`**
  (`apps/daemon/src/adapter/codex/mod.rs`).
- **A `TODO` waiting on an upstream release with no issue to cite is written `TODO(<project> <release>)`.** Every `TODO`
  says what it waits for: an upstream issue, an upstream release, a plan or ticket as the rule above allows, or the
  restore condition of a suspended test case (`docs/conventions/tests.md`, "What a test asserts"). A `TODO` that waits
  for nothing scheduled is not allowed, since nothing will ever act on it.
- **A deliberately left transitional layer is marked `TODO`, naming the milestone doc that removes it** as the
  `agent-plan-exec` skill's "Boundaries" sets out; the milestone that closes deletes the `TODO` with the layer.
