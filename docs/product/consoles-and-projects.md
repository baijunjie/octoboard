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
| Working directory | Octoboard | `~/.octoboard/consoles/<console id>/`, created when the console is created. Not settable and not changeable. |

Both agent fields take one of the three supported agents (Claude Code, Codex, Grok Build) and default to Claude Code
in the creation form.

Multiple consoles can exist side by side and are independent of each other.

### Editing a console

Only the name, the hub agent and the default agent can be changed. Changing an agent affects sessions opened
afterwards; a session that already exists keeps the agent it was started with.

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
  configuration and credentials.
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
