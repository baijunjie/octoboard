# Probing the agent CLIs

Rules for settling how Claude Code, Codex and Grok Build actually behave by running them.

## Strip the parent session's agent markers before every probe — enumerated, never by prefix

Development here happens inside an agent session, which exports a dozen-odd variables marking the process as an agent
child, and a CLI launched from it inherits them and silently changes behaviour — Claude Code turns transcript saving
off and stops applying `--permission-mode`, with no error and no warning, so the probe answers a question nobody asked.
A login shell does not clean this up: `$SHELL -l -i -c 'env -0'` passes inherited variables straight through, so the
environment has to be filtered explicitly.

**Enumerate the markers to strip; never sweep by name prefix.** The markers and the user's own settings share the same
prefixes and cannot be told apart by name, so a prefix sweep fails in both directions: `GROK_CODE_XAI_API_KEY`,
`GROK_HOME`, `CODEX_HOME` and `CLAUDE_CODE_USE_BEDROCK` are user settings it drops — losing the first stops a
key-authenticated session from starting at all, with a failure that reads as a login problem — while `CLAUDE_PID` and
`CLAUDE_EFFORT` are markers no obvious prefix catches. Build the list by dumping `env` inside a live session of each
agent and stripping what is there and only there. The daemon has to filter its environment snapshot the same way; see
the "Known pitfalls of the Tauri / Rust approach" section of `docs/architecture.md`.

Stripping the enumerated markers does not disturb login state, the credential-bridge variables included. **The
filtered environment does have to keep `USER`**, which no marker list contains and a bare `env -i HOME=…` drops: without
it Claude Code answers `loggedIn: false` even under the real `HOME`, which reads exactly like filtering that broke the
login (the measurement is in the "Moving a conversation to another config directory" section of
`docs/agent-cli-reference.md`). With `USER` in place, a "Not logged in" result from a correctly filtered probe is a real
finding, not an artefact of the filtering.

## Find out which install of a CLI you are probing, and put its version on the finding

The same agent CLI is often installed more than once at different versions — a package manager's copy that stays
put beside a self-updating one, say — and the copies disagree on exactly the behaviour probes are run to settle: a
resumed Codex session's handling of an edited `AGENTS.md` came out opposite on 0.142.5 and 0.162.1. Before a probe,
list every install with `which -a <cli>` and run `--version` on each; probe by explicit path, and say which install
the login shell resolves (`$SHELL -l -i -c 'command -v <cli>'`), since that is the one Octoboard launches, and it need
not be the newest. Write the version into every finding that goes into `docs/agent-cli-reference.md`, and when the
installs disagree, measure both and record the older behaviour as its own case rather than generalising from either.

## Verify a capability with the launch overrides Octoboard actually passes

Before choosing a CLI capability for Octoboard, reproduce its adapter's launch arguments and injection in the isolated
probe, including hook, MCP and instruction overrides even when the capability does not use them. Those overrides can
select a different runtime, so success in a bare CLI launch is only a control, not proof that Octoboard can use it.
The injection is described in `docs/product/per-agent-launch-specifics.md`'s "How each agent is given the injection"
section; measured message-delivery constraints are in `docs/agent-cli-reference.md`'s "Writing into a running session"
section.

## `--help` is not the authority on what a CLI supports

Whole feature surfaces are missing from it. Claude Code's hook-event list does not appear in `--help` at all and has
to be pulled out of the shipped executable with `strings`; Grok Build ships its authoritative user guide on disk under
`~/.grok/docs/user-guide/`, where `10-hooks.md` is the hooks specification and `26-config-reference.md` the config key
table, each tens of kilobytes that `--help` gives no hint of. Read those before concluding a capability is absent —
reasoning from `--help` alone has produced wrong conclusions here more than once. Both sources are rewritten on
upgrade, so record the CLI version next to anything taken from them.

The caution runs the other way too: what is in the binary is not what this build does. A capability named in the
executable's own strings — down to a hint text describing the option that switches it on — can be gated on a
server-side flag, leaving the option accepted and completely inert. Measure the behaviour first and read strings to
explain the measurement, never to conclude that a capability is available.

A configuration file's silence says no more than the strings do. The hook-event names in an injected settings file are
accepted whatever is written in them, invented ones included, while the same file's permission rules are validated
loudly — so "it was registered and the session started without complaint" is not evidence the event exists, and the
loud validation next door is exactly what makes it feel like evidence.

Nor is a real event's name evidence that the handler fires: a CLI can skip a handler type for particular events with
nothing on screen — Claude Code runs no `http` handler for `SessionStart` or `Setup`, saying so only in its debug log —
and a design that waited on such an event held a session's writes until the user typed. So before anything rests on an
event arriving, see it arrive on the real CLI, on each path that has to produce it (a fresh launch, a resume with no
prompt), through the handler type the launch actually registers.

## To know exactly what Claude Code does to an input, run the code that produced the symptom

Applies when the question is the precise behaviour of a check or transform Claude Code runs locally — which characters
a prompt check removes, which inputs it lets through — and above all when Octoboard has to reimplement it. Claude Code
is a Bun single-file executable with its JavaScript embedded as plain text, so the code itself is the answer.

**Find it by the wording the symptom puts on screen, never by a plausible-looking pattern elsewhere in the
executable.** The binary carries many similar regexes for unrelated features: the `\p{Cf}`, `\p{Co}` and `\p{Cn}`
patterns in it belong to tool-name and path handling, and a set taken from them for the prompt's invisible-character
check was wrong in both directions (the check uses a hand-picked list, leaves private-use, unassigned and most format
characters alone, and removes letters and marks those three categories miss). The same goes for a list of what an agent
strips that comes from a model's or a brief's summary: trace it to the code path behind the observed message first.
Search the executable with a short script that maps the file and runs a regex over its bytes; a `grep` with long
context fails on a binary that size (inside a Claude Code session `grep` is ugrep, and it hits its complexity limit).

Cut the self-contained function out and run it under Node: it answers any input with no session and no model turn, and
for a reimplementation it is a differential oracle — feed random strings through Octoboard's version and assert the
extracted function would change nothing in its output. Keep one real session for the end-to-end confirmation. A Unicode
property escape (`\p{Extended_Pictographic}`, `\p{Script=…}`) resolves against the running engine's Unicode data, and
Node's need not be the version Claude Code's runtime carries, so before generating a table from Node's answers check
which way the two versions differ and that the table errs on the side the reimplementation can afford.

## Settle it without spending a model turn, and ask before redirecting a logged-in client's traffic

Before running a real session to find out what an agent does, try the free routes. The CLIs' own inspection
subcommands answer most discovery questions outright and cost nothing: `grok inspect --json` lists every instruction
file it found, `codex debug prompt-input` dumps the model-visible input list, `claude auth status` reports login state
as JSON. Beyond those, pointing the agent's API base URL at a local HTTP server that dumps the request body and
returns an error (`ANTHROPIC_BASE_URL` for Claude Code; each CLI has its own) makes the prompt it assembled — system
prompt, instruction-file blocks, tool definitions — readable with nothing reaching the vendor, and provokes error
paths that are otherwise unreachable.

**A stand-in endpoint or stub that carries a real login or real credentials — a signed-in config directory, a linked
or copied `auth.json`, the real Keychain login — redirects an authenticated client's traffic to a listener you
control, so it needs the user's explicit authorisation every time.** Ask first and leave the probe undone if the
answer is no; being cheap is not a reason to set it up quietly. A stub run from a scratch config directory with a fake
key and no login redirects no authenticated client's traffic and needs no authorisation.

With authorisation, since it carries the user's login, to provoke Grok's `StopFailure` hook (Grok Build 1.0.46),
point `endpoints.models_base_url` at a local stand-in for the chat endpoint that answers HTTP 400. Set it in a
throw-away `GROK_HOME` so the user's own config is untouched, and link the user's `auth.json` into it so Grok starts
logged in. An HTTP 500 makes Grok retry for minutes and never end the turn, so no `StopFailure` fires.

**Where the question is what the agent *does* rather than what it assembled — that it calls an injected MCP tool, what
it does with the result, what the daemon sees around it — a stub that answers in the model's place carries the whole
confirmation, with no turn spent and no working account needed.** Substitute only the inference endpoint and keep every
other part of the run real: a local server speaking the CLI's own chat protocol, selected through the throwaway config
directory alone (for Grok Build, a per-model entry carrying its own `base_url` and `api_key`, picked by the model name
Octoboard passes on as `--model`, so the global endpoint key above is not needed), driving the agent through a scripted
turn that calls the tool. Two things decide whether such a stub works at all. It has to answer the way the CLI actually
asks, which for a CLI that always streams means a server-sent event stream and not a plain JSON body. And a script of
canned turns has to survive the extra, tool-less requests the CLI makes around a turn of its own — a title, a summary
line, a guess at the next user line — by serving them without letting them consume script entries; otherwise the script
runs ahead and the turn under test is answered with something meant for another request. Whether it needs
authorisation follows the rule above: none from a scratch directory with a fake key, the same as any stand-in endpoint
once a real login is carried into it.

## Prove an injection, and an MCP tool, out of band rather than through the model

Whether a launch's hooks and MCP injection took is settled by looking at processes, not by asking an agent to use them.
All three CLIs connect their MCP servers as the process starts, before any turn, so a session opened with no task at all
is enough: find that session's own `octoboardd mcp` child and the injection landed. Look for it with
`ps -axo pid,command | grep <session id>`, because macOS `pgrep -af` prints pids with no command line — a grep of its
output for a session id never matches, and working injection reads as broken injection.

The tools need no agent in the loop either. The MCP child carries the daemon's port and the session's token in its argv,
and the daemon accepts a tool call as plain JSON on `POST /mcp/:token`, so `curl` exercises every tool, the refusal of
one the session's role does not have, and report delivery, against a live daemon and without a single model turn. What
that leaves untested is the stdio child itself — the schemas it announces and the tool name the model ends up
seeing — so keep one real session for those.

## Do not look for a mouse-aware TUI through Grok's bash mode

Grok's bash mode (`!`) has no controlling TTY (`TERM=dumb`, `/dev/tty` unusable), so a mouse-aware TUI such as `vim`
cannot run in it and it is no route to testing mouse reporting.

## A probe whose expected answer is "no" needs a positive control in the same run

When the question is "does the agent read file X" and the result comes back negative, that negative is evidence only
if the same run also shows the agent reading something you already know it reads. Discovery is gated on preconditions
you may not have thought to control for — Grok reads no project instruction file in a folder it has not been told to
trust, and its project hooks also need a git root; server-side feature flags gate discovery the same way — and each of
them turns a whole matrix negative in a way indistinguishable from the thing being measured. When the control fails
too, vary the preconditions one at a time before naming which one is the gate: Grok's folder-trust gate on instruction
files was once recorded here as a need for a git repository.

The kind of login is one such precondition, and the fake-key stub recommended above fixes it at the wrong value for
anything gated on the account: Claude Code shows some screens (its "Resume this conversation?" at launch) only to a
subscription account of a known rate-limit tier, and others only under a server-side flag, so an API-key probe reports
"no dialog" whatever the code does. Read the gate out of the executable first, then satisfy it in the scratch config
directory without a real login — a fake subscription `.credentials.json` carrying the tier the gate checks, and the
flags it reads planted under `cachedGrowthBookFeatures` in that directory's `.claude.json` — with inference on a local
stub. Like the fake key, this carries no real login and needs no authorisation.

A negative that is a *silence* — "no event fires on this path" — needs one thing more than the control: knowing what
would have ended it. That becomes a finding only once you can say why waiting longer could never have produced the
event, because the event you were waiting for is armed by a step the path under test never reaches, for instance.
Establish that mechanism, not a duration; without it the probe has measured its own patience and "wait longer" stays an
objection you cannot answer.

The reverse holds for a probe whose expected answer is "refused": put the target where nothing else already allows it.
Grok's sandbox leaves temp directories writable under every profile, and an agent session's scratch directory usually
sits under one (`/private/tmp` on macOS), so a sandbox-write probe whose target resolves there comes back "allowed"
whatever the sandbox does with the path under test; place such a target outside every temp directory (on macOS, under
`~/Library/Caches`, say).

## Moving files into a config directory is proven only by a resume that opens the intended conversation

Applies whenever a change copies, links or shares an agent's files between config directories — what a session-account
switch carries, a directory two accounts share — or decides to leave some behind. Neither "the files arrived" nor "the
resume started without an error" settles it: an agent resolves a resume from whatever it finds in the target, and what
it finds can open a stale branch of the conversation with no error or warning. Codex does: a thread that had a turn
reverted is a chain of rollout files, and leaving the later ones out — or a target whose own database row names an
older head, even with every file present — resumes with the reverted turn back and the newest turns gone (the "Codex,
history segments" note in the "Moving a conversation to another config directory" section of
`docs/agent-cli-reference.md`).

**Decide what each file is from its contents, never from its name, before deciding what to carry.** Codex's
`rollout-…-<id>_<x>` files were once read from their names as subagent rollouts and left behind; their `session_meta`
carries the thread's own id and the lineage that makes them later segments of it.

So the check is a real resume under the target directory, built from a copy of a real session whose history produced
every kind of file the agent writes for a conversation — for Codex a thread with a reverted turn, compressed files
included — since a fixture holding only a plain, linear record cannot show a stale pick. Then confirm which branch came
up: a turn that survives present and a reverted turn absent, and the record the agent went on to use (Codex writes the
path of the rollout it opened into the home's database). Repeat it for every arrangement the change can produce,
including a target that already holds the conversation and its own record of it from before the source changed.

## A permission probe cannot assume that anything prompts: set the mode yourself and read it back

Permission-prompt behaviour cannot be probed before reading the settings file of the config directory the CLI resolves
(`settings.json` for Claude Code): a default-mode setting there can put every session into a mode that never asks, and
the allow list can pre-approve the very call the probe meant to be denied on. Both produce the same lone observation —
no prompt appeared — and it reads as "this build does not prompt here".

So pick a call that neither the allow list nor any pattern in it covers — a reproduction command copied from a bug
ticket is among the likeliest things to be allowlisted, exactly because it is something the user runs often — and
**write the prompting mode into the probe's own config directory instead of relying on any default**: neither the
user's settings nor a fresh directory's own default can be assumed to be a mode that asks, and a fresh directory
carrying no permission rules at all is not. For Claude Code that is `permissions.defaultMode: "default"` in the
throwaway directory's `settings.json`. Nothing overrides the allow list.

Then confirm the mode in effect from the running session's own transcript records, not from its status line, before
reading anything into a prompt that did not appear. Asking for a mode is not getting it, and the status line's wording
for a mode need match neither the settings value that asked for it nor any ordinary name for it, so it settles nothing
on its own.

## Measure a pre-approval setting under every approval policy, and from the server's side

A setting meant to make something run unasked — a per-server or per-tool approval value for an injected MCP server,
for one — is evaluated together with the session's overall approval policy, which is the user's own setting and is not
the same for an interactive launch as for a headless one. A value that does not in fact pre-approve fails differently
under each: under a policy that asks, it raises a modal; under one that never asks, the call is refused outright and
never sent. So a measurement under one policy settles nothing about the other, and "nothing prompted" under the
second is the symptom of the setting not working rather than of it working. Measure each policy the product can
launch that agent under.

Put the instrument on the far side of the call: a stub stdio MCP server that logs which JSON-RPC methods reach it is
what tells "refused before anything was sent" from "sent and approved", which the agent's screen does not — a refused
call surfaces as a failed tool call, the same shape as a server that erred. Register the stub under a key of its own
instead of probing through Octoboard's injected server, whose tools act for real, and let its tool declare no
annotations: an agent left to decide for itself asks for an unannotated tool, so an annotated stub answers an easier
question than the one being asked. The values measured for Codex are in the `Codex` section of
`docs/agent-cli-reference.md`.
