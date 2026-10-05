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
still resolves its Keychain credentials and reports `loggedIn: true`. So a "Not logged in" result from a correctly
filtered probe is a real finding, not an artefact of the filtering.

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
you may not have thought to control for — Grok locates a project by walking up for a `.git` directory and reads no
project instructions without one, and folder trust and server-side feature flags gate other agents' discovery the same
way — and each of them turns a whole matrix negative in a way indistinguishable from the thing being measured.

A negative that is a *silence* — "no event fires on this path" — needs one thing more than the control: knowing what
would have ended it. That becomes a finding only once you can say why waiting longer could never have produced the
event, because the event you were waiting for is armed by a step the path under test never reaches, for instance.
Establish that mechanism, not a duration; without it the probe has measured its own patience and "wait longer" stays an
objection you cannot answer.

## A permission probe runs under the user's own settings, and they void the obvious probe command

Permission-prompt behaviour cannot be probed before reading the settings file of the config directory the CLI resolves
(`settings.json` for Claude Code): a default-mode setting there can put every session into a mode that never asks, and
the allow list can pre-approve the very call the probe meant to be denied on. Both produce the same lone observation —
no prompt appeared — and it reads as "this build does not prompt here".

So pick a call that neither the allow list nor any pattern in it covers, and ask for the mode explicitly on the command
line. A reproduction command copied from a bug ticket is among the likeliest things to be allowlisted, exactly because
it is something the user runs often. The command line overrides the default-mode setting; nothing overrides the allow
list.

Then confirm from the running session itself — its status line names the permission mode in effect — that the mode you
asked for is the mode you got, before reading anything into a prompt that did not appear. Asking for a mode is not
getting it.

## A probe is not read-only: give it its own scratch directory

Running these CLIs writes into the user's real configuration. The product's rule against touching the user's global
agent configuration does not extend to probes, and a probe cannot avoid it, so plan for the residue instead.

The configuration written is whichever one the probed CLI resolves, not necessarily the default location:
`CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `GROK_HOME` are user settings that stripping the agent markers rightly keeps, so
a probe launched from a shell that exports one writes into that directory. Look for the residue there, and when the
probing is done tell the user what it left in their configuration.

Use a dedicated scratch directory as cwd, never a shared one such as `/tmp`: each CLI records a per-directory trust
decision in the user's own config (Claude Code in the `.claude.json` of the config directory in effect, under
`projects[<path>].hasTrustDialogAccepted`), so probing directly in `/tmp` marks `/tmp` itself trusted and every later
probe run with `/tmp` as its own cwd silently starts out trusted, quietly invalidating any test of untrusted-workspace
behaviour. That decision is keyed on the exact path and reaches nothing below it: a parent directory carrying an
accepted trust flag leaves a fresh subdirectory of it untrusted, so never carry "that directory is already trusted"
over to a path beneath it. Probes also leave session records behind in the user's agent directories.

A fresh scratch directory is by definition untrusted, and Claude Code stops there on its folder-trust dialog and
does nothing else — a probe that looks like it produced no output at all is usually sitting on that dialog. Grepping
the captured output for the dialog's words finds nothing either: as of Claude Code 2.1.289 its full-screen UI places
each word with a cursor-move sequence instead of printing spaces, so strip escape sequences *and* all whitespace before
matching any text in a capture. Unless untrusted behaviour is the thing being measured, answer the dialog once in that
directory before any measurement: the cursor starts on "No, exit", so the answer is Down then Enter, and an Enter on
its own — a pasted line's trailing CR included — ends the session instead.

When several probes run concurrently, each needs its own distinct scratch directory, and expect to see the other
probes' trust entries and session records — that residue is not evidence of a defect.

## A probe that goes through the daemon runs against the user's live Octoboard data unless it needs no logged-in agent

The daemon derives its data directory, its database and its single-instance lock from `$HOME`, and writes its port file
to `$TMPDIR`. So a probe that does not need a logged-in agent — the protocol, the terminal socket, the window's
connection — runs the daemon, or a copy of the app, with a throwaway `HOME` and `TMPDIR`: the user's board is untouched
and it runs alongside their own Octoboard. For a session that draws real output there without a model turn, open a
Codex session: with no login under that `HOME` it stops at its sign-in screen. Make sure `CODEX_HOME` is not set in
the environment it is launched from, or Codex reads the user's own configuration and login after all. Do not count on
a changed `HOME` hiding Claude Code's login, which lives in the macOS keychain.

A fake agent CLI for such a daemon goes on `PATH` from the throwaway `HOME`'s shell rc file (`.zshrc` when `$SHELL`
is zsh), never by prepending it to the `PATH` the daemon is started with. The daemon launches every agent with the
environment of `$SHELL -l -i -c env`, and on macOS that login shell's `/etc/zprofile` runs `path_helper`, which moves
the system paths (`/etc/paths`, `/etc/paths.d`, where Homebrew's `bin` usually sits) ahead of everything inherited. An
inherited prepend therefore loses to an installed agent of the same name, and the *real* CLI launches under the
throwaway `HOME` with nothing on screen saying so; the rc file is sourced after `/etc/zprofile`, so an
`export PATH=<fake bin>:$PATH` there wins. Confirm from the session's process (`ps`) which executable actually ran
before reading anything off the screen.

A probe that does need a logged-in agent cannot be isolated this way, because the agents need the real `$HOME` to find
their credentials and trust state. Expect it to create consoles and sessions in the user's real board and to run any
pending schema migration against the user's live database — copy that database aside first whenever the change being
probed touches the schema, and plan the cleanup as part of the probe rather than after the fact.

Clean up in the order the daemon enforces, and leave time between the steps: archiving a session returns as soon as its
record is written, while the agent process is still only being asked to exit, so deleting the console straight
afterwards is refused for "running sessions" even though every session was archived. Wait for those processes to be
gone, then delete — the refusal is the daemon working, not a defect.
