# Five new agent providers

Adding Gemini CLI, OpenCode, GitHub Copilot CLI, Pi and Qwen Code to the agents Octoboard launches and drives, beside
Claude Code, Codex and Grok Build.

## Problem

Octoboard's rule is that a new agent is a descriptor, an adapter directory, a presentation module and one line in each
registry (`docs/conventions/provider-neutral-core.md`, "The rule"). That rule has only ever been exercised by three
agents that look alike: each reports through hooks whose payload names its event, each has one variable that names its
configuration directory, each signs in to one subscription through a browser, and each has a plan quota its CLI can be
asked for. The five new CLIs break those likenesses:

- **Pi** has no hooks; it reports its state through OSC 7501 by itself, and takes an extension file per launch.
- **OpenCode** has no hooks either; a plugin inside its process sees its events. It keeps conversations in a SQLite
  database, has no variable that moves its data directory, and holds keys for many model providers at once.
- **Gemini CLI** has hooks, but its variable names the parent of its configuration directory, it has no flag that
  appends to the system prompt, and its hooks do not run in a folder it does not trust.
- **GitHub Copilot CLI** has hooks and a plan, but its code cannot be read and its quota has no known command.
- **Qwen Code** has Claude-style hooks and flags, and a sign-in model that is in the middle of changing.

What each offers, with where every fact comes from, is in `cli-fact-sheets.md`. The sheets were written from the
installed packages and their docs alone: no TUI was started, no model was called and no account was signed in.

So the work has two parts: the places where generic code still describes the first three agents, and the five
providers themselves.

## Plan outline

1. **Generic layer first, merged before any provider.** Three milestones repair the places that are keyed by agent or
   by the first three agents' formats, add the descriptor facts the new shapes need, and teach the accounts surfaces
   about an agent with no plan quota and no browser sign-in. They change no behaviour of the three existing agents.
2. **One milestone registers all five at once**, as bare providers behind a preview gate and with their brand marks
   and colours, so that every shared file is edited once and the provider branches never meet in one.
3. **Five provider milestones run in parallel**, each in its own branch and session, each touching only its own
   provider's files. Each starts by measuring its CLI, since nothing in the fact sheets was measured on a running
   session.
4. **A release step per provider** lifts its gate and updates the shared product docs, the gallery fixtures and the
   website, one provider at a time on the target branch.

## Key design decisions

### Provider branches touch only their own provider's files

A provider milestone edits its descriptor (`apps/daemon/src/providers/<id>.rs`), its adapter directory
(`apps/daemon/src/adapter/<id>/`), its test data (new files named for it under `apps/daemon/testdata/`), its
presentation module and served-info fixture (`packages/ui/src/providers/<id>.ts` and the fixture beside it) and its own
section of `docs/agent-cli-reference.md`. Nothing else.

Every file two providers would both have to edit is edited for all five in one place:

| Shared file | Edited by |
|---|---|
| `apps/daemon/src/providers/mod.rs` (`mod` lines, `REGISTRY`) and `apps/daemon/src/adapter/mod.rs` (`mod` lines, `PROVIDERS`) | `04-register-preview-providers.md`; the gate on each line by `10-release-a-provider.md` |
| `packages/ui/src/providers/registry.ts` (`BUILT_IN`) and `packages/ui/src/providers/agentMarks.ts` | `04-register-preview-providers.md` |
| `scripts/verify/provider-neutral.mjs` (the agent vocabulary) and its test | `01-provider-neutral-repairs.md` makes it per-provider; `04-register-preview-providers.md` fills it |
| `docs/agent-cli-reference.md` (a section per agent) | `04-register-preview-providers.md` creates the five headings; each provider writes inside its own |
| The cross-agent tables of `docs/agent-cli-reference.md` and the product docs that list agents | `10-release-a-provider.md` |
| The gallery and fixture daemon's provider list, the website and the documentation site | `10-release-a-provider.md` |
| Generic traits, `Capabilities`, `AgentLayout`, wire types and generated bindings | The three generic milestones only |

Why: registries, tables and fixtures are lists, and five branches appending to the same list conflict at every merge.
A provider that finds it needs a change outside its own files has found a gap in the generic layer: it reports back,
and the change lands on the target branch as a generic change, not in the provider's branch.

There are no shared i18n resources to protect: a provider's copy, in every language, lives inside its own presentation
module.

### Unfinished providers are registered behind a preview gate

The five are registered early so that their branches are independent, but a bare provider has no status, no Octoboard
tools and no role, and must not reach a release. Their registry lines and module declarations carry
`#[cfg(any(test, debug_assertions))]`, the same way the test-only provider's carry `#[cfg(test)]`: tests, clippy and
development builds see all five, a release build sees only those whose gate has been lifted.

Nothing in the UI is gated: the window lists what the daemon serves, so a provider a release build does not serve is
simply not shown, whatever its presentation module holds.

Why this gate, which the user chose: it needs no runtime code and no change to any place that lists agents; the daemon
simply serves fewer providers, and a database a development build wrote is already handled by a build that lacks an
agent (`docs/product/supported-agents.md`, "Data of an agent this build does not support"). Rejected: a runtime
`preview` flag on the descriptor, because every one of the many places that list agents would have to honour it; no
gate at all, because a release cut while the work is in flight would offer agents that cannot be orchestrated; and no
early registration, each provider branch adding its own registry lines, because every merge would then conflict in
each shared file.

### Brand marks and colours come with the registration

Each of the five gets its own mark, and its own colour where its brand has one, in the milestone that registers it,
and the user confirms them there before it merges. They are not left as the generic mark until release.

Why, by the user's decision: a mark is what tells the agents apart in every list from the first development build on,
and settling all five in one place lets them be judged side by side and keeps the shared list of marks
(`packages/ui/src/providers/agentMarks.ts`) out of the provider branches. That list only supplies the website with
marks by id; the website names the agents it shows itself, so an unreleased provider's mark appears nowhere outside a
development build.

### No new status mechanism

All five fit what exists:

- An agent with hooks (Gemini CLI, Copilot CLI, Qwen Code) is read by its `HookInterpreter`, as today.
- An agent whose events come from a plugin or extension inside its process (OpenCode, and Pi's richer events later)
  forwards them through the same hook route. Octoboard writes that plugin, so the payload's shape is Octoboard's own,
  and the provider's `HookInterpreter` reads it. The plugin's source is an asset of the provider's adapter directory.
- Pi's own OSC 7501 reports give its status through the path a provider without a status integration already takes
  (`AgentProvider::has_status_integration` false; `docs/product/program-status-reports.md`).
- An agent driven through its own server (OpenCode's `serve` and `attach`, later) fits `PrivateServer` and
  `MessageTransport`, which Codex already uses.

Why: each of these is one of the two existing paths, hooks or program reports; a third would be a second description
of the same thing.

### Injection stays per launch and never edits the user's files

Each provider is given its hooks or extension, the Octoboard MCP server and the role text through a flag, a variable
that names a file Octoboard wrote in the session's scratch directory, or an extension file loaded for the run. Where a
variable replaces a file the user may have (Gemini CLI's system settings file, a config variable the user already
sets), the adapter carries the user's content into what it writes. This is the existing rule
(`docs/product/launching-agents.md`, "What Octoboard never modifies").

### An account's directory is always the agent's configuration directory

Gemini CLI's variable names the parent of `.gemini`. The layout declares that, the generic helpers export the parent,
and a Gemini account's directory has to be named `.gemini`, which the provider refuses otherwise and its copy
explains. Rejected: treating the variable's value as the account directory, because the default account's directory
would then be the user's home directory, which the removal, sharing and display code must never be handed.

OpenCode has no variable that moves its data directory. By the user's decision its provider declares that it has only
the default account; the window offers no Add account for it. Rejected: isolating OpenCode accounts through
`XDG_DATA_HOME` and its siblings, because every program the agent starts would inherit them; and accounts as sets of
credentials handed over through `OPENCODE_AUTH_CONTENT`, because that puts keys into the environment of every process
the agent starts.

### An agent without a plan quota says so, and its sign-in is read apart from quota

Today an account's sign-in state is a by-product of its quota reading, and an agent that is never read shows "Reading
usage…" for ever. A provider that declares no quota window is one Octoboard reads no plan quota for: its accounts show
no usage anywhere and it has no tab in the usage panel. Its sign-in state comes from a reading of its own that the
provider answers from the agent's sign-in record. Log in is offered only where the provider can drive the agent's
sign-in without a terminal; elsewhere the account says how to sign in with the agent's own CLI.

All five start without a quota reading. A provider that has a plan (Copilot CLI, Gemini CLI with a Google login,
possibly Qwen Code) declares its windows when its quota reading is built, which is after its first milestone.

Octoboard takes no API keys from the user, by the user's decision: an account that signs in with a key shows the
command of the agent's own CLI to run, and Octoboard offers no field for the key. Why: "Octoboard never uses or writes
the credential itself" (`docs/product/logging-accounts-in.md`, "How each agent signs in"). Rejected: a key field in
the login dialog that pipes the key to the agent's own login command, because Octoboard would then handle a secret.

### Each provider ships in two steps: a usable session first, the account features after

A provider milestone's scope is what makes a session of the agent a full member of the board:

- launching and resuming, with the model, the effort where the agent has one, and the first task;
- the injected status reporting, Octoboard MCP server and role text;
- the session's status, its turns and the hand it raises, from the agent's own signals;
- the agent's folder-trust confirmation, its interrupt key, and its title;
- the account's directory, its sign-in state and, where it can be driven, Log in;
- its model list, its permission setting, its CLI's install and update facts, and its copy.

Left for later, each as work of its own once the provider is released: the plan quota and its panel, the agent's own
records (history of an archived session, the context size, a missing conversation refusing a resume, removing records
with a session), carrying a conversation across an account switch or to another directory, sharing the default
configuration, the MCP inventory, compatibility with another agent's configuration, and delivery through the agent's
own server. Why this cut: every item in the first list has a safe default in the generic layer when a provider does not
answer (`docs/conventions/provider-neutral-core.md`, "Daemon": a default method is the answer of a provider with none
of the thing), so a provider without the second list is limited, not wrong.

### Measured, not assumed

Every provider milestone opens with measurement. A probe runs in a scratch home and a scratch working directory of its
own, never against the user's configuration, and without a model call where it can: a local stand-in answers the
agent's inference requests. A sign-in is done by the user, on an account they choose, since a login flow completes in
their own browser; and a probe reads a real account's state but never spends or changes anything on it. What needs a
real account is listed in each milestone. A fact that contradicts the fact sheet replaces it in the provider's section
of `docs/agent-cli-reference.md`; the sheet is not kept up to date after its milestone starts.

### Ids, names and order

| Id | Label | Short name |
|---|---|---|
| `gemini` | Gemini CLI | Gemini |
| `copilot` | GitHub Copilot CLI | Copilot |
| `opencode` | OpenCode | OpenCode |
| `qwen` | Qwen Code | Qwen |
| `pi` | Pi | Pi |

They are registered after Grok Build in this order, which is the order every list of agents shows them in; the user
accepted the ids and the order. The milestones are numbered by ease instead, easiest first, for whoever staffs them
one at a time.

## Milestones

Generic layer, in order:

1. [Provider-neutral repairs](01-provider-neutral-repairs.md)
2. [Descriptor facts for new agent shapes](02-descriptor-facts-for-new-agent-shapes.md)
3. [Accounts without a plan quota or a browser sign-in](03-accounts-without-plan-quota-or-browser-sign-in.md)
4. [Register the five as preview providers](04-register-preview-providers.md)

Providers, in parallel once the four above are merged:

5. [Pi](05-pi-provider.md)
6. [Qwen Code](06-qwen-code-provider.md)
7. [GitHub Copilot CLI](07-copilot-cli-provider.md)
8. [Gemini CLI](08-gemini-cli-provider.md)
9. [OpenCode](09-opencode-provider.md)

After each provider:

10. [Release a provider](10-release-a-provider.md), carried out once per provider, one at a time

## Open

No decision of the user's is outstanding: the preview gate, OpenCode's single account, no API keys in Octoboard, the
marks and colours with the registration, and the ids and their order are settled above.

Work this plan knows of and does not yet split into milestones, because what it takes cannot be said before a
measurement with a real account:

- **Plan quota for Copilot CLI, Gemini CLI and Qwen Code.** No command of any of them is known to print it. Gemini
  CLI's is per-model daily buckets, which no existing window kind names, so its panel needs a new kind on the wire.
- **Records kept in a database.** `RecordStore` walks directories and folds lines; OpenCode's conversations are rows
  in SQLite. Its history, context size and removal need a record store that is not file-shaped.
- **A shared ACP client.** Gemini CLI, Copilot CLI, Qwen Code and OpenCode all speak the Agent Client Protocol over
  stdio. It is the candidate for listing Copilot CLI's and Qwen Code's models and for driving Gemini CLI's sign-in. If
  a measurement confirms either, the client is a generic helper built once before the providers that use it.
- **A hook that prints.** If Gemini CLI's role text can only be given as a `SessionStart` hook's output, the hook
  needs a command that prints a file on every host, which the daemon's own executable does not offer today.
- **One instruction text read several times.** A console's directory holds one link per registered provider's
  instruction file name. With `GEMINI.md` and `QWEN.md` added beside `CLAUDE.md` and `AGENTS.md`, an agent that reads
  several of those names (Copilot CLI reads three, Pi and Qwen Code two) may read the console's text more than once.
  Each provider milestone measures it; if one does, the remedy is decided then.
- **OpenCode's next generation.** The vendor's documentation already describes a version with other config keys, a
  different plugin interface and other server routes. The OpenCode milestone targets the installed generation and
  records which facts would change.
- **Windows.** Nothing about the five on Windows is measured. Each provider milestone keeps the Windows cross-check
  passing; measuring them on Windows is work after release.
