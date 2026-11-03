# 04 Records and History

> Goal: Move each provider's records, transcripts and history knowledge, and its record watchers, behind the provider
> interface.
> Completion criteria:
> - The records modules and the record watching contain no branch on the agent kind.
> - The records, history, resumability and account-switch tests pass, with behavior unchanged.

## Technical design

- [ ] `AgentProvider::records() -> &dyn RecordStore`, grown from the records layout descriptor. It covers:
  - locating a session's index entry and record paths
  - reading the context fold and the observed model and effort
  - reading conversation and history
  - judging resumability
  - removing a session's records
  - the files carried over on an account switch, and continuing after a switch
  - the record watcher (transcript or rollout follower) the provider uses, which milestone 03's hook decision arms

## Implementation plan

- [ ] Implement the record store for each existing provider from its current records code, and move its watcher
      behind it.
- [ ] Route these through the session's provider:
  - record location and collection
  - context, conversation and history
  - resumability and removal
  - carried files and record watching

## Notes for the developer

- **Reusable capabilities**:
  - the records layout descriptor (moved into the provider descriptor in milestone 02)
  - the existing per-provider transcript and rollout watchers
- **Reference docs**: `docs/structure/daemon.md`; `docs/agent-cli-reference.md`.
