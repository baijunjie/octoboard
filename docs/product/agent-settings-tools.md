# Settings tools for agents

Octoboard's MCP server (see `docs/product/hub-orchestration.md`) lets a session read the settings the user configures
in the UI, and lets a console session change most of them. A change made through these tools is checked and refused
exactly as the same change made in the UI is, and reaches every open window the same way. Which role is offered which
tool is in the tool tables of `docs/product/hub-orchestration.md`; this document says what the settings tools do.

## Who reads and changes what

- **A console session** reads and changes its own console, that console's projects and the application-wide settings.
  Another console, and another console's projects, are out of its reach.
- **A project session**, bound or not, reads its own project's settings with `get_project`, which takes no argument and
  answers for no other project. It changes no setting.

Never through these tools, by any session: changing a project's directory, source, host or remote URL; giving or
taking away a project's trust permission; removing a project; creating or deleting a console; setting or removing a
console's avatar; reading, setting or clearing a console's console session instructions (see "Console session
instructions" in `docs/product/consoles-and-projects.md`); adding, editing or removing an account; adding or removing
a trusted folder. Each of those stays the user's own, in the UI.

The console session's instructions tell it that projects, its console and the application settings belong to the
user: it changes one only when asked to, says what it changed, and treats accounts and trusted folders as read-only.

## Accounts and launch settings in arguments and results

Two shapes are shared by every tool that reads or sets an account or a model and reasoning effort.

**`accounts`** is an object keyed by agent (`claude`, `codex`, `grok`). In an argument, an agent left out is left as it
is. Each value is:

- for a project: `inherit` (the console's reference for that agent), `default` (the agent's default account), or an
  account of that agent by id or by name;
- for a console: `default` or `null` (the agent's default account), or an account of that agent by id or by name.

A name is looked up among that agent's own accounts, compared as account names are (trimmed and ignoring letter case,
see "Agent accounts" in `docs/product/settings.md`). An account of another agent is refused as an unknown account.
Results always name an account by its id, and say `inherit` or `default` for the other states. The three states a
project's account can be in are in "The project's account, model and reasoning effort" in
`docs/product/consoles-and-projects.md`.

**`launch`** is an object keyed by agent, each value `{"model": …, "effort": …}`. In an argument, a field left out is
left as it is, and `null` or a blank string is the agent's own default. A model or effort that is being changed must be
one the agent offers, under the rules in "The Advanced section" in `docs/product/model-and-reasoning-effort.md`. Results
give every agent, with `null` for an unset value.

## A project's settings

`list_projects`, `get_project`, `add_project` and `update_project` give a project as the same set of fields:

| Field | Contents |
|---|---|
| `project` | The project's id. |
| `name` | Its name. |
| `description` | What the project is for, as the user or the console session gave it (see "The project's description" in `docs/product/consoles-and-projects.md`), or `null` when it has none. |
| `host` | The host it is bound to. |
| `path` | Its directory. |
| `source` | How it was associated: `local`, `parent` or `git`. |
| `remote_url` | The recorded remote URL (see "The project's remote URL" in `docs/product/consoles-and-projects.md`), or `null`, always without any credential it carries. |
| `default_agent` | Its own default agent, or `null` when it follows the console's `project_session_agent`, else its `default_agent`. |
| `pinned` | Whether it is pinned. |
| `tags` | Its tags. |
| `trust_consent` | Whether the user has given the trust permission for it (see "The trust permission" in `docs/product/folder-trust.md`). Read-only. |
| `accounts` | Its account setting for every agent, as above. |
| `launch` | Its model and reasoning effort for every agent, as above; for a value it leaves unset, its sessions take its console's `project_session_launch` (see "The console's own settings: `get_console` and `update_console`" below). |

`list_projects` adds the sessions running in each project (see "The console session's tools" in
`docs/product/hub-orchestration.md`).

**A credential is never shown.** For an `http`, `https` or `git` URL everything before the host's `@` is removed, so
`https://user:token@example.com/r.git` reads `https://example.com/r.git`. For an `ssh` URL only a password is removed:
the user in `ssh://git@example.com/r.git` names the remote and stays. A URL of any other shape is given as recorded.

## Adding a project with its settings

The console session's `add_project` associates projects from the same three sources as the user's form (see
"Associating a project" in `docs/product/consoles-and-projects.md`), and besides the location takes the settings the
form does: `name`, `description`, `default_agent`, `tags`, `accounts`, `launch` and `pinned`. Every project one call
adds takes the same settings, except that `name` and `description` apply only when a single project is associated (see
"Project names" in `docs/product/consoles-and-projects.md`). A blank `name` or `description` counts as none given, and
so does a `description` of `null`. The call answers with each added project's settings.

A `git` association's `remote_url` is held to a stricter rule than the form's:

- it must be a URL a configuration document may be cloned from (see "What may be cloned from" in
  `docs/product/configuration-export-and-import.md`);
- it must carry no credential, in the sense of the section above. The clone uses the user's own git credentials.

Either refusal is a tool error naming the URL without its credential, and nothing is cloned.

## Changing a project: `update_project`

Takes `project` — an id, or a name that names exactly one project of the console — and any of:

| Argument | Effect |
|---|---|
| `name` | The new name. A blank one is refused. |
| `description` | The new description, stored trimmed; `null` or a blank string removes it. |
| `tags` | Replaces the whole tag list; an empty list removes every tag. |
| `add_tags` | Adds tags to those the project has. |
| `remove_tags` | Removes tags, matched ignoring letter case. |
| `pinned` | Pins or unpins it. |
| `default_agent` | An agent, or `null` to follow the console's again (its `project_session_agent`, else its `default_agent`). |
| `accounts`, `launch` | As above. |

`tags` beside `add_tags` or `remove_tags` is refused. Tags end up normalised as in "Tags" in
`docs/product/consoles-and-projects.md`. Anything left out is left as it is, and the call answers with the project's
settings as stored.

## The console's own settings: `get_console` and `update_console`

`get_console` answers with the calling console session's own console: `console` (its id), `name`, `default_agent` —
the default agent for console sessions — `project_session_agent` — the default agent for project sessions, `null` while
it follows `default_agent` (see "Consoles" in `docs/product/consoles-and-projects.md`) — `accounts`, `launch` — the
model and reasoning effort of the console's console sessions — `project_session_launch` — in the same shape, the model
and reasoning effort a project session takes for each value neither it nor its project sets — `has_icon`, whether it
has an avatar, and `workdir`, its working directory. Neither the avatar itself nor the console's console session instructions are ever returned.

`update_console` takes any of `name` (a blank one is refused), `default_agent` (an agent; it cannot be cleared),
`project_session_agent` (an agent, or `null` to follow `default_agent` again), `accounts`, `launch` and
`project_session_launch`, leaves the rest as it is, and answers as `get_console` does.

## Application settings: `get_settings` and `update_settings`

`get_settings` answers with:

| Field | Contents |
|---|---|
| `auto_pull_repositories` | Whether projects behind their upstream are pulled (fast-forwarded) on their own (see "Automatically pulling repositories" in `docs/product/project-git-status.md`). |
| `default_clone_dir` | The default clone directory (see "Default clone directory" in `docs/product/settings.md`). |
| `accounts` | Every account, each with `id`, `agent`, `name` and `default`. Each agent's default account is listed first among that agent's, with the id `default`. An account's directory is never given. |
| `trusted_directories` | The trusted folders (see "Trusted folders" in `docs/product/folder-trust.md`). Read-only. |

`update_settings` takes `auto_pull_repositories` and `default_clone_dir`, under the rules of the same settings in
Settings: a clone directory must be absolute or start with `~/`, and a blank one goes back to the built-in default.
A refused value leaves the other one unapplied too. Turning `auto_pull_repositories` on fast-forwards the projects
already known to be behind at once, as turning the switch on in Settings does, which changes files in the user's
repositories. The call answers as `get_settings` does.

## When a change takes effect

As for the same change in the UI: sessions already running keep what they were started with. A changed account
reaches sessions opened afterwards, and a changed model or effort reaches a session's next launch where the session
inherits that value (see `docs/product/model-and-reasoning-effort.md`). That holds for the calling console session
too: its own console's model and effort reach it when it is next launched, and its console's account only console
sessions opened afterwards. A changed `project_session_launch` reaches a project session at its next launch, unless its
project or the session sets that value itself.

## Refusals

A refused call comes back as a tool error carrying the reason in prose and changes nothing, as for every Octoboard
tool. Besides the refusals above, an argument of the wrong type — a boolean, a list or a text given as something else —
is refused with a reason rather than read as left out.
