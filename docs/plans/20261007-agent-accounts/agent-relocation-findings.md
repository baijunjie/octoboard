# Conversation relocation: what was measured

Reference for milestone 5, not a milestone of its own: the measurement is done, and
[05 — Switching a session's account](05-switching-a-sessions-account.md) is what acts on it.

Measured on macOS against **Claude Code 2.1.289**, **Codex 0.160.0** and **Grok Build 1.0.46**. Each result carried
a control in the same run, named below. All three agents rewrite their on-disk layout on upgrade, so re-measure
against the installed versions before relying on any of this.

## All three agents support the switch

For each agent, **the session's own conversation record alone is sufficient**: copied into a second config directory,
the agent resumes the conversation there with its full content. No index, registry or per-project entry has to be
copied with it, even though all three keep one.

| Agent | What to copy | Where it goes |
|---|---|---|
| Claude Code | `<session id>.jsonl` | `<config dir>/projects/<cwd slug>/` |
| Codex | `rollout-<timestamp>-<thread id>.jsonl` | anywhere under `<home>/sessions/` |
| Grok Build | the `<session id>/` directory, whole | `<home>/sessions/<url-encoded cwd>/` |

**Neither the Claude slug rule nor Grok's encoding has to be reimplemented.** The record's path *relative to the
config directory* is what matters, and the source directory already holds it: copy from
`<source>/<relative path>` to `<target>/<same relative path>`, creating the intermediate directories. Deriving the
slug or the encoding independently would be a second implementation of something the agent already wrote down, and
neither rule was measured here.

For Grok, `<home>` is the **account's directory** — the source home Octoboard builds a session's private home from,
not the private home itself, which is discarded with the process.

### Claude Code

The record copied alone into a project-slug directory that had never held it resumed with the conversation intact —
the control word planted in the original conversation came back. The global config file's `projects` map was empty in
the target directory and nothing was needed there, so `.claude.json` is not part of a resume's lookup.

Session resolution happens **before** the login check: an unauthenticated config directory asked to resume a session
it does not hold answers `No conversation found with session ID: <id>`, and the same directory with the record
copied in answers `Not logged in · Please run /login` instead. So the two failures are distinguishable, and a
relocation can be verified without a second login.

### Codex

The record copied into a fresh home at a date path unrelated to its own (`sessions/2099/01/01/`) resumed with the
conversation intact. The date directories are how Codex writes, not how it looks up, and `session_index.jsonl` is
not consulted. Control: a thread id with no record in the home answers
`thread/resume: thread/resume failed: no rollout found for thread id <id>`.

As with Claude Code, resolution precedes the login check.

### Grok Build

The session directory copied under a different working directory's encoded name resumed with the conversation
intact. `sessions/session_search.sqlite` is not consulted.

Two Grok-specific findings:

- **The login check comes first**, unlike the other two: an unauthenticated home refuses with `Not signed in` before
  it looks for the session, so a relocation cannot be verified without a login, and a failed switch cannot be told
  apart from a login problem by the message alone.
- **A session missing locally is fetched from a remote registry**: the control printed
  `Session "<id>" not found locally, restoring conversation from remote...` and then failed with a 404. So a switch
  between two homes of the *same* account might not need a copy at all. The switch should copy regardless, since
  across two different accounts the registry is the other account's and will not serve the session.

## Switching carries the session into the target account's whole setup

A config directory holds the agent's **global configuration**, not only its login. Measured: the same Codex thread
resumed in a fresh home came up with that home's own defaults — `reasoning effort: none` where the original had
`medium` — because `config.toml` lives in the home. Claude Code's `settings.json` and Grok's `config.toml` are in the
same position. Milestone 5 is where this gets written into the product docs.

## Why a second config directory really is a second account

The premise the whole feature rests on.

**Measured**: Claude Code keeps the credential in the macOS Keychain under a service name derived from the config
directory, so two directories hold two independent logins. Pointing `CLAUDE_CONFIG_DIR` at a fresh directory reports
`Not logged in` while the default directory is logged in, and copying the logged-in directory's own account record
into the fresh one does not change that — which rules out the alternative reading, that only the account record was
missing while one shared token sat behind it.

**Read out of the 2.1.289 executable, not measured, and nothing in the plan acts on it**: the service name appears to
be `Claude Code-credentials` with `-<first 8 hex of sha256(config directory)>` appended when `CLAUDE_CONFIG_DIR` is
set and nothing appended when it is not. Background only — it says why the measurement came out as it did.

**Inferred from the file layout, not measured**: Codex and Grok Build keep the login in a file inside the directory
(`auth.json` in both), so a second directory is a second login for them by construction.

## Not established here

Whether **Codex** creates its home directory when pointed at one that does not exist. The run above created the
target directory by copying into it, so nothing was measured; the plan infers it from Codex keeping its login, its
configuration and its sessions all inside that directory. Claude Code was measured to create it.

## A trap to guard against

The failure mode to watch for is an agent that resumes *successfully* into an empty conversation. None of the three
did that here — each refused distinctly — but it is what a future version could introduce, and it would look like a
successful switch.
