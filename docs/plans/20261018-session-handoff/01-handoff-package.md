# 01 Handoff package

> Goal: from a session, build by program alone the package a receiving session reads to pick the work up.
> Completion criteria: for a sample record of each of Claude Code, Codex and Grok Build, the package holds the user's
> messages, the model's replies, the latest todo list and the files changed, and no tool output; for a project
> session responsible for branches, it holds each branch's worktree path, head and changes against its base. Verified
> by unit tests over sample records and a test repository.

## Technical design

- [ ] A per-agent record reader that turns an agent's conversation record into one common sequence of entries: the
      user's messages, the model's replies, todo list updates, and file changes (from the edit and write tool calls).
      Tool results and file contents read are dropped.
- [ ] The handoff package, made of:
  - the original session's identity: id, title, agent, account, project or console, and its brief;
  - the path of the original conversation record;
  - the filtered conversation from the reader;
  - the latest todo list;
  - the files changed in the conversation;
  - for a project session, each branch it is responsible for: the worktree it is checked out in, its head commit,
    its commits and changed files against the branch it was cut from, and whether the worktree has uncommitted
    changes;
  - the sessions it owns, with their titles and states.
- [ ] The package is written to a file in Octoboard's own data directory, as text a model reads without knowing any
      agent's record format. It is removed with the receiving session when that session is deleted.

## Implementation plan

- [ ] Readers for Claude Code, Codex and Grok Build, each tested against a sample record of that agent.
- [ ] The git state of the responsible branches, read from the project's repository.
- [ ] Assembly of the package from the readers, the git state and the session's own records, then writing it out.

## Notes for the developer

- **Reusable capabilities**: the record lookup used for session usage and titles, which already finds each agent's
  record for a session (including Codex's compressed rollouts, if the shared-account-sessions plan has landed); the
  responsible-session records of branches; the project pane's Git mode reading of branches and changes.
- **Development notes**: reading only — nothing is written to the agent's directories or the user's repository.
  An unreadable or missing record yields a package without the conversation, not a failure.
- **Reference docs**: `docs/agent-cli-reference.md`, `docs/product/session-usage.md`,
  `docs/product/branch-responsible-session.md`.
