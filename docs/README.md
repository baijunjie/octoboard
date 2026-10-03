# Documentation Index

Long-lived documentation under `docs/`. Development plan docs and bug tickets are temporary and are not listed here.

## Plan

- [MVP plan](mvp.md) — what Octoboard is, its architecture, the orchestration mechanism, the data model, and the scope of
  the MVP. Section 6 holds the per-agent injection mechanisms and the conditions each adapter must satisfy; section 13
  records what validation settled and what is still open.

## Code

- [Project map](project-map.md) — navigation from the code tree to each module's own doc.

## Development memory

- [Probing agent CLIs](memory/probing-agent-clis.md) — how to establish what the three agent CLIs actually do: which
  session markers to strip and why enumerating them beats matching a prefix, why neither `--help` nor the binary's own
  strings can be trusted, how to settle a question without spending a model turn and where that stops being yours to
  decide, why a probe expecting "no" needs a positive control, and the residue a probe leaves in the user's configuration.
- [Verifying the desktop UI](memory/verifying-the-desktop-ui.md) — how to verify terminal and UI behaviour in the real
  app: bisecting a symptom against the daemon, what a scripted GUI probe can and cannot prove, and which checks need a
  person.
