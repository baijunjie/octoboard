# Documentation Index

Long-lived documentation under `docs/`. Development plan docs and bug tickets are temporary and are not listed here.

## Plan

- [MVP plan](mvp.md) — what Octoboard is, its architecture, the orchestration mechanism, the data model, and the scope of
  the MVP. Section 6 holds the per-agent injection mechanisms and the conditions each adapter must satisfy; section 13
  records what validation settled and what is still open.

## Product

- [Consoles and projects](product/consoles-and-projects.md) — consoles, their working directory and agent defaults; the
  three ways a project is associated, what is editable afterwards, and what deleting either one does.
- [Sessions](product/sessions.md) — hub and project sessions, the three-level menu, agent selection, the five session
  statuses and their transitions, archiving, interruption and resuming, and the terminal.
- [Launching agents](product/launching-agents.md) — the guarantee that project files and the user's agent configuration
  are never modified, what is injected per launch, the launch environment, and the per-agent specifics.
- [Application lifecycle](product/application-lifecycle.md) — startup and the single-instance rule, losing the daemon
  connection, the quit confirmation, crash behaviour, and the files Octoboard keeps under `~/.octoboard`.

## Reference

- [Agent CLI reference](agent-cli-reference.md) — what the three agent CLIs themselves do, against the versions the
  facts were established on: each one's hook events, the payload fields and the keys a turn can be correlated on, what
  a failing hook costs, and how a project's own configuration layers around an injected one.

## Code

- [Project map](project-map.md) — navigation from the code tree to each module's own doc.

## Development memory

- [Probing agent CLIs](memory/probing-agent-clis.md) — how to establish what the three agent CLIs actually do: which
  session markers to strip and why enumerating them beats matching a prefix, why neither `--help` nor the binary's own
  strings can be trusted, how to settle a question without spending a model turn and where that stops being yours to
  decide, why a probe expecting "no" needs a positive control, and the residue a probe leaves in the user's configuration.
- [Verifying the desktop UI](memory/verifying-the-desktop-ui.md) — how to verify terminal and UI behaviour in the real
  app: why a UI change has to be launched rather than only reviewed, how to get an error out of a blank window,
  bisecting a symptom against the daemon, what a scripted GUI probe can and cannot prove, and which checks need a
  person.
