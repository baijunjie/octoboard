> Severity: Moderate

## Symptom

At an agent launch the daemon reads Claude Code's settings files, Claude Code's global config file and Codex's
`config.toml` with a plain blocking `std::fs::read_to_string`, so a FIFO at one of those paths blocks the read and an
oversized file is read without bound.

## Reproduction steps

1. In a project associated with Octoboard, replace `.claude/settings.local.json` with a FIFO:
   `rm -f .claude/settings.local.json && mkfifo .claude/settings.local.json`.
2. Start a Claude Code session in that project.
3. Observe whether the session leaves `starting`.

The same applies to `.claude/settings.json` in the project, to `.claude.json` in Claude Code's config directory in
effect (or `~/.claude.json`), and, for a Codex session, to `config.toml` in the account's Codex home.

## Expected vs. actual

- Expected: a file inside a project is read only through `browse::live`, and a file a launch reads outside the project
  is opened without blocking, must be a regular file, and is read with a bound; a FIFO or an oversized file counts as
  unreadable and the launch goes on (`docs/memory/handling-paths-and-files-in-the-daemon.md`, "Read a file inside a
  project only through `browse::live`, and one a launch reads outside it without blocking").
- Actual: the reads below use `std::fs::read_to_string`, which blocks on a FIFO until a writer opens it and has no size
  limit.

## Environment

macOS, `main` as of `cd4e194bd` and branch `feat/claude-compat`. Precondition: one of the paths above is a FIFO, a
link to a device such as `/dev/zero`, or a very large file.

## Scope of impact

Any user whose project, Claude Code config directory or Codex home holds such a file at one of those paths; a project
can contain one through a cloned repository. Workaround: replace the file with a regular one.

## Leads

- Verified: `apps/daemon/src/adapter/claude/settings.rs` — `user_status_line` and `http_hooks_allowed` read the files
  from `own_settings_files` / `settings_files` (the user's and the project's `.claude/settings*.json`) with
  `std::fs::read_to_string`; `untrusted_workspace_notice` reads `global_config_file(...)` the same way.
- Verified: `apps/daemon/src/adapter/codex.rs` — `read_config` reads the Codex home's `config.toml` with
  `std::fs::read_to_string`.
- Verified: `std::fs::read_to_string` on a FIFO with no writer was still blocked after 3 s (a scratch Rust program on
  this machine).
- Verified: `apps/daemon/src/adapter/files.rs` `read_bounded_text` already reads with `browse::live` inside the
  project and a non-blocking, bounded regular-file read outside it.
- Inferred, not run: steps 1–3 leave the Claude Code session stuck in `starting`, since the launch performs these reads
  before spawning the agent.

## Acceptance criteria

- [ ] No read of an agent's settings or config file during a launch uses blocking `std::fs::read_to_string` or
      `File::open`; files inside the project go through `browse::live`.
- [ ] With a FIFO at the project's `.claude/settings.local.json`, a Claude Code session starts.
- [ ] With a FIFO at the Codex home's `config.toml`, a Codex session's launch does not block.
- [ ] A settings or config file above its size bound is treated as unreadable, and the launch goes on.
- [ ] Tests cover a FIFO and an oversized file for these reads.
