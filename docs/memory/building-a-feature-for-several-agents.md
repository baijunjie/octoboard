# Building a feature for several agents

## Agent-specific handling lives in the provider; look for a design every agent can use first

Whatever one agent needs handled differently lives in that agent's provider, behind a capability or an interface method
that generic code calls, never in a branch of generic code on which agent it is. That holds where no general solution
exists and where one agent supports the general mechanism badly too: Codex delivering messages through its own server is
allowed because it sits behind `MessageTransport`, not because it is an exception
(`docs/conventions/provider-neutral-core.md`, "General principles", which also says a missing capability or method is
added first).

Before any per-agent handling, look for a design that needs none, even where one agent offers an easier signal. For
example, an unstarted plan-quota window is recognised for every agent by comparing its readings over time, not by
reading Codex's placeholder reset time. The user asked for this to guide agent features from now on.
