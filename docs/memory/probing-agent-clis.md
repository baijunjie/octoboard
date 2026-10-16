# Probing the agent CLIs

Rules for settling how Claude Code, Codex and Grok Build actually behave by running them, and for probing a live
Octoboard session that launches them.

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

Stripping the enumerated markers does not disturb login state, the credential-bridge variables included: Claude Code
still resolves its Keychain credentials and reports `loggedIn: true`. **The filtered environment does have to keep
`USER`**, which no marker list contains and a bare `env -i HOME=…` drops: without it Claude Code cannot reach its
Keychain credential and answers `loggedIn: false` even under the real `HOME` with no `CLAUDE_CONFIG_DIR` set, which
reads exactly like filtering that broke the login (Claude Code 2.1.296, macOS; Codex 0.160.0 needs no such variable).
With `USER` in place, a "Not logged in" result from a correctly filtered probe is a real finding, not an artefact of
the filtering.

## Find out which install of a CLI you are probing, and put its version on the finding

The same agent CLI is often installed more than once at different versions — a package manager's copy that stays
put beside a self-updating one, say — and the copies disagree on exactly the behaviour probes are run to settle: a
resumed Codex session's handling of an edited `AGENTS.md` came out opposite on 0.142.5 and 0.162.1. Before a probe,
list every install with `which -a <cli>` and run `--version` on each; probe by explicit path, and say which install
the login shell resolves (`$SHELL -l -i -c 'command -v <cli>'`), since that is the one Octoboard launches, and it need
not be the newest. Write the version into every finding that goes into `docs/agent-cli-reference.md`, and when the
installs disagree, measure both and record the older behaviour as its own case rather than generalising from either.

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

## Settle it without spending a model turn, and ask before redirecting traffic

Before running a real session to find out what an agent does, try the free routes. The CLIs' own inspection
subcommands answer most discovery questions outright and cost nothing: `grok inspect --json` lists every instruction
file it found, `codex debug prompt-input` dumps the model-visible input list, `claude auth status` reports login state
as JSON. Beyond those, pointing the agent's API base URL at a local HTTP server that dumps the request body and
returns an error (`ANTHROPIC_BASE_URL` for Claude Code; each CLI has its own) makes the prompt it assembled — system
prompt, instruction-file blocks, tool definitions — readable with nothing reaching the vendor, and provokes error
paths that are otherwise unreachable.

**That second route redirects an authenticated client's traffic to a listener you control, so it needs the user's
explicit authorisation every time.** Ask first and leave the probe undone if the answer is no; being cheap is not a
reason to set it up quietly.

With that authorisation, to provoke Grok's `StopFailure` hook (Grok Build 1.0.46), point `endpoints.models_base_url`
at a local stand-in for the chat endpoint that answers HTTP 400. Set it in a throw-away `GROK_HOME` so the user's own
config is untouched, and link the user's `auth.json` into it so Grok starts logged in. An HTTP 500 makes Grok retry
for minutes and never end the turn, so no `StopFailure` fires.

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
runs ahead and the turn under test is answered with something meant for another request. It redirects an
authenticated client's traffic like any other stand-in endpoint, so it needs the same authorisation.

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

A negative that is a *silence* — "no event fires on this path" — needs one thing more than the control: knowing what
would have ended it. That becomes a finding only once you can say why waiting longer could never have produced the
event, because the event you were waiting for is armed by a step the path under test never reaches, for instance.
Establish that mechanism, not a duration; without it the probe has measured its own patience and "wait longer" stays an
objection you cannot answer.

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

## A probe is not read-only: give it its own scratch directory

Running these CLIs writes into the user's real configuration. The product's rule against touching the user's global
agent configuration does not extend to probes, and a probe cannot avoid it, so plan for the residue instead.

The configuration written is whichever one the probed CLI resolves, not necessarily the default location:
`CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `GROK_HOME` are user settings that stripping the agent markers rightly keeps, so
a probe launched from a shell that exports one writes into that directory. Look for the residue there, and when the
probing is done tell the user what it left in their configuration.

A throwaway config directory built as a symlink farm over the user's real one — the arrangement that keeps the login
without copying a credential — **writes through every entry left symlinked**. So before the run, give the farm a real
file or directory of its own for each entry the CLI writes: its config file, its trust store, and an empty directory in
place of the symlinked session store, or the probe's own session records land in the user's home. What stays symlinked
still collects the run's residue, a live-session index, logs and caches among it, so include it in what you tell the
user the probe left.

Use a dedicated scratch directory as cwd, never a shared one such as `/tmp`: each CLI records a per-directory trust
decision in the user's own config (Claude Code in the `.claude.json` of the config directory in effect, under
`projects[<path>].hasTrustDialogAccepted`), so probing directly in `/tmp` marks `/tmp` itself trusted and every later
probe run with `/tmp` as its own cwd silently starts out trusted, quietly invalidating any test of untrusted-workspace
behaviour. Below a trusted directory it depends on git (read out of the Claude Code 2.1.294 executable): a directory
that is not in a git repository is trusted through any trusted directory above it, without a trust screen and with its
own entry left at `false`, while inside a repository only directories up to the repository root count. So a fresh
scratch subdirectory of a trusted `/tmp` starts out trusted too unless it is a repository. Codex asks only inside a git
repository and records the trust for the repository's root — from a linked worktree, the main checkout's root (Codex
0.161.0) — so a Codex probe run from this repository or any worktree of it marks the user's own clone trusted in their
`config.toml`; give a Codex trust probe a fresh `git init` inside its scratch directory instead. Probes also leave
session records behind in the user's agent directories.

A fresh scratch directory is by definition untrusted, and Claude Code stops there on its folder-trust dialog and
does nothing else — a probe that looks like it produced no output at all is usually sitting on that dialog. Grepping
the captured output for the dialog's words finds nothing either: as of Claude Code 2.1.289 its full-screen UI places
each word with a cursor-move sequence instead of printing spaces, so strip escape sequences *and* all whitespace before
matching any text in a capture. Unless untrusted behaviour is the thing being measured, answer the dialog once in that
directory before any measurement: the cursor starts on "No, exit", so the answer is Down then Enter, and an Enter on
its own — a pasted line's trailing CR included — ends the session instead.

When several probes run concurrently, each needs its own distinct scratch directory, and expect to see the other
probes' trust entries and session records — that residue is not evidence of a defect.

## A probe that goes through the daemon runs against the user's live Octoboard data unless its `HOME` is a throwaway one

The daemon derives its data directory, its database and its single-instance lock from `$HOME`, and writes its port file
to `$TMPDIR`. So a probe that does not need a logged-in agent — the protocol, the terminal socket, the window's
connection — runs the daemon, or a copy of the app, with a throwaway `HOME` and `TMPDIR`: the user's board is untouched
and it runs alongside their own Octoboard. For a session that draws real output there without a model turn, open a
Codex session: with no login under that `HOME` it stops at its sign-in screen.

**Drive it from a client of its own on the control WebSocket, and build no application for it.** The daemon on its
own, plus a small client on `GET /ws/control` (that section of `apps/daemon/PROTOCOL.md`) to create the consoles,
projects and sessions, read the events and send the answers, is the whole instrument whenever what differs between
agents is on the agent's side: the window's rendering of a dialog, and answering it, are agent-independent, so once
they have been exercised for one agent, confirming the behaviour for the next needs no window, no packaged build and
none of the screen-recording or accessibility permissions that driving a window needs. Build the application only for
behaviour the Tauri shell itself carries.

Launch it from an emptied environment, `env -i HOME=<dir> TMPDIR=<dir> … /usr/bin/open -n <app>`, never with
`open --env` overrides alone: `open` hands the app the calling shell's whole environment besides the variables it
names, so a `CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `GROK_HOME` exported there — an agent session exports
`CLAUDE_CONFIG_DIR` routinely — points the run's agents at the user's own configuration despite the throwaway `HOME`,
and for Codex and Grok Build at the login kept there too. The same goes for a daemon started straight from the shell.

A throwaway `HOME` hides Claude Code's keychain login as well: the login keychain is looked up under
`$HOME/Library/Keychains`, so there Claude Code comes up logged out. When a run needs it logged in, symlink
`<throwaway HOME>/Library/Keychains` to the real `~/Library/Keychains`, which brings back every account's login
without copying a credential. That symlink is still what carries the login as of 2.1.296: removing it alone, with
everything else left in place, flips the same probe back to `loggedIn: false`.

**Leave `CLAUDE_CONFIG_DIR` unset in such a run, and never set it to repair a login that looks broken.** Setting it
at all costs Claude Code the login, whatever directory it names — pointed at the real `~/.claude`, or at the
`<throwaway HOME>/.claude` that the unset variable resolves to anyway, the probe answers `loggedIn: false` (2.1.296).
Unset is what a probe wants regardless: the config directory then lands inside the throwaway `HOME`, where the probe
owns it and can write its own `settings.json`. The agents are not symmetric here — Codex comes up logged in with
`CODEX_HOME` pointed at the user's real `~/.codex` under a throwaway `HOME` (0.160.0) — so the arrangement that
worked for a Codex probe is the wrong one to carry over to Claude Code.

A fake agent CLI for such a daemon goes on `PATH` from the throwaway `HOME`'s shell rc file (`.zshrc` when `$SHELL`
is zsh), never by prepending it to the `PATH` the daemon is started with. The daemon launches every agent with the
environment of `$SHELL -l -i -c env`, and on macOS that login shell's `/etc/zprofile` runs `path_helper`, which moves
the system paths (`/etc/paths`, `/etc/paths.d`, where Homebrew's `bin` usually sits) ahead of everything inherited. An
inherited prepend therefore loses to an installed agent of the same name, and the *real* CLI launches under the
throwaway `HOME` with nothing on screen saying so; the rc file is sourced after `/etc/zprofile`, so an
`export PATH=<fake bin>:$PATH` there wins. For the same reason, making an agent unavailable in such a run takes
removing from `PATH` in that rc file every directory that holds it (`which -a <cli>`), Homebrew's `bin` included,
since the same CLI is often installed in more than one. Confirm from the session's process (`ps`) which executable
actually ran before reading anything off the screen.

Everything else a launched agent has to see belongs in that same rc file, for the same reason: the real `PATH` —
without it the throwaway `HOME`'s login shell finds no agent executable at all — and a config-directory variable such
as `CODEX_HOME` or `GROK_HOME` pointing at the run's own throwaway configuration.

A probe that needs a logged-in agent whose login cannot be brought into the throwaway `HOME` has to run on the real
one. Expect it to create consoles and sessions in the user's real board and to run any pending schema migration
against the user's live database — copy that database aside first whenever the change being probed touches the
schema, and plan the cleanup as part of the probe rather than after the fact.

Clean up in the order the daemon enforces, and leave time between the steps: archiving a session returns as soon as its
record is written, while the agent process is still only being asked to exit, so deleting the console straight
afterwards is refused for "running sessions" even though every session was archived. Wait for those processes to be
gone, then delete — the refusal is the daemon working, not a defect.
