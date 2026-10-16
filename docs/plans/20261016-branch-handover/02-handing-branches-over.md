# Handing branches over with `request_session`

> Goal: a session asking for a new session can hand it the branches it is responsible for, with the user's approval.
> Completion criteria: `request_session` accepts an optional `branches` list; branches the requester is not responsible
> for, or that are not in the repository of the new session's project, are refused before the user is asked; the
> confirmation dialog names the branches; on approval their responsible session becomes the new session, and on refusal,
> time-out or withdrawal nothing moves; leaving `branches` out behaves exactly as today. Verified by daemon
> `request_session` tests, a dialog test, and a gallery scenario showing the branches in the dialog.

## Technical design

- [ ] `request_session` gains an optional `branches` argument: a list of local branch names in the new session's
      project's repository.
- [ ] Validation before asking, alongside the existing refusals: each branch exists, the requester is its responsible
      session, and its repository is the new session's project's; a `console` request with branches is refused.
- [ ] The waiting request, and the `session_request` event, carry the validated branch list.
- [ ] Approval starts the session as today, then moves the listed branches' responsible session to it with the
      compare-and-set move from milestone 1; a branch whose responsible session changed meanwhile is reported in the
      outcome rather than moved.
- [ ] The outcome note and the new session's opening paragraph name the branches handed over.
- [ ] The dialog adds the branches to its sentence, each as code; with none, the sentence is as today.

## Implementation plan

- [ ] Extend argument reading and the pre-ask checks for `request_session`.
- [ ] Carry the branch list through the request, the event and the UI store.
- [ ] Do the move after the session has started, and report partial moves in the outcome.
- [ ] Update the dialog copy in every catalogue language and its test, and the tool description telling agents when
      to pass `branches` (handing over the work on a branch) and when not to (working on the checked-out branch).
- [ ] Add the gallery scenario through the gallery's own skill.

## Notes for the developer

- **Reusable capabilities**: the `request_session` approval machinery and its pre-ask checks; the responsible-session
  record and move from milestone 1; the UI's code rendering for technical literals.
- **Development notes**: the dialog shows only what Octoboard knows, never text the agent wrote; branch names are
  checked values, not agent prose. Branch names are technical literals and follow the code-and-direction rules for them.
- **Reference docs**: `docs/product/session-requests.md`, `docs/product/branch-creator.md`,
  `docs/ui-accessibility.md` (technical literals in prose), `.claude/skills/knowledge-i18n-copy`.
