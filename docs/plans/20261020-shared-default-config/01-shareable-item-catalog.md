# 01 Shareable item catalog

> Goal: each agent has a verified list of the items an account shares, and the open questions about linking are
> answered with the real agents.
> Completion criteria: the catalog is in the daemon with unit tests covering every agent's list and the never-shared
> items; each finding below has been observed with the real agent against scratch directories and recorded.

## Technical design

- [ ] A per-agent catalog of shareable items, each marked as a directory or a file, and as configuration or
      conversation history (the two are handled differently when the switch is turned on).
- [ ] Never-shared items are absent from every catalog (see the overview).

## Implementation plan

- [ ] List every entry each agent keeps in its configuration directory and classify it.
- [ ] For each file item, link it into a scratch account directory, make the agent save it, and record whether the link
      survives.
- [ ] Codex: whether `state_<n>.sqlite` can be opened through a link by several processes at once without damage; and,
      with only `sessions/` shared, whether a conversation started under one account resumes under another.
- [ ] Confirm two accounts writing conversation history into the shared directory at once do not interfere.
- [ ] Confirm the Codex and Grok Build items load through a link, Grok's included through its per-session home.

## Notes for the developer

- **Reusable capabilities**: the agent adapters' knowledge of each agent's configuration directory variable and of the
  default account's resolution; Grok Build's per-session home, which already links every entry of the source home
  except the ones Octoboard owns.
- **Development notes**: run the agents against scratch directories only, never the user's real ones.
- **Reference docs**: `docs/agent-cli-reference.md`, `docs/product/agent-accounts.md`, `docs/product/consoles.md`,
  "Agent session data" in `docs/product/launching-agents.md`.
