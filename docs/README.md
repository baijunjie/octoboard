# Documentation Index

Long-lived documentation under `docs/`. Development plan docs and bug tickets are temporary and are not listed here.

## Plan

- [MVP plan](mvp.md) — what Octoboard is, its architecture, the orchestration mechanism, the data model, and the scope of
  the MVP. Section 6 holds the per-agent injection mechanisms and the conditions each adapter must satisfy; section 13
  records what validation settled and what is still open.

## Product

- [Consoles and projects](product/consoles-and-projects.md) — consoles, their working directory, agent defaults and
  per-agent config directories; the three ways a project is associated, what is editable afterwards, and what deleting
  either one does.
- [Sessions](product/sessions.md) — hub and project sessions, the three-level menu, agent selection, the five session
  statuses and their transitions, the raised hand and its notification, archiving, interruption and resuming, and the
  terminal.
- [Hub orchestration](product/hub-orchestration.md) — what the hub can do: its tools and a project session's `report`,
  the brief a task is handed over as, the reporting loop and what happens when a session stops without reporting,
  automatic archiving, and which sessions the hub drives.
- [Report panel](product/report-panel.md) — the hub's third pane: pushing a page with `show_page` and what the page id is
  for, paging back through the kept history, why a history page is read-only and where that is enforced, what a page may
  contain and which outbound channels it has none of, the `octoboard.submit(data)` bridge and how a submission reaches the
  hub session, and the window's minimum size.
- [Launching agents](product/launching-agents.md) — the guarantee that project files and the user's agent configuration
  are never modified, the three things injected per launch and the hub's generated instruction file, the launch
  environment, and the per-agent specifics.
- [Application lifecycle](product/application-lifecycle.md) — what Octoboard runs on and how it is distributed, startup
  and the single-instance rule, losing the daemon connection, the quit confirmation and which gestures it covers, how to
  quit a window that has stopped responding, crash behaviour, and the files Octoboard keeps under `~/.octoboard`.

## Reference

- [Agent CLI reference](agent-cli-reference.md) — what the three agent CLIs themselves do, against the versions the
  facts were established on: each one's hook events, the payload fields and the keys a turn can be correlated on, what
  a failing hook costs, and how a project's own configuration layers around an injected one.

## Code

- [Project map](project-map.md) — navigation from the code tree to each module's own doc.

## Development memory

- [Probing agent CLIs](memory/probing-agent-clis.md) — how to establish what the three agent CLIs actually do, and how to
  probe a live Octoboard session that launches them: which session markers to strip and why enumerating them beats
  matching a prefix, why neither `--help` nor the binary's own strings can be trusted, how to settle a question without
  spending a model turn and where that stops being yours to decide, how to prove a per-launch injection and an MCP tool
  out of band rather than through the model, why a probe expecting "no" needs a positive control, and the residue a probe
  leaves — in the user's configuration and in their live `~/.octoboard` data.
- [Verifying the desktop UI](memory/verifying-the-desktop-ui.md) — how to verify terminal and UI behaviour in the real
  app: why a UI change has to be launched rather than only reviewed, how to get an error out of a blank window,
  ruling out a locked screen before trusting a capture, bisecting a symptom against the daemon, what a scripted GUI
  probe can and cannot prove, where to watch for a report page's blocked navigation, where a network probe's positive
  control has to come from, and which checks need a person.
- [Writing automated tests](memory/writing-automated-tests.md) — the fixture conventions this project's tests need on
  macOS, starting with why an executable written fresh per test flakes only under a parallel run.
