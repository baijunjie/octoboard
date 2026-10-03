import type { Agent } from "./protocol";

/** Display labels for the three agents the MVP supports (see "Agent adapters" in docs/mvp.md). */
export const AGENT_LABEL: Record<Agent, string> = {
  claude: "Claude Code",
  codex: "Codex",
  grok: "Grok Build",
};

export const AGENT_OPTIONS: { value: Agent; label: string }[] = (
  Object.keys(AGENT_LABEL) as Agent[]
).map((value) => ({ value, label: AGENT_LABEL[value] }));
