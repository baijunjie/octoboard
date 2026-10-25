# Complete the session record

> Goal: an agent can read every fact a write tool depends on about a session.
> Completion criteria: `get_session` and `list_sessions` records show the session's account, observed model and
> effort, `pinned` and `locked`, and the branches a session is responsible for are readable; tests cover each field for
> each caller role; the tool descriptions and product docs say what is returned.

## Technical design

- [ ] Session record gains: account (id and display name, never its config directory), observed model, observed
      effort, `pinned`, `locked`.
- [ ] The branches a session is responsible for, with their worktree state, become readable for any session
      (including the caller itself), in the form settled under "Key design decisions" in the overview.

## Implementation plan

- [ ] Extend the record builder shared by `get_session` and `list_sessions` with the new fields.
- [ ] Expose responsible branches by calling the existing branch inspection the app uses.
- [ ] Update the tool descriptions so callers know `switch_session_account`, archiving and `handover_branches` can be
      checked beforehand.

## Notes for the developer

- **Reusable capabilities**: the control protocol's `inspect_session_branches` request; the session's stored
  `account_id`, `observed_model` and `observed_effort`.
- **Development notes**: reads are open to every session; keep it that way. Do not leak account directories.
- **Reference docs**: `apps/daemon/PROTOCOL.md`, `docs/product/hub-orchestration.md`,
  `docs/product/branch-responsible-session.md`, `docs/product/account-quotas.md`, `docs/memory/writing-daemon-code.md`.
