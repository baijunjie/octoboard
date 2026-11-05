> Severity: Minor

# Grok Build's Unix system configuration directory is read on Windows

## Symptom

On a Windows daemon host, a Grok Build launch reads the sandbox profile from `\etc\grok` on the daemon's current drive,
a directory Grok Build itself never loads there, so a file in it can refuse a launch Grok would have run.

## Reproduction steps

1. On a Windows daemon host with Grok Build available, create `C:\etc\grok\requirements.toml` (on the drive the daemon's
   current directory is on) holding `[sandbox]` with `profile = "workspace-write"` (any profile other than `off`).
2. Make sure nothing else sets a sandbox profile: no `GROK_SANDBOX` in the launch environment, none in the source home's
   `config.toml`.
3. Start a new Grok Build session in Octoboard.
4. Run `grok inspect` in the same directory for comparison.

## Expected vs. actual

- Expected: the launch goes ahead, since Grok Build has no system-wide configuration directory on Windows and does not
  load that file (`docs/agent-cli-reference.md`, "Grok Build has no system-wide configuration directory on Windows");
  the refusal is for "a Grok Build launch that would run under Grok's own sandbox"
  (`docs/product/per-agent-launch-specifics.md`).
- Actual: Unknown — not run. From the code (see Leads), the launch is expected to be refused with
  `provider.grok.sandbox_unsupported`.

## Environment

- Windows daemon host; Grok Build 1.0.77 (the version whose `grok inspect` was measured not loading `C:\etc\grok`).
- Octoboard branch `feat/windows-port` at 122c7b186.
- Precondition: a `requirements.toml` or `managed_config.toml` under `\etc\grok` on the daemon's current drive, which no
  Windows installation creates by itself.

## Scope of impact

- Grok Build users on Windows with a file at that unusual path; nobody else.
- Workaround: remove or rename `\etc\grok`.

## Leads

- Verified: `apps/daemon/src/adapter/grok/mod.rs` defines `SYSTEM_GROK_DIR = "/etc/grok"` and `plan` passes it to
  `plan_under` on every host, with no platform condition; a path starting with `/` on Windows names that directory on the
  current drive.
- Verified (documented measurement): Grok Build 1.0.77's Windows implementation has no system configuration directory,
  and `grok inspect` does not load a `C:\etc\grok` candidate (`docs/agent-cli-reference.md`).
- Inferred: `/etc/grok/managed_config.toml`'s fleet default is read the same way and would refuse the launch the same way.

## Acceptance criteria

- [ ] On a Windows host, a `requirements.toml` or `managed_config.toml` under `\etc\grok` on the current drive does not
      refuse a Grok Build launch.
- [ ] On macOS and Linux, `/etc/grok/requirements.toml` and `/etc/grok/managed_config.toml` are read as before.
- [ ] `docs/product/per-agent-launch-specifics.md` says which hosts read the system-wide files.
