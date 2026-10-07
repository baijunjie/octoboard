# Named agent accounts

A user who keeps more than one login of the same agent — a personal Claude Code subscription and a work one, say —
can name each of them in Settings, pick one when a session is opened, and move a running session from one to
another by hand when its usage has run out.

## Problem

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

An **account** is a named config directory of one agent — the directory that agent keeps its login and its
conversation history in. Accounts are kept once for the whole application, in Settings, and referred to everywhere
a config directory is referred to today.

- **On every start** Octoboard works out which agents are available — their binaries resolve or they do not — and
  what each agent's default account resolves to, so there is always something to pick without the user configuring
  anything.
- **Settings** gains an Agent accounts section: the accounts of each agent, with adding, editing and removing.
- **The console** no longer takes a path per agent; it picks one of that agent's accounts instead. Every path a
  console holds today becomes an account, so no user loses a setup.
- **Opening a session** offers the agent and the account in one control: a single grouped list whose groups are the
  agents and whose entries are that agent's accounts. Picking an entry settles both at once, so no combination that
  does not exist can be chosen.
- **With no agent available** no session can be opened at all, and the user is told to install one instead of being
  let through to a launch failure.
- **A running session** can be moved to another account of the same agent from its action menu, when its agent has
  more than one. The session's conversation is relocated into the account's config directory first, so the move
  resumes the conversation rather than starting a new one.

## Key design decisions

**Accounts replace the console's config directory fields rather than layering over them.** Two ways to name the same
directory would mean two precedence chains for the user to hold in their head, and a path typed into a console could
not be shown by name anywhere. The migration makes this invisible: a console's stored path becomes an account named
after the directory, by the naming rule below.

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
same file (see "Agent config directories" in `docs/product/consoles-and-projects.md` and "Per-agent specifics a user
will notice" in `docs/product/launching-agents.md`). A default account that pinned `~/.claude` would move every
existing user's Claude Code onto a config file with no trust decisions and no history in it. For the same reason
Octoboard never offers the agent's own default directory as a *second*, pinned account: the default account already
covers it.

**What "pinning nothing" means at launch differs by agent**, and the difference is the existing one rather than a new
one. For Claude Code and Codex Octoboard sets no variable, so whatever the user's shell exports stands, and the
agent's own default applies when it exports nothing. For Grok Build, `GROK_HOME` is set on every
launch regardless, to the per-session home Octoboard builds; what an account pins is the **source home** that home
is built from, so the default account means no source home is pinned and the source is whatever the user's shell
exports, else `~/.grok` (see "The launch environment" in `docs/product/launching-agents.md`).

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
per-session home, which is discarded with the process — neither persists (see "Grok Build" under "Per-agent
specifics a user will notice" in `docs/product/launching-agents.md`). That is exactly the silent failure a switch
must not produce, so a Grok session whose account directory is not an initialized Grok home is **refused at
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
migration of the console paths — it uses the directory's last path component, and when that is already taken for
that agent it uses the whole directory instead, since a path is always distinct.

**The Agent accounts section says nothing about which accounts are in use.** A count that changes while the dialog
is open is noise, and nothing a user would do differently for it: an account can be removed whether or not sessions
hold it, because a session launches from the directory it recorded.

## Milestones

1. [Accounts: storage, protocol and the migration of the console's paths](01-accounts-storage-and-migration.md)
2. [Agent availability and the default account](02-availability-and-the-default-account.md)
3. [The Agent accounts section in Settings](03-settings-section.md)
4. [Choosing an account where an agent is chosen](04-account-pickers.md)
5. [Switching a session's account](05-switching-a-sessions-account.md)

[The relocation findings](agent-relocation-findings.md) are a reference rather than a milestone: they are already
measured, and milestone 5 is what acts on them.

**Aligning the product docs is part of each milestone, not a step at the end.** This plan falsifies standing
statements in `docs/product/consoles-and-projects.md`, `docs/product/settings.md`, `docs/product/sessions.md`,
`docs/product/sidebar.md`, `docs/product/launching-agents.md` and `docs/product/hub-orchestration.md`, and a
milestone that shipped behaviour its own product doc contradicts would not be independently mergeable. Each
milestone therefore names the docs it has to bring along.

## Open

- **When the grouped list stops reading well as a drop-down.** Milestone 4 offers the agents and their accounts as
  one grouped drop-down. Past some number of entries a list with filtering by typing reads better, but the
  threshold is a judgement to make against a real account list rather than a number to fix here. The grouping is the
  same either way, so this is a change of control and not of model.
- **Whether an account can be moved between agents.** Not offered, on the grounds that a directory belongs to one
  agent's layout. If a user turns out to want it, it is a rename of the agent on the record plus a check that no
  session holds it.
