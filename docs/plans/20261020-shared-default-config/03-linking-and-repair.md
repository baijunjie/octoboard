# 03 Linking, restoring and launch-time repair

> Goal: the daemon keeps each account's sharing choice and makes the account's directory match it.
> Completion criteria: daemon tests cover turning the switch on (configuration backed up then linked, conversation
> history moved into the default directory then linked, a missing directory created in the default directory, a missing
> file skipped), turning it off (links removed, backups restored, conversation history left in the default directory),
> a change recorded while a session of the account runs and carried out only once none runs, a replaced file link
> repaired before a launch or resume, the default account refused, and the choice surviving a daemon restart; the
> protocol exposes reading and changing it.

## Technical design

- [ ] A per-account setting: whether sharing is on, and whether its directory has been brought in line with it yet.
- [ ] Protocol requests to read and change the setting, rejected for the default account.
- [ ] Turning on and turning off, as set out in the overview, over the agent's catalog (01).
- [ ] Before a session of the account starts or resumes: a pending change is carried out if none of the account's
      sessions is running, then every file item that is no longer a link is backed up and linked again.

## Implementation plan

- [ ] Store the setting with the account, then add the turn-on and turn-off operations.
- [ ] Hook the pending change and the repair into the launch and resume path of every agent, a switch to the account
      included.

## Handoff

- A shared Codex state database names a rollout by whichever account's path wrote it — Codex records an absolute path
  through the running account's `sessions/` link, and an account switch leaves a row alone when it already names the
  carried rollout by another account's route. Turning an account's sharing off replaces its `sessions/` link with an
  empty directory, so every row naming a rollout through that account's link would then name a missing file for the
  accounts still sharing. Turning off has to account for this (for example by rewriting such rows to the default
  directory's path), and a test should cover it.

## Notes for the developer

- **Reusable capabilities**: the account store and its protocol requests; the launch path every adapter shares; the
  record of which sessions are running under which account.
- **Development notes**: an operation that fails half-way reports which items it left in which state rather than
  rolling back silently; the protocol contract is updated with the new requests and errors.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/product/agent-accounts.md`.
