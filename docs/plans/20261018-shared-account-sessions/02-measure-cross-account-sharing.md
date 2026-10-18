# 02 Measure cross-account sharing

> Goal: establish, by measurement against real signed-in accounts, whether a conversation recorded under one account
> resumes correctly under another, and how each agent behaves over a linked session directory.
> Completion criteria: each question below is answered with a measured result, recorded in
> `docs/agent-cli-reference.md`; any answer that makes sharing unsafe is reported to the user before milestone 3
> starts.

## Implementation plan

- [ ] Cross-account resume with two real signed-in accounts (ideally in different organisations): Claude Code
      transcripts containing signed thinking blocks; Codex rollouts containing encrypted reasoning items. If this
      fails, neither copying nor sharing gives a usable cross-account resume — report before going on.
- [ ] Codex over a linked `sessions/`: home B resuming a rollout home A compressed; whether one home's compression
      touches a rollout another home is actively writing; each home's database after the other home's maintenance.
- [ ] Codex over a linked `sessions/`: a home with no database row for a thread resuming it while subagent rollouts
      `…-<id>_<child>` sit in the shared directory — does it open the parent or a subagent?
- [ ] Claude Code over a linked `projects/`: whether one account's `cleanupPeriodDays` prunes the other's
      conversations.
- [ ] Grok Build: resuming under account B a session created by account A, given its remote session registry.
- [ ] A terminal agent under the other account resuming the same conversation while an Octoboard session runs it.

## Notes for the developer

- **Development notes**: measure in scratch config directories linked to copies, never against the user's real
  session stores; the agents' versions are recorded with each result.
- **Reference docs**: `docs/agent-cli-reference.md`.
