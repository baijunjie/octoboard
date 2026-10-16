# Responsible session for a branch

> Goal: every branch with a creator also has a responsible session, and the Git mode follows the selected session's
> responsible branches.
> Completion criteria: a branch a session creates records that session as both creator and responsible session; the
> responsible session can be moved to another session without touching the creator; selecting a session in the sidebar
> sets the Git mode's selectors from the branch it most recently became responsible for; existing creator behaviour and
> the creator badge are unchanged. Verified by daemon store and attribution tests and the Git mode's following tests.

## Technical design

- [ ] A responsible-session record per branch of a project's repository, kept beside the creator record, with the time
      the session became responsible, so "most recently became responsible" can be ordered.
- [ ] Recording a creator also records the same session as responsible, at the same time.
- [ ] A store operation that moves a set of branches' responsible session from one session to another, atomically, and
      only where the current responsible session is the expected one.
- [ ] The daemon's state the UI reads carries each branch's responsible session next to its creator.
- [ ] What clears or keeps the creator record (deleting a session, removing a project) does the same to the
      responsible record.

## Implementation plan

- [ ] Add the record and its migration; existing branches with a creator get their creator as responsible session.
- [ ] Extend creator attribution to write the responsible session in the same step.
- [ ] Add the move operation with its compare-and-set condition and tests.
- [ ] Switch the Git mode's "follow the selected session" from the newest created branch to the newest responsible
      branch; the worktree, From and To rules otherwise stay as they are.

## Notes for the developer

- **Reusable capabilities**: the branch-creator attribution (git `reference-transaction` hook reporting under the
  session the agent was started for) and its storage; the Git mode's following of the selected session; the store's
  compare-and-set pattern used for session bindings.
- **Development notes**: a creator is never inferred and never wrong; the responsible session must keep the same bar —
  it is only ever set by attribution or by an explicit handover.
- **Reference docs**: `docs/product/branch-creator.md`, `docs/product/project-pane-git-mode.md` ("Following the
  selected session"), `docs/memory/writing-daemon-code.md`.
