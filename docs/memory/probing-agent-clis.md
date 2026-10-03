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
the "Known pitfalls of the Tauri / Rust approach" section of `docs/mvp.md`.

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

## A probe whose expected answer is "no" needs a positive control in the same run

When the question is "does the agent read file X" and the result comes back negative, that negative is evidence only
if the same run also shows the agent reading something you already know it reads. Discovery is gated on preconditions
you may not have thought to control for — Grok locates a project by walking up for a `.git` directory and reads no
project instructions without one, and folder trust and server-side feature flags gate other agents' discovery the same
way — and each of them turns a whole matrix negative in a way indistinguishable from the thing being measured.

## A probe is not read-only: give it its own scratch directory

Running these CLIs writes into the user's real configuration. The product's rule against touching the user's global
agent configuration does not extend to probes, and a probe cannot avoid it, so plan for the residue instead.

Use a dedicated scratch directory as cwd, never a shared one such as `/tmp`: each CLI records a per-directory trust
decision in the user's own config (Claude Code in `~/.claude.json` under `projects[<path>].hasTrustDialogAccepted`),
so probing directly in `/tmp` marks all of `/tmp` trusted and every later probe there silently starts out trusted,
quietly invalidating any test of untrusted-workspace behaviour. Probes also leave session records behind in the user's
agent directories.

When several probes run concurrently, each needs its own distinct scratch directory, and expect to see the other
probes' trust entries and session records — that residue is not evidence of a defect.
