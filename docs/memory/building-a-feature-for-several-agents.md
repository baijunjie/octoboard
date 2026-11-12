# Building a feature for several agents

## Prefer a solution that works for every agent; handle one agent specially only where that is justified

When a feature involves several agents (Claude Code, Codex, Grok Build), look first for a solution that works for every
agent, and choose it over one built around a single agent's behaviour. This is a preference, not a ban on
agent-specific logic: handling one agent specially is still right when there is no general solution, or when a
particular agent supports the general solution badly — Codex's message delivery going through its app server rather
than its terminal is such a case. For example, an unstarted plan-quota window is recognised for every agent by
comparing its readings over time, rather than by reading Codex's placeholder reset time. The user asked for this to
guide similar agent features from now on.

Where the agent-specific part goes, once chosen, is set by the "Where new code goes" section of
`docs/structure/daemon.md`.
