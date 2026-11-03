# 05 Screen Reading

> Goal: Move how each provider's terminal screens are recognized behind the provider interface.
> Completion criteria:
> - Session screen handling contains no branch on the agent kind.
> - The dialog, input-box, trust and interrupt tests pass, with behavior unchanged.

## Technical design

- [ ] `AgentProvider::screen() -> &dyn ScreenReader`. It covers:
  - the dialog recognition table
  - the input box, its stash and draft
  - the trust prompt
  - the terminal title and title activity, and interrupting by title
  - interrupting a turn
  - the swap screen

## Implementation plan

- [ ] Implement the screen reader for each existing provider from its current screen recognition.
- [ ] Route these through the session's provider:
  - session dialogs and the trust prompt
  - the input box, stash and draft
  - title activity and interrupting

## Notes for the developer

- **Reference docs**: `docs/agent-cli-reference.md`; `docs/memory/`.
