# Provider-neutral rule

> Goal: a session that writes code involving agent differences meets the provider-neutral rule before designing it,
> and knows what to do when the difference it needs has no capability or interface method yet.
> Completion criteria: the rule has a prominent home in both structure docs with a counterexample and the steps for a
> new difference; the development memory no longer reads as permission to special-case in place; the change and UI
> checkers name the rule as a required check; the instruction file's section points at the new home.

## Implementation plan

- [ ] Give the rule its own subsection (or the first bullet) of "Where new code goes" in `docs/structure/daemon.md` and
      `docs/structure/ui.md`, with a short counterexample (a branch on the agent id in generic code) and its fix.
- [ ] Add the steps for a new per-agent difference: declare a capability on the descriptor, or add a method to the
      provider's interface and implement it in each adapter; in the UI, read the served capability and keep
      provider-named copy and codes in the provider's presentation module.
- [ ] Rewrite the opening of the development memory on building a feature for several agents so that it first says any
      agent-specific handling lives in the provider, then when such handling is justified.
- [ ] Add the rule as a required check to the change checker's and UI checker's definitions, including a search of the
      diff for agent ids and agent-specific literals in generic code.
- [ ] Update the "Provider-neutral core" section of `CLAUDE.md` if the structure docs' heading changes.

## Notes for the developer

- Development notes: the structure docs are maintained by `map-writer`, the development memory by `memory-writer`;
  dispatch them as the `agent-docs` skill says. The checker definitions are installed by the setup installers: check
  whether the change belongs in the installed template or in a project-specific section that survives reinstalling.
- Reference docs: `docs/structure/daemon.md` and `docs/structure/ui.md` ("Where new code goes"), `docs/architecture.md`,
  `docs/memory/building-a-feature-for-several-agents.md`, `.claude/agents/change-checker.md`,
  `.claude/agents/ui-checker.md`.
