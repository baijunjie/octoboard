# Consoles and projects

A **console** groups a set of projects and owns a working directory of its own. A **project** is a directory on a
host, associated with one console. A console is created and managed by the user; a project is associated either by the
user or by that console's hub session. Sessions that run inside them are described in
`docs/product/sessions.md`.

## Consoles

A console carries:

| Field | Set by | Notes |
|---|---|---|
| Name | the user, required | Free text. |
| Hub agent | the user | The agent the console's own hub session runs. |
| Default agent | the user | The fallback agent for sessions opened under this console's projects. |
| Agent config directories | the user, optional | One per agent: where that agent's sessions in this console keep their configuration; see "Agent config directories" below. |
| Working directory | Octoboard | `~/.octoboard/consoles/<console id>/`, created when the console is created. Not settable and not changeable. |

Both agent fields take one of the three supported agents (Claude Code, Codex, Grok Build) and default to Claude Code
in the creation form.

Multiple consoles can exist side by side and are independent of each other.

### Agent config directories

A console can hold up to three optional directories, one per agent. Each is the directory that agent keeps its own
configuration, login state and conversation history in — for a user who keeps a second setup of an agent beside the
default one, such as `~/.claude-alt`:

| Agent | What the directory is | The agent's usual default |
|---|---|---|
| Claude Code | what Claude Code itself takes from `CLAUDE_CONFIG_DIR` | `~/.claude` |
| Codex | what Codex itself takes from `CODEX_HOME` | `~/.codex` |
| Grok Build | the Grok home, what Grok itself takes from `GROK_HOME` | `~/.grok` |

How each agent is pointed at its directory is in "The launch environment" and "Per-agent specifics a user will notice"
in `docs/product/launching-agents.md`.

The console dialog shows one input for each agent currently selected as the console's hub agent or default agent —
one row or two, in the order Claude Code, Codex, Grok Build — labelled with that agent's name and marked optional, with
the agent's usual default as its placeholder. A directory can therefore be entered only for an agent selected in one
of those two fields. A directory already stored for an agent that is not currently selected is kept as it is: it is
neither shown nor saved by the dialog, and it still applies to that agent's sessions, such as those of a project whose
own default agent it is. To see or clear it, select that agent as the hub or default agent again.

- **Left blank**, it is unset: that agent's sessions use whatever the user's login shell exports for the agent's
  variable, and the agent's own default when it exports none.
- **When set**, it applies to every session of that agent opened in the console afterwards — the hub and project
  sessions alike, including a session whose agent was chosen for that session alone — and takes precedence over the
  value in the user's shell environment (see "The launch environment" in `docs/product/launching-agents.md`). A session
  reads only its own agent's directory; sessions of the other agents are unaffected.

For Claude Code, entering `~/.claude` is not the same as leaving the field blank: whenever a directory is set, Claude
Code reads its global config file from `.claude.json` inside that directory instead of from `~/.claude.json`.

For Grok Build, the directory must already be a Grok home that Grok has been run against; Octoboard does not check
this. Why, and what happens otherwise, is under "Grok Build" in "Per-agent specifics a user will notice" in
`docs/product/launching-agents.md`.

Each value is checked when the console is saved, by the same rules for all three agents, and a failure is shown in the
dialog, naming the agent, with nothing saved:

- surrounding whitespace is trimmed, and a blank value means unset;
- a leading `~` or `~/` is expanded to the home directory of the host the daemon runs on;
- the result must be an absolute path;
- it is normalized lexically — a trailing `/`, `.` components and `..` components are resolved; a symlink is kept as
  typed;
- it must exist and be a directory. Octoboard never creates it.

What is stored, and shown when the console is edited, is the expanded absolute path.

**A session keeps the directory it was opened with.** A session takes the console's value for its own agent when it is
opened and keeps it for its lifetime; resuming or reopening it relaunches with that same directory, because the agent
keeps the conversation inside it. A session opened while its agent's setting was unset holds no directory and resumes
under whatever the user's shell exports at that moment.

If a session's directory no longer exists when it is launched or resumed, the launch is refused rather than started —
the agent would otherwise come up logged out, without the conversation a resume is meant to continue. This holds for
all three agents, and the message names the agent and the directory and says to recreate it or to clear it in the
console's settings. For sessions opened afterwards, correcting or clearing the console's setting resolves it; an
existing session can be resumed again only once the directory exists at that path again.

### Editing a console

The name, the hub agent, the default agent and the config directories the dialog shows can be changed, and a config
directory can be cleared. Changing an agent or a config directory affects sessions opened afterwards; a session that
already exists keeps the agent and the config directory it was started with.

A config directory is checked only when it is changed: saving a console with that field untouched succeeds even if the
stored directory has since disappeared.

### Deleting a console

Deleting a console is **refused while any of its sessions still has a running process** — those must be archived (or
the application restarted, which interrupts them) first. The error names the reason.

When it goes through, deleting a console also deletes every project association under it, every session record
belonging to it, archived sessions included, and every page its hub pushed to the report panel, and removes the
console's own working directory under `~/.octoboard`. No project directory is touched.

## Projects

A project carries a name, the directory it points at, an optional default agent, how it was associated, and — for a
GitHub association — the remote URL it was cloned from. Every project is bound to a host (see "Hosts" below).

### Associating a project

There are three sources:

| Source | Input | Result |
|---|---|---|
| A single directory | a path | That one directory becomes a project. The path must exist and be a directory. |
| A parent directory | a path | Every git repository **directly beneath** that path becomes its own project. Only one level down is scanned; a checkout nested deeper belongs to the repository above it. Associating nothing is an error: if no git repository is found directly beneath the path, the request is refused and names the directory. |
| A GitHub URL | a repository URL plus a parent directory | The repository is cloned into a new directory beneath the parent, and the clone is then associated. |

A path may be entered by hand or picked with the directory browser. A leading `~` is expanded to the home directory
of the host the daemon runs on.

The console's hub session can associate a project itself, from the same three sources and under all the
rules in this section (see "The hub's tools" in `docs/product/hub-orchestration.md`).

For a GitHub association:

- The target directory name is taken from the URL's last segment with any `.git` suffix removed. Both
  `https://host/owner/repo(.git)` and `git@host:owner/repo(.git)` are understood.
- If that directory already exists, the request is refused rather than cloning into or over it.
- The clone runs with the user's own shell environment, so it uses the `git` on the user's `PATH` and their git
  configuration and credentials. Capturing that environment is bounded the same way a session launch's is, and a
  capture that does not complete refuses the clone (see "The launch environment" in
  `docs/product/launching-agents.md`).
- A clone can take minutes; other state updates keep flowing while it runs.

A directory that is already associated with the same console is skipped rather than associated twice. Associating a
parent directory again after new repositories have appeared beneath it therefore adds only the new ones. If every
directory found is already associated, the request is refused and says so.

### Project names

A name can be given only when a single project is being associated; it then overrides the derived name. Otherwise, and
for every project a parent-directory association produces, the name is the directory's own name.

### The project's default agent

A project may carry its own default agent, or leave it unset to inherit the console's. The selection order for a new
session is in "Which agent a session uses" in `docs/product/sessions.md`.

### Editing a project

Only the name and the default agent can be changed. The association itself — the source, the directory, the remote
URL — is fixed once the project exists; a project that should point somewhere else is removed and associated again.

The name cannot be cleared: saving an empty name is rejected. The default agent can be cleared, which puts the project
back to inheriting the console's default.

### Removing a project

Removing a project **never touches the directory or anything in it**. It removes the association, and with it the
records of that project's sessions, archived ones included.

It is refused while any session of that project still has a running process.

## Browsing directories

The directory browser lists directories only — a project is a directory — and hides entries whose name begins with a
dot. Each entry is flagged with whether it is a git repository. Entries are sorted case-insensitively by name. The
listing always comes from the host the projects live on, never from the application's own filesystem view.

A listing can fail, and that is a normal path rather than an edge case: a packaged application is granted file access
per volume by macOS, and the user may decline the prompt or leave it unanswered. The failure is reported with the path
and with the hint that per-volume access has to be granted; whatever was listed before stays on screen so the browser
can be navigated back out of. The same failure can surface when a session is opened: a project directory the daemon
cannot reach is reported as such rather than silently starting the agent somewhere else.

## Hosts

Every project and every session is bound to a host. Only one host exists, the local machine, listed as
"This machine". Hosts cannot be added or edited.
