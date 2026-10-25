# Settings tools for agents

Octoboard's MCP server (see `docs/product/hub-orchestration.md`) lets a session read the settings the user configures
in the UI, and lets a console session change most of them. A change made through these tools is checked and refused
exactly as the same change made in the UI is, and reaches every open window the same way. Which role is offered which
tool is in the tool tables of `docs/product/hub-orchestration.md`, and the reads every role has are in
`docs/product/what-every-session-reads.md`; this document says what the settings tools do.

## Who reads and changes what

- **A console session** reads and changes its own console, that console's projects and the application-wide settings.
  It changes nothing of another console.
- **A project session**, bound or not, reads its own project's settings with `get_project`, which takes no argument and
  answers for no other project. It changes no setting.
- **Every session**, of either kind, reads the settings of every project of its own console with `list_projects`; of
  another console's projects it reads only each one's id, name, description and tags (see "Listing a console's
  projects" in `docs/product/what-every-session-reads.md`).

**One rule decides which settings the tools expose: an agent may help the user configure most of Octoboard, but not what
must be done by the user personally.** Every setting of a console, its projects and the application that the sections
below list is exposed under that rule. What must be done by the user personally is never available through these tools,
to any session:

- changing a project's directory, source, host or remote URL;
- giving or taking away a project's trust permission;
- removing a project;
- creating or deleting a console;
- setting or removing a console's avatar;
- reading, setting or clearing a console's console session instructions (see "Console session instructions" in
  `docs/product/consoles.md`);
- adding, editing or removing an account, or turning one off or back on;
- adding or removing a trusted folder;
- adding, removing or reordering agents (see "Added agents" and "Reordering and removing an agent" in
  `docs/product/agent-accounts.md`), since the list of added agents decides whether a new session of an agent may be
  opened, or a session handed off to it, at all, from the window or by an agent's tool: a session could otherwise add
  an agent the user removed or never added and start sessions on it, and which agents Octoboard runs, and in what
  order they are offered, is the user's own choice;
- reading or changing Claude Code's approval mode or Codex's approval reviewer (see "Approval modes" in
  `docs/product/agent-accounts.md`), since a session could otherwise lift the permission prompts of the sessions
  launched after it, its own next resume included, for instance by switching Claude Code to Approve everything;
- reading or changing whether session requests, resume requests, account switches or handoffs are approved
  automatically (see "Approving requests automatically" in `docs/product/session-requests.md`), since a session could
  otherwise approve its own requests;
- reading or changing whether usage windows are started automatically (see "Start usage windows automatically" in
  `docs/product/settings.md`), since that spends the user's quota.

Each of those stays the user's own, in the UI. Nor do the tools reach what a window keeps for itself rather than the
daemon, such as the appearance, Markdown style, language and keyboard shortcuts.

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
see "Adding, editing and removing an account" in `docs/product/agent-accounts.md`). An account of another agent is
refused as an unknown account.
Results always name an account by its id, and say `inherit` or `default` for the other states. The three states a
project's account can be in are in "The project's account, model and reasoning effort" in
`docs/product/projects.md`.

**`launch`** is an object keyed by agent, each value `{"model": …, "effort": …}`. In an argument, a field left out is
left as it is, and `null` or a blank string is the agent's own default. A model or effort that is being changed must be
one the agent offers, under the rules in "The Advanced section" in `docs/product/model-and-reasoning-effort.md`. Results
give every agent, with `null` for an unset value.

**The default agent and account settings do not decide what a session's own sessions run on.** The tools describe a
project's `default_agent` and `accounts`, a console's `default_agent`, `project_session_agent` and `accounts`, and what
`get_console` and `get_project` return, as applying to sessions opened without a choice. They say that a session a
project session starts itself — with `new_session`, or with `request_session` in its own console — runs on that
session's own agent unless it chooses another; that one a console session starts for a project — with `new_session` or
`request_session` — runs on the project's default agent; that a console session's request for a console session runs on
the requester's own agent; and that a left-out account is chosen as "The agent and account of a session a session
starts" in `docs/product/hub-orchestration.md` describes. They tell a console session to name another agent or account
only when the user asked for it.

## A project's settings

`list_projects` for the caller's own console, `get_project`, `add_project` and `update_project` give a project as the
same set of fields:

| Field | Contents |
|---|---|
| `project` | The project's id. |
| `name` | Its name. |
| `description` | What the project is for, as the user or the console session gave it (see "The project's description" in `docs/product/projects.md`), or `null` when it has none. |
| `host` | The host it is bound to. |
| `path` | Its directory. |
| `source` | How it was associated: `local`, `parent` or `git`. |
| `remote_url` | The recorded remote URL (see "The project's remote URL" in `docs/product/projects.md`), or `null`, always without any credential it carries. |
| `default_agent` | Its own default agent, or `null` when it follows the console's `project_session_agent`, else its `default_agent`. |
| `pinned` | Whether it is pinned. |
| `tags` | Its tags. |
| `trust_consent` | Whether the user has given the trust permission for it (see "The trust permission" in `docs/product/folder-trust.md`). Read-only. |
| `accounts` | Its account setting for every agent, as above. |
| `launch` | Its model and reasoning effort for every agent, as above; for a value it leaves unset, its sessions take its console's `project_session_launch` (see "The console's own settings: `get_console` and `update_console`" below). |
| `worktrees` | Its worktree settings (see `docs/product/worktree-settings.md`) as `{"override": …, "effective": …}`: under `override` the project's own value for each of `use_worktrees`, `rebase_before_merge`, `delivery` and `revert_gate`, `null` for one it leaves to the application-wide value; under `effective` the four resolved values for it now (`rebase_before_merge`, `delivery` and `revert_gate` apply only while `use_worktrees` is on). |

`list_projects` adds the sessions running in each project (see "Listing a console's projects" in
`docs/product/what-every-session-reads.md`).

**A credential is never shown.** For an `http`, `https` or `git` URL everything before the host's `@` is removed, so
`https://user:token@example.com/r.git` reads `https://example.com/r.git`. For an `ssh` URL only a password is removed:
the user in `ssh://git@example.com/r.git` names the remote and stays. A URL of any other shape is given as recorded.

## Adding a project with its settings

The console session's `add_project` associates projects from the same three sources as the user's form (see
"Associating a project" in `docs/product/projects.md`), and besides the location takes the settings the
form does: `name`, `description`, `default_agent`, `tags`, `accounts`, `launch`, `pinned` and `worktrees` (the
project's own worktree settings, as `update_project` takes them, except that an item left out or `null` follows the
application-wide value; see "A project's own worktree settings" in `docs/product/worktree-settings.md`). Every project
one call adds takes the same settings, except that `name` and `description` apply only when a single project is
associated (see "Project names" in `docs/product/projects.md`). A blank `name` or `description` counts as none given,
and so does a `description` of `null`. The call answers with each added project's settings.

Explicit text follows "Project names" and "The project's description" in `docs/product/projects.md`. Overlong text is
refused before anything is cloned or stored; text ignored for several parent-directory projects is not checked.

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
| `worktrees` | An object of any of `use_worktrees` (`true` or `false`), `rebase_before_merge` (`true` or `false`), `delivery` (`ask`, `merge` or `pull_request`) and `revert_gate` (`true` or `false`); each item given becomes the project's own value, `null` for an item puts it back to following the application-wide value, and an item left out stays as it is. |

`tags` beside `add_tags` or `remove_tags` is refused. Tags end up normalised as in `docs/product/project-tags.md`.
Anything left out is left as it is, and the call answers with the project's settings as stored.

Changed text follows the limits and unchanged-text compatibility rules in "Editing a project" in
`docs/product/projects.md`.

## The console's own settings: `get_console` and `update_console`

`get_console` answers with the calling console session's own console: `console` (its id), `name`, `default_agent` — the
default agent for console sessions — `project_session_agent` — the default agent for project sessions, `null` while it
follows `default_agent` (see `docs/product/consoles.md`) — `accounts`, `launch` — the model and reasoning effort of the
console's console sessions — `project_session_launch` — in the same shape, the model and reasoning effort a project
session takes for each value neither it nor its project sets — `has_icon`, whether it has an avatar, and `workdir`, its
working directory. Neither the avatar itself nor the console's console session instructions are ever returned.

`update_console` takes any of `name` (a blank one is refused), `default_agent` (an agent; it cannot be cleared),
`project_session_agent` (an agent, or `null` to follow `default_agent` again), `accounts`, `launch` and
`project_session_launch`, leaves the rest as it is, and answers as `get_console` does.

## Application settings: `get_settings` and `update_settings`

`get_settings` answers with:

| Field | Contents |
|---|---|
| `auto_pull_repositories` | Whether projects behind their upstream are pulled (fast-forwarded) on their own (see "Automatically pulling repositories" in `docs/product/project-git-status.md`). |
| `auto_delete_unresumable_archives` | Whether archived sessions whose conversation is gone are deleted on their own (see "Deleting archived sessions whose conversation is gone" in `docs/product/deleting-sessions.md`). |
| `auto_select_account` | Whether a new session whose account was not named uses the account of its agent whose weekly quota resets soonest (see "Automatically select an account" in `docs/product/settings.md` and "Which account a session uses" in `docs/product/sessions.md`). |
| `sidebar_order_by_status` | Whether the user's sidebar ranks sessions and projects by status (see "Order the sidebar by status" in `docs/product/settings.md` and "Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`). |
| `default_clone_dir` | The default clone directory (see "Default clone directory" in `docs/product/settings.md`). |
| `worktrees` | The application-wide worktree settings, as `use_worktrees`, `rebase_before_merge`, `delivery` and `revert_gate` (see "The four settings" in `docs/product/worktree-settings.md`); `rebase_before_merge`, `delivery` and `revert_gate` apply only while `use_worktrees` is on. |
| `accounts` | Every account, each with `id`, `agent`, `name`, `default` and `disabled`, which is true for an account the user turned off, so that no new session can be opened on it (see "Turning an account off" in `docs/product/agent-accounts.md`). Each agent's default account is listed first among that agent's, with the id `default` and, as its `name`, the name the user gave it, else `Default` (see "The default account" in `docs/product/agent-accounts.md`). An account's directory is never given. |
| `trusted_directories` | The trusted folders (see "Trusted folders" in `docs/product/folder-trust.md`). Read-only. |

`update_settings` takes `auto_pull_repositories`, `auto_delete_unresumable_archives`, `auto_select_account`,
`sidebar_order_by_status`, `default_clone_dir` and `worktrees`, under the rules of the same settings in Settings: the
switches take `true` or `false`, a clone directory must be absolute or start with `~/`, and a blank one goes back to the
built-in default. `worktrees` is an object of any of the four worktree settings, with the values `update_project`'s
takes (`rebase_before_merge`, `delivery` and `revert_gate` apply only while `use_worktrees` is on, and keep their values
while it is off); an item left out, or given as `null`, stays as it is. A refused value leaves the others unapplied too.
An `auto_approve_session_requests`, `auto_approve_resume_requests`, `auto_approve_account_switches`,
`auto_approve_handoffs` or `auto_start_quota_windows` argument is ignored — the setting stays as it is, `get_settings`
never lists any of them, and the rest of the call applies as usual. Turning `auto_pull_repositories` on fast-forwards
the projects already known to be behind at once, as turning the switch on in Settings does, which changes files in the
user's repositories. Turning `auto_delete_unresumable_archives` on deletes nothing at once, and turning it off stops a
deletion pass under way, exactly as the switch in Settings does. Turning `auto_select_account` on or off changes no
session already open; it is read when a later `new_session`, `request_session` or `handoff_session` leaves the account
out (see "Which account a session uses" in `docs/product/sessions.md`). The call answers as `get_settings` does.

## When a change takes effect

As for the same change in the UI: sessions already running keep what they were started with. A changed account
reaches sessions opened afterwards, and a changed model or effort reaches a session's next launch where the session
inherits that value (see `docs/product/model-and-reasoning-effort.md`). That holds for the calling console session
too: its own console's model and effort reach it when it is next launched, and its console's account only console
sessions opened afterwards. A changed `project_session_launch` reaches a project session at its next launch, unless its
project or the session sets that value itself. A changed `use_worktrees`, `rebase_before_merge` or `delivery`,
application-wide or a project's own, is sure to reach only project sessions begun after it (see "When a change reaches a session" in
`docs/product/worktree-settings.md`).

## Refusals

A refused call comes back as a tool error carrying the reason in prose and changes nothing, as for every Octoboard
tool. Besides the refusals above, an argument of the wrong type — a boolean, a list or a text given as something else —
is refused with a reason rather than read as left out. So is a `worktrees` object, for a project or application-wide,
that holds a key other than the four worktree settings or a `delivery` other than `ask`, `merge` and `pull_request`,
so that a misspelt item is not taken for a change that changed nothing.
