# Isolating agent CLI probes

Rules for keeping a probe of Claude Code, Codex or Grok Build, or of a live Octoboard session that launches them, off
the user's own configuration, accounts and Octoboard data, and for building the scratch homes and config directories
such a probe runs in so that they behave like the agent's own.

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

**A throwaway `HOME` does not contain everything either**: some residue lands machine-wide whatever `HOME` and `TMPDIR`
say, and one piece has to be prevented before the run rather than cleaned up after it. On macOS, Claude Code's first
TUI run registers the throwaway `HOME`'s own `Applications/Claude Code URL Handler.app` with LaunchServices as the
system-wide `claude-cli://` handler, and it stays registered after the scratch directory is deleted; so write
`"disableDeepLinkRegistration": "disable"` into the scratch config directory's `settings.json` before the first
launch, and remove a handler already registered with `lsregister -u <app path>`. A Codex TUI start in a scratch home
launches a managed `codex app-server` daemon from that home (parent pid 1) that keeps running after the TUI exits and
fills the home's `packages/` with a few hundred megabytes; find it with `ps` by the home's path and kill it when the
probe ends. The rest, with versions, is in the "Residue the probes leave outside the scratch directory" paragraph of
`docs/agent-cli-reference.md`'s "Sharing a configuration directory between accounts" section.

Use a dedicated scratch directory as cwd, never a shared one such as `/tmp`: each CLI records a per-directory trust
decision in the user's own config (Claude Code in the `.claude.json` of the config directory in effect, under
`projects[<path>].hasTrustDialogAccepted`), so probing directly in `/tmp` marks `/tmp` itself trusted and every later
probe run with `/tmp` as its own cwd silently starts out trusted, quietly invalidating any test of untrusted-workspace
behaviour. Below a trusted directory it depends on git (read out of the Claude Code 2.1.294 executable): a directory
that is not in a git repository is trusted through any trusted directory above it, without a trust screen and with its
own entry left at `false`, while inside a repository only directories up to the repository root count. So a fresh
scratch subdirectory of a trusted `/tmp` starts out trusted too unless it is a repository. Codex asks only inside a git
repository and records the trust for the repository's root, from a linked worktree the main checkout's (the
"Folder-trust confirmations" section of `docs/agent-cli-reference.md`), so a Codex probe run from this repository or any
worktree of it marks the user's own clone trusted in their `config.toml`; give a Codex trust probe a fresh `git init`
inside its scratch directory instead. Probes also leave session records behind in the user's agent directories.

A fresh scratch directory is by definition untrusted, and Claude Code stops there on its folder-trust dialog and
does nothing else — a probe that looks like it produced no output at all is usually sitting on that dialog. Grepping
the captured output for the dialog's words finds nothing either: as of Claude Code 2.1.289 its full-screen UI places
each word with a cursor-move sequence instead of printing spaces, so strip escape sequences *and* all whitespace before
matching any text in a capture. Unless untrusted behaviour is the thing being measured, answer the dialog once in that
directory before any measurement, with the keys the "Folder-trust confirmations" section of
`docs/agent-cli-reference.md` gives: an Enter on its own — a pasted line's trailing CR included — ends the session.

When several probes run concurrently, each needs its own distinct scratch directory, and expect to see the other
probes' trust entries and session records — that residue is not evidence of a defect.

## Ask the user before a probe runs a login flow that can open their browser

Applies to any probe of an agent's sign-in — `claude auth login`, `codex login`, `grok login` — including one against a
scratch config directory, **and to any Grok Build run at all whose home holds no login**, since Grok starts the same
sign-in inline on a plain run. The scratch directory isolates only the credential written on disk; the authorisation
runs in the user's own default browser, which is usually already signed in to the vendor, so the flow can complete
against their real account with nobody approving it (a Grok Build login did, in about five seconds), leave a
server-side session that deleting the scratch credential does not revoke, and put pages up on their screen.

So ask first, naming the agents and saying that pages will open, and leave the probe undone if the answer is no. Without
that, read the authorisation URL only through routes that open nothing: Claude Code with `BROWSER` set to a script that
records its argument; Codex through `codex app-server`'s `account/login/start` or `codex login --device-auth`. Grok
Build and plain `codex login` open the browser themselves and nothing found stops them (`docs/agent-cli-reference.md`,
"Signing in a config directory without a terminal"), so they have no such route. A login that has to complete is done
by the user, against an account they choose. If one completes anyway, tell the user which account it signed in, so they
can revoke the session on the vendor's side.

**For Grok Build, a throwaway `GROK_HOME` and `HOME` with no `auth.json` do not keep a run off the user's account**
(Grok Build 1.0.62, macOS). A first run there, with no login intended, ran the inline sign-in through the already
signed-in browser and wrote a real token into the scratch home within seconds, with nothing in the probe's own output;
a later run with `auth.json` emptied to `{}` still authenticated with a session token from somewhere outside both
directories (source not found). A probe that must not touch the account sets `GROK_CODE_XAI_API_KEY` to a fake key,
which makes Grok select API-key auth instead of any cached token, and points the model it selects with `--model` at a
per-model `[model.<name>]` entry in the scratch `config.toml` carrying a `base_url` on `127.0.0.1` and a fake
`api_key`, so inference never leaves the machine. Confirm afterwards from `$GROK_HOME/logs/unified.jsonl`, not from
the terminal: its `auth method selection` record should name the API key with `has_cached_token` false, and no
`auth: inline auth flow` or `cached_token` record should appear.

## A probe that goes through the daemon runs against the user's live Octoboard data unless its `HOME` is a throwaway one

The daemon derives its data directory, its database and its single-instance lock from `$HOME`, and writes its port file
to `$TMPDIR`. So a probe that does not need a logged-in agent — the protocol, the terminal socket, the window's
connection — runs the daemon, or a copy of the app, with a throwaway `HOME` and `TMPDIR`: the user's board is untouched
and it runs alongside their own Octoboard. For a session that draws real output there without a model turn, open a
Codex session: with no login under that `HOME` it stops at its sign-in screen.

**Drive it from a client of its own on the control WebSocket, and build no application for it.** The daemon on its own,
plus a small client on the control WebSocket (the "`GET /ws/control` — management and status" section of
`apps/daemon/PROTOCOL.md`), connecting with the key from the daemon's stdout startup line since the port file holds no
key, to create the consoles, projects and sessions, read the events and send the answers, is the whole instrument
whenever what differs between agents is on the agent's side: the window's rendering of a dialog, and answering it, are
agent-independent, so once they have been exercised for one agent, confirming the behaviour for the next needs no
window, no packaged build and none of the screen-recording or accessibility permissions that driving a window needs.
Build the application only for behaviour the Tauri shell itself carries.

Launch it from an emptied environment, `env -i HOME=<dir> TMPDIR=<dir> … /usr/bin/open -n <app>`, never with
`open --env` overrides alone: `open` hands the app the calling shell's whole environment besides the variables it
names, so a `CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `GROK_HOME` exported there — an agent session exports
`CLAUDE_CONFIG_DIR` routinely — points the run's agents at the user's own configuration despite the throwaway `HOME`,
and for Codex and Grok Build at the login kept there too. The same goes for a daemon started straight from the shell.

A throwaway `HOME` hides Claude Code's keychain login as well. When a run needs it logged in, symlink
`<throwaway HOME>/Library/Keychains` to the real `~/Library/Keychains`, which brings back every account's login
without copying a credential. **Leave `CLAUDE_CONFIG_DIR` unset in such a run, and never set it to repair a login that
looks broken**: setting it costs Claude Code the login whatever directory it names, while unset puts the config
directory inside the throwaway `HOME`, where the probe owns it and can write its own `settings.json`. Codex is not
symmetric here — it stays logged in with `CODEX_HOME` pointed at the user's real `~/.codex` — so the arrangement that
worked for a Codex probe is the wrong one to carry over (the measurements are in the "Moving a conversation to another
config directory" section of `docs/agent-cli-reference.md`).

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
one. Expect it to create consoles and sessions in the user's real board. When the build being probed has a newer
schema version than the user's live database, opening that database moves it aside and starts an empty one at the
newer version, and the user's own build then refuses to open the empty one. So copy that database aside first whenever
the change being probed touches the schema, put it back during cleanup, and plan the cleanup as part of the probe
rather than after the fact.

Clean up in the order the daemon enforces, and leave time between the steps: archiving a session returns as soon as its
record is written, while the agent process is still only being asked to exit, so deleting the console straight
afterwards is refused for "running sessions" even though every session was archived. Wait for those processes to be
gone, then delete — the refusal is the daemon working, not a defect.

## Give a scratch daemon a Grok home in the state Grok itself leaves one

Applies to a probe that runs the daemon over a hand-built Grok home — a scratch `GROK_HOME` or account directory
(measured on Grok Build 1.0.62).

**Initialise the home before taking any baseline of it.** The daemon reads every account's quota by running the
agent's own CLI against the account's directory, the first time within about 15 seconds of its start and with no
session involved (`docs/product/account-quotas.md`, "How a quota is read" and "When quotas are read"). For Grok that is
`grok agent --no-leader stdio`, and its first run in a home never run before initialises it: `README.md`, `docs/`,
`installed-plugins/`, and `[marketplace] default_skills_installs_purged = true` appended to `config.toml`, following a
symlink into the file it points at. A probe asserting that a shared `config.toml` comes through a session untouched
then sees a change no session made. Run Grok in the home once beforehand, or pre-seed that marker line.

**Seed Grok's trust store in Grok's own format, or not at all** (the format is in the "Folder-trust confirmations"
section of `docs/agent-cli-reference.md`). A store Grok cannot read leaves every folder untrusted, so a session whose
directory holds an `AGENTS.md` — every console session's does — gets the trust prompt, the daemon presses `y`, and Grok
fails to save the trust and quits about 0.3 s after start, which reads exactly like a daemon defect. Left out, the
store is created by that press. When a session in such a run ends right after it starts, read its final screen in the
daemon's saved output, `<HOME>/.octoboard/output/<session id>`, before suspecting the daemon.
