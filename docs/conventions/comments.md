# Comment conventions

The project-specific rules for code comments and for what committed text may refer to. The general rules (what a comment
explains and what it does not, the level it sits at, how a doc is referenced) are in the `agent-docs` skill,
"Comment conventions"; they are not repeated here.

## Language and content

- **Comments, like all committed prose, are in English.** Only the content of internationalization resources stays in
  its target language (`CLAUDE.md`, "Language").
- **Nothing that holds only on one machine goes into a committed comment or doc**: local paths, environment values,
  install locations. Write paths relative to the repository root and an environment variable's name, not its value.
- **Write new code like the code around it**: naming, structure, comment density and error handling follow local habit.
  A change clears out the dead code it produced and updates the comments it made stale.

## What a comment may refer to

- **A comment may refer to** any long-lived doc registered in `docs/README.md` (`docs/product/`, `docs/structure/`,
  `docs/conventions/`, `docs/architecture.md`, `docs/agent-cli-reference.md`, `docs/voiceover-cases.md` and the like), a
  module `README.md`, `apps/daemon/PROTOCOL.md`, and external standards or upstream issues. It never refers to
  `docs/memory/`, whose entries are loose: anything needed from it is written into the comment.
- **A temporary doc (a development plan, a bug ticket) is referred to only while it exists, marked `TODO`**, as the
  `agent-docs` skill's "Referencing docs" sets out. Beyond it: code and tests never depend on one, a lasting explanation
  is written into the comment or a long-lived doc, and commit messages and PR descriptions never refer to a plan or a
  ticket, since they outlive it and cannot be updated.
- **File names and section headings of every doc a comment may refer to stay stable, because comments refer to them**;
  renaming one means updating the referring comments in the same change.

## TODO

- **A `TODO` waiting on an upstream project cites its issue as `TODO(org/repo#n, <url>)`**
  (`apps/daemon/src/adapter/codex/mod.rs`).
- **A `TODO` waiting on an upstream release with no issue to cite is written `TODO(<project> <release>)`.** Every `TODO`
  says what it waits for: an upstream issue, an upstream release, a plan or ticket as the temporary-doc rule above
  allows, or the restore condition of a suspended test case (`docs/conventions/tests.md`, "What a test asserts"). A
  `TODO` that waits for nothing scheduled is not allowed, since nothing will ever act on it.
- **A deliberately left transitional layer is marked `TODO`, naming the milestone doc that removes it** as the
  `agent-plan-exec` skill's "Boundaries" sets out; the milestone that closes deletes the `TODO` with the layer.
