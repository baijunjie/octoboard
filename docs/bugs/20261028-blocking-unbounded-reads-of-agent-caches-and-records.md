> Severity: Moderate

## Symptom

Outside the settings and config reads at an agent launch, the daemon still reads some files an agent keeps — Codex's
and Grok's `models_cache.json`, Claude Code's `package.json`, and Grok's shared `config.toml` at a session's exit —
with plain `std::fs::read` / `std::fs::read_to_string`, so a FIFO at one of those paths blocks the read and an
oversized file is read without bound.

## Reproduction steps

1. Codex default effort: in the Codex home in effect (`~/.codex` or the account's directory), replace the catalogue
   with a FIFO: `rm -f models_cache.json && mkfifo models_cache.json`. Run a Codex session whose records show a model
   and no explicit effort, so the daemon looks the session's default effort up. Observe whether the session's record
   updates keep arriving.
2. Grok model list: in the Grok home in effect (`~/.grok` or the account's directory), replace `models_cache.json`
   with a FIFO the same way, then open the model choice for a Grok launch so the daemon lists Grok's models. Observe
   whether the list arrives.
3. Claude Code version: with Claude Code installed from npm (the binary resolves into a `claude-code` directory under
   `node_modules`), replace that directory's `package.json` with a FIFO, then start a Claude Code session. Observe
   whether the session leaves `starting`.
4. Grok writeback: make the account's Grok `config.toml` a symlink to a shared file, start a Grok session, replace the
   shared file with a FIFO while the session runs (`rm -f <shared> && mkfifo <shared>`), then end the session.
   Observe whether the session's teardown completes.

For the size bound, put a file of several hundred MiB at the same paths instead of a FIFO.

## Expected vs. actual

- Expected: a file the daemon reads from an agent's own directories, or from an agent's install, is opened without
  blocking, must be a regular file, and is read within a bound; a FIFO or an oversized file counts as unreadable and
  the work goes on (`docs/memory/handling-paths-and-files-in-the-daemon.md`, "Read a project's files only through
  `browse::live`, and the agent's own configuration without blocking").
- Actual: the reads below use `std::fs::read` / `std::fs::read_to_string`, which block on a FIFO until a writer opens
  it and have no size limit.

## Environment

macOS, `main` as of `1cec2bfa0`. Precondition: one of the paths above is a FIFO, a link to a device such as
`/dev/zero`, or a very large file.

## Scope of impact

Any user whose Codex home, Grok home, Claude Code install or shared Grok config holds such a file at one of those
paths. Workaround: replace the file with a regular one.

## Leads

- Verified: `apps/daemon/src/default_effort.rs` `Catalogue::refresh` checks `metadata.len()` against
  `JSON_FILE_LIMIT` and then reads the file with `std::fs::read`; a FIFO's metadata reports length 0, so the check
  passes and the read blocks.
- Verified: `apps/daemon/src/agent_models.rs` `list` reads Grok's `models_cache.json` with `std::fs::read_to_string`
  after running `grok models`.
- Verified: `apps/daemon/src/adapter/claude/mod.rs` `installed_version` reads the `package.json` of the `claude-code`
  directory with `std::fs::read_to_string`; `apps/daemon/src/term.rs` calls it while building the launch spec, before
  `plan`.
- Verified: `apps/daemon/src/adapter/grok.rs` `SharedConfigWriteback::publish_locked` reads the session's config copy
  and the shared target with `std::fs::read_to_string`.
- Verified: `apps/daemon/src/adapter/files.rs` `read_regular_text` reads a file without blocking, as a regular file
  only, within a bound.
- Inferred, not run: step 1 stalls whatever drives `DefaultEfforts::of` (the record collection for Codex sessions);
  step 2 stalls the model listing; step 3 holds the launch; step 4 holds the session's teardown.

## Acceptance criteria

- [ ] None of the four reads above uses blocking `std::fs::read`, `std::fs::read_to_string` or `File::open`.
- [ ] With a FIFO at the Codex home's `models_cache.json`, a Codex session's default effort lookup returns without
      blocking.
- [ ] With a FIFO at the Grok home's `models_cache.json`, listing Grok's models returns without blocking.
- [ ] With a FIFO at the Claude Code install's `package.json`, a Claude Code session starts.
- [ ] With a FIFO at the shared Grok `config.toml`, a Grok session's exit completes without blocking.
- [ ] Each of these files above its size bound is treated as unreadable, and the work goes on.
- [ ] Tests cover a FIFO and an oversized file for these reads.
