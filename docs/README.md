# Documentation Index

Long-lived documentation under `docs/`. Development plan docs and bug tickets are temporary and are not listed here.

## Product

- [Consoles and projects](product/consoles-and-projects.md) — consoles, their working directory, agent defaults and
  per-agent config directories; the three ways a project is associated, what is editable afterwards, and what deleting
  either one does.
- [Sessions](product/sessions.md) — hub and project sessions, the three-level menu, agent selection, the five session
  statuses and their transitions, the raised hand and its notification, how a declined Claude Code prompt lowers the
  hand from the agent's own transcript and what that costs to keep working, archiving, interruption and resuming, and
  the terminal.
- [Hub orchestration](product/hub-orchestration.md) — what the hub can do: its tools and a project session's `report`,
  the brief a task is handed over as, the reporting loop and what happens when a session stops without reporting,
  automatic archiving, and which sessions the hub drives.
- [Report panel](product/report-panel.md) — the hub's third pane: pushing a page with `show_page` and what the page id is
  for, paging back through the kept history, why a history page is read-only and where that is enforced, what a page may
  contain and which outbound channels it has none of, the `octoboard.submit(data)` bridge and how a submission reaches the
  hub session, and the window's minimum size.
- [Launching agents](product/launching-agents.md) — the guarantee that project files and the user's agent configuration
  are never modified, the three things injected per launch and the hub's generated instruction file, the launch
  environment, Claude Code's workspace-trust prompt, how Octoboard answers it and trusted folders, and the per-agent specifics.
- [Application lifecycle](product/application-lifecycle.md) — what Octoboard runs on and how it is distributed, startup
  and the single-instance rule, who can reach the daemon (any local program, but no web page in a browser), losing the
  daemon connection, the quit confirmation and which gestures it covers, how to quit a window that has stopped
  responding, crash behaviour, and the files Octoboard keeps under `~/.octoboard`.

## Reference

- [Architecture](architecture.md) — what Octoboard is and its core concepts, the layers and why the daemon is split from
  the application, the technology choices, the orchestration design decisions (the MCP server as a child process,
  structured arguments, why reporting is not forced), the known pitfalls of the Tauri / Rust approach, and the reasoning
  behind the data model.
- [Agent CLI reference](agent-cli-reference.md) — what the three agent CLIs themselves do, against the versions the
  facts were established on: each one's hook events, the payload fields and the keys a turn can be correlated on, what
  a failing hook costs, how a project's own configuration layers around an injected one, how Octoboard is injected into
  each agent and the conditions that come with it, and the rules for writing into a running session.

## Code

- [Project map](project-map.md) — the monorepo layout (the daemon at the top level, clients under `apps/`, shared code
  under `packages/`) and the workspace root's common commands, then navigation from the code tree to each module's own
  doc.

## Development memory

- [Probing agent CLIs](memory/probing-agent-clis.md) — how to establish what the three agent CLIs actually do, and how to
  probe a live Octoboard session that launches them: which session markers to strip and why enumerating them beats
  matching a prefix, why neither `--help`, the binary's own strings, nor a config file's silent acceptance of a
  capability's name can be trusted, how to settle a question without spending a model turn and where that stops being
  yours to decide, how to prove a per-launch injection and an MCP tool out of band rather than through the model, why a
  probe expecting "no" needs a positive control and why a measured silence also has to be bounded by what would have
  ended it, why a permission probe has to be set up against the user's own settings with the mode in effect confirmed
  from the session itself, and the residue a probe leaves — in the user's configuration and in their live
  `~/.octoboard` data — and how a probe that needs no logged-in agent avoids the latter with a throwaway
  `HOME`/`TMPDIR`, using Codex's sign-in screen as session output.
- [Verifying the desktop UI](memory/verifying-the-desktop-ui.md) — how to verify terminal and UI behaviour in the real
  app: why a UI change has to be launched rather than only reviewed, why a daemon-side change needs the daemon built
  and the running sidecar's binary confirmed before anything read off the window means anything, how to get an error
  out of a blank window, reading what the packaged webview sends to the daemon through a wrapped sidecar (dev mode
  sends a different origin), ruling out a locked screen before trusting a capture, bisecting a symptom against the daemon,
  what a scripted GUI probe can and cannot prove and why its setup should go through the daemon's protocol instead
  (including raising a session's hand with a forged hook event), where to watch for a report page's blocked
  navigation, where a network probe's positive control has to come from, why another worktree's dev server or daemon
  may be the one answering and why yours are stopped by PID, and which checks need a person.
- [Writing UI components](memory/writing-ui-components.md) — conventions for `packages/ui` components: why a HeroUI
  control pressed with the mouse takes keyboard focus off the terminal, when `preventFocusOnPress` is needed, and why
  "⋯" menus are built on `ActionMenu`.
- [Writing automated tests](memory/writing-automated-tests.md) — the fixture conventions this project's tests need on
  macOS: why an executable written fresh per test flakes only under a parallel run, and how to verify behaviour the
  daemon derives from an agent's own output by replaying a committed capture rather than staging a live session.
