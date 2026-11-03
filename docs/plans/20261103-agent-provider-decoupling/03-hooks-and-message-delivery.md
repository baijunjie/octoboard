# 03 Hooks and Message Delivery

> Goal: Move how each provider's hook events are read, and its message delivery and launch-prompt quirks, behind the
> provider interface.
> Completion criteria:
> - Hook handling, message writing and reporting, and launch-prompt choice contain no branch on the agent kind.
> - The hook and delivery tests pass, with behavior unchanged.

## Technical design

- [ ] `AgentProvider::hooks() -> &dyn HookInterpreter`. Given a hook event, the interpreter answers:
  - status
  - turn key
  - submitted prompt
  - whether the turn was cancelled
  - the turn outcome
  - subagent start and stop
  - background tasks
  - whether a record watch is to be armed after the hook (the watch itself is milestone 04's)
- [ ] Delivery capabilities, declared by the provider:
  - whether a write is confirmed by the agent's report of the prompt it sent (and marked unsent when no report comes)
  - whether a report is held for a completion notice
  - whether a resume instruction goes as the launch prompt, to avoid a resume dialog

## Implementation plan

- [ ] Implement the interpreter for each existing provider from its current hook handling.
- [ ] Route hook handling and applying through the interpreter of the session's provider.
- [ ] Switch message writing, reporting and launch-prompt choice to the declared delivery capabilities.

## Notes for the developer

- **Development notes**:
  - The daemon's layering test keeps the completion-notice check below the hooks layer. Keep that dependency
    direction.
  - Read `docs/memory/` before moving logic.
- **Reference docs**:
  - `docs/structure/daemon.md`, section "Dependency direction", and the daemon's layering test
  - `docs/memory/`
