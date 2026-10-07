# Console sessions and agent accounts

Two reworks in one topic, because they rewrite the same surfaces and only one order of them avoids rework.

- **Several console sessions per console.** A console may hold any number of orchestrating sessions at once, each
  with its own name, colour and its own set of sessions, instead of the single one a console is limited to today.
- **Named agent accounts.** A user who keeps more than one login of the same agent — a personal Claude Code
  subscription and a work one, say — can name each of them in Settings, pick one when a session is opened, and move a
  running session from one to another by hand when its usage has run out.

They meet in three places: the vocabulary for the orchestrating session, the session record and its schema, and the
sidebar row the orchestrating session is reached through. Done in the wrong order each of the three is written twice.
The milestone numbering below is the order that avoids that; see "Why this order" at the end of the milestone list.

## Problem

### One orchestrating session per console

Today a console has at most one orchestrating session, and that limit is not a UI convention but the shape of the
data. A project session's membership in the orchestration is a boolean, and the session it reports to is found by
looking the console's one orchestrating session up. A live second one is refused by the daemon, and the application
suppresses every request it knows would be refused.

Two things follow that the user does not want:

- A console cannot run two orchestrations side by side. One orchestrating session per console means one line of work
  per console, where the user wants several, each focused on one dispatching task.
- Because the orchestrating session is a singleton, everything around it is scoped to the console rather than to the
  session: its archive is reached from the console's single row, and the report panel's pages belong to the console.
  With several sessions those scopes are wrong — this is what surfaced the problem, as an archive entry sitting in
  the wrong menu.

The terminology has the same defect. Documentation and code say "the hub" and "the console's hub" throughout, as of
one thing; and `hub session` is named for what the session does while `project session` is named for what it belongs
to, so a reader reasonably infers a container called a Hub, which does not exist.

### One unnamed config directory per console per agent

An agent's login lives in its config directory, and Octoboard already lets a console pin one directory per agent.
That mechanism is enough to run a second setup of an agent, but not enough to live with several:

- A directory is pinned per console, as a free-text path, with no name. A user with two logins has to retype the
  path in every console that should use it, and nothing on screen says which login a session is running under.
- The choice is made in the console's settings, away from where the session is opened, and a session cannot be
  opened under a different login than the console's.
- The choice is fixed for a session's lifetime. When a login's usage runs out mid-task there is nothing to do but
  open a new session elsewhere and lose the conversation.

Octoboard also assumes throughout that the user has a usable agent. When none is installed, a session can still be
opened and fails at launch with a message about a binary not being on the `PATH`, which says nothing about what to
do.

## Outline

### Console sessions

- **Terminology, settled first.** The orchestrating session is a **console session**: it belongs to a console, as a
  **project session** belongs to a project. Both are named on one axis — what they belong to. The word "hub"
  survives only as the default title a new console session is given (`Hub 1`, `Hub 2`, …), which the user may
  rename; it names no type, no identifier and nothing in prose.
- **Binding replaces membership.** A project session carries the id of the console session it is bound to, or none.
  Reports are routed along that binding. The one-live-console-session rule, its claim machinery and every
  pre-emptive refusal around it are deleted.
- **Everything scoped to the console session, not the console.** Report panel pages, the archive, the focus mode and
  the colour badge all hang off the console session.
- **Isolation is on writes, not on reads.** A console session may read every session in its console, each marked with
  which console session owns it, and may act only on the ones bound to it.
- **Two focus modes.** A console session's focus mode shows only the projects and sessions bound to it; a project's
  focus mode shows only that project's unbound sessions.

### Agent accounts

An **account** is a named config directory of one agent — the directory that agent keeps its login and its
conversation history in. Accounts are kept once for the whole application, in Settings, and referred to everywhere
a config directory is referred to today.

- **On every start** Octoboard works out which agents are available — their binaries resolve or they do not — and
  what each agent's default account resolves to, so there is always something to pick without the user configuring
  anything.
- **Settings** gains an Agent accounts section: the accounts of each agent, with adding, editing and removing.
- **The console** no longer takes a path per agent; it picks one of that agent's accounts instead.
- **Opening a session** offers the agent and the account in one control: a single grouped list whose groups are the
  agents and whose entries are that agent's accounts. Picking an entry settles both at once, so no combination that
  does not exist can be chosen.
- **With no agent available** no session can be opened at all, and the user is told to install one instead of being
  let through to a launch failure.
- **A running session** can be moved to another account of the same agent from its action menu, when its agent has
  more than one. The session's conversation is relocated into the account's config directory first, so the move
  resumes the conversation rather than starting a new one.

## Key design decisions

### Console sessions

**The binding is by session id and is set when the session is created.** Naming the console session rather than
asking "does this console have one" means archiving or interrupting a console session does not break the link:
reopening it restores delivery by itself. Whether a binding may later be changed is left open (below).

**`origin` and the binding stay separate.** Who created a session (the user, or a console session) and which console
session it is bound to are two facts. A session a console session starts is always bound to it; a session the user
starts by hand may be bound to one as well.

**Writes are isolated, reads are not.** Two console sessions can dispatch work into the same project, and therefore
into the same working directory, at the same time. A console session that cannot see the other's sessions would
collide with them. It is told who owns what and told to leave it alone — the treatment an unbound session already
gets today, so the rule is not new, only its scope.

**Archiving a console session goes by whether a process is running, not by status.** While any session bound to it
still has a process running, archiving is refused and the refusal names them; with none running, archiving the
console session archives its dormant bound sessions with it. Reopening a bound session reopens its console session
first. This last part is only possible once the one-live rule is gone: today such a reopen would be refused.

**Deleting an archived console session deletes its archived bound sessions.** The mirror of cascading archive: were
they left behind, reopening one would have no console session to reopen with it, and its binding would dangle.

**Colours come from a fixed palette, not from random values.** A console session is assigned one on creation, from
the colours not already in use in that console, wrapping round only when they are exhausted. Each palette entry
carries a light and a dark variant so the badge meets contrast either way. The badge carries no information on its
own — the console session's name is in its tooltip.

**The database is rewritten rather than migrated.** The application has not shipped, so the schema is changed in
place and existing local data is discarded. This decision governs the accounts milestones too; see "Where the two
meet" below.

### Agent accounts

**Accounts replace the console's config directory fields rather than layering over them.** Two ways to name the same
directory would mean two precedence chains for the user to hold in their head, and a path typed into a console could
not be shown by name anywhere.

**An account is application-wide, not per console.** A login belongs to the machine Octoboard runs on, not to one
grouping of projects, and the whole point is to use the same login from several consoles without retyping it.

**Every agent has a default account, and it is the state of pinning nothing.** Today a session with no directory
pinned launches exactly as the user's own shell would have it — a state no screen can name. The default account
names it rather than replacing it: it pins nothing, it is Octoboard's own entry rather than a directory the user
typed, its name is Octoboard's own word for it and is not editable, and it is shown with the directory it resolves
to so the user can see where the session runs.

**Pinning nothing is not the same as pinning the agent's own default**, which is why the default account must not
become a path. Setting `CLAUDE_CONFIG_DIR` to `~/.claude` makes Claude Code read its global config file from
`.claude.json` inside that directory instead of from `~/.claude.json`, and Octoboard's own trust warning reads that
same file. A default account that pinned `~/.claude` would move every existing user's Claude Code onto a config file
with no trust decisions and no history in it. For the same reason Octoboard never offers the agent's own default
directory as a *second*, pinned account: the default account already covers it.

**What "pinning nothing" means at launch differs by agent**, and the difference is the existing one rather than a new
one. For Claude Code and Codex Octoboard sets no variable, so whatever the user's shell exports stands, and the
agent's own default applies when it exports nothing. For Grok Build, `GROK_HOME` is set on every launch regardless,
to the per-session home Octoboard builds; what an account pins is the **source home** that home is built from, so
the default account means no source home is pinned and the source is whatever the user's shell exports, else
`~/.grok`.

**An agent is available when its binary resolves on the user's login shell `PATH`, and nothing else is checked.**
Whether a config directory holds a login is not Octoboard's business: a user may authenticate with an API key from
their shell, with a credential helper, or through an organization's gateway, and in none of those cases does the
config directory show a login. Measured against Claude Code 2.1.289: a fresh config directory is created by the
agent itself and carries no login, so the directory's existence says nothing either way.

**Each login is scoped to its config directory**, which is what makes the whole feature work. Measured; see
[the relocation findings](agent-relocation-findings.md).

**An account's directory is not checked for existence when it is set.** Whatever directory the user names is handed
to the agent as its variable, and the agent creates the directory on first run — measured for Claude Code, and
inferred for Codex from its keeping everything it needs inside that directory. A user pointing an account at a
directory they are about to create should not be stopped.

**Grok Build is the exception, and it is caught at launch rather than at creation.** A Grok config directory must
already be a Grok home that Grok has been run against: Octoboard builds a per-session home that links only entries
already present in the source home, so for an empty directory Grok writes its login and its conversation into the
per-session home, which is discarded with the process — neither persists. That is exactly the silent failure a
switch must not produce, so a Grok session whose account directory is not an initialized Grok home is **refused at
launch**, on the same path that already refuses a config directory that has disappeared. Creation still does not
check, so the rule lives in one place and the user is told when it matters.

**The one other place existence matters is resuming a conversation.** Octoboard refuses today to launch a session
whose config directory has disappeared, because the agent would come up logged out without the conversation the
resume was meant to continue — an outcome that looks like success. That refusal is kept, narrowed to what it
protects: a session that has a conversation on the agent's side. Opening a new session, and resuming one that never
had a conversation, never refuse on this account.

**No usage-limit detection.** Whether a usage limit even produces a machine-readable signal is unestablished, and
the only candidate — matching the raw HTTP status and response body a Claude Code `StopFailure` hook carries — is
documented to be rewritten by the agent on upgrade. A wrong guess would move a session's login without the user
asking. The switch is therefore the user's action alone.

**Switching accounts relocates the conversation.** All three agents keep a session's record inside the config
directory, so a resume under a different directory would not find the conversation. The switch copies the session's
own conversation record into the target directory before relaunching. Measured against the installed versions: that
record alone is enough for all three, so all three support the switch; see
[the relocation findings](agent-relocation-findings.md). This is the one point where Octoboard writes into a
directory the user owns, and it is narrow by construction: one conversation record of one session, added, with no
settings file of the agent's touched. Two promises in `docs/product/launching-agents.md` have to be reworded to say
so — that the user's own agent configuration is never written to, and that Octoboard never copies an agent's
conversation history.

**A switch moves the session to the target account's whole setup, not only its login.** A config directory holds the
agent's global configuration too, so a setting the user keeps in one account's directory does not follow the session
into another's. This is a consequence to document rather than to work around — the directory is the agent's own
notion of a setup.

**The switch is offered only when there is somewhere to switch to** — that is, when the session's agent has more
than one account. With one account the entry is hidden rather than shown and disabled.

**A project keeps its default agent and gains no account of its own.** An account is a login, and a project is not
the boundary a login belongs to — a console is, which is why the console carries one account per agent. A user who
wants a set of projects on the work login groups them under a console. The resolution chain is therefore the
session's choice, else the console's account for that agent, else the agent's default account, with the project
contributing the agent alone as it does today.

**An account's name is required and unique within its agent**, compared trimmed and ignoring letter case, so that
the grouped picker never shows two entries a user cannot tell apart. Names are not compared across agents, and the
default account's name takes part in the comparison like any other. Where Octoboard names an account itself — the
console dialog still saves a path, with no name field to ask with, until milestone 7 replaces it — it uses the
directory's last path component, and when that is already taken for that agent it uses the whole directory instead,
since a path is always distinct.

**The Agent accounts section says nothing about which accounts are in use.** A count that changes while the dialog
is open is noise, and nothing a user would do differently for it: an account can be removed whether or not sessions
hold it, because a session launches from the directory it recorded.

### Where the two meet

**No migration of stored data anywhere in this plan.** Milestone 2 rewrites the schema and discards local data;
every milestone after it therefore inherits an empty database as its starting point. The accounts work consequently
does **not** migrate a console's pinned paths into accounts — by the time it runs there are no consoles left holding
paths. A user's own setup is re-entered by hand after milestone 2, which is a smaller cost than migration code
written for data this plan has already agreed to discard.

**A console session holds an account exactly as a project session does.** The account is resolved and recorded when
a session is opened, so several console sessions in one console may each sit on a different account, and each can be
switched independently. The console still carries one account per agent as the default they resolve from.

**Both reworks name things in the same vocabulary.** The accounts work is written and built in the vocabulary
milestone 1 settles. Nothing it adds says "hub".

## Milestones

1. 01 Terminology: console sessions and project sessions (closed)
2. [The binding, and the end of the one-live rule](02-binding-data-model.md)
3. [The sidebar's console sessions](03-sidebar.md)
4. [Accounts: storage and protocol](04-accounts-storage.md)
5. [Agent availability and the default account](05-availability-and-the-default-account.md)
6. [The Agent accounts section in Settings](06-settings-section.md)
7. [Choosing an account where an agent is chosen](07-account-pickers.md)
8. [Switching a session's account](08-switching-a-sessions-account.md)
9. [Report panel pages per console session](09-report-panel-scope.md)
10. [What a console session may see and touch](10-tool-surface.md)
11. [Archiving, reopening and deleting along the binding](11-archive-cascade.md)
12. [Choosing a binding when a session is created](12-binding-selector.md)
13. [The two focus modes](13-focus-modes.md)

[The relocation findings](agent-relocation-findings.md) are a reference rather than a milestone: they are already
measured, and milestone 8 is what acts on them.

### Why this order

Milestones 1 to 3 come first because each one removes a place where the two reworks would otherwise collide:

- **1 settles the vocabulary**, so the accounts work is written once, in the final words, instead of being renamed
  afterwards.
- **2 settles the schema and the no-migration premise**, so the accounts work adds its columns to the final shape and
  writes no migration.
- **3 removes the single row** the orchestrating session is reached through today. Milestone 8 puts a Switch account
  entry in a console session's menu; with 3 done first, that menu is the one it will keep.

Milestones 4 to 8 are then the accounts work in full, before the rest of the console-session work, because nothing in
9 to 13 blocks it and it is the shorter wait of the two.

Milestones 9 to 13 have no overlap with accounts and only build on 2. Two of them pick up a consequence of the
accounts work and say so in their own notes: 13 adds a third surface that names a session's agent, which milestone 7
requires to name the account too, and 11 changes what ending a console session's process does, which milestone 8's
switch must not inherit.

**Aligning the product docs is part of each milestone, not a step at the end.** This plan falsifies standing
statements in `docs/product/consoles-and-projects.md`, `docs/product/settings.md`, `docs/product/sessions.md`,
`docs/product/sidebar.md`, `docs/product/launching-agents.md`, `docs/product/hub-orchestration.md` and
`docs/product/report-panel.md`, and a milestone that shipped behaviour its own product doc contradicts would not be
independently mergeable. Each milestone therefore names the docs it has to bring along.

## Open

Settled later; none of these changes a milestone's goal or how it is verified.

- **Whether a binding can be changed after the session is created.** It is fixed for the session's lifetime today,
  and stays fixed here. Letting the user move a session to another console session, or bind one that started
  unbound, is a feature on top of this plan.
- **Whether a report for a console session with no process running is queued or refused.** It is refused today, as
  any message for a session without a process is, and that is unchanged. The archive rule removes the common case
  (a console session cannot be archived while its sessions are working) but not an interrupted one.
- **The palette's actual colours**, and how many entries it has.
- **What the badge is called in the product documentation.** "Binding badge" is used in this plan to avoid a longer
  compound; the documentation flow may settle on another name.
- **When the grouped list stops reading well as a drop-down.** Milestone 7 offers the agents and their accounts as
  one grouped drop-down. Past some number of entries a list with filtering by typing reads better, but the threshold
  is a judgement to make against a real account list rather than a number to fix here. The grouping is the same
  either way, so this is a change of control and not of model.
- **Whether the orchestration product doc is renamed.** Its file name still contains "hub" after milestone 1, which
  renames the vocabulary inside it but not the file. Renaming the file touches the documentation index and every
  cross-reference to it, for a gain no reader of the prose feels.
- **Whether an account can be moved between agents.** Not offered, on the grounds that a directory belongs to one
  agent's layout. If a user turns out to want it, it is a rename of the agent on the record plus a check that no
  session holds it.
