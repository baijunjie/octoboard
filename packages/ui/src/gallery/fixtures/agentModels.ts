import type { Agent, AgentModel } from "../../protocol";

/** What the fixture daemon lists for Claude Code when a scenario says nothing else: its version-free
 * aliases, with the levels each takes spread so every case of the effort picker is on show — a full
 * set, a narrower one, and a model that takes none. */
export const SAMPLE_CLAUDE_MODELS: AgentModel[] = [
  { id: "best", name: "Best available", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { id: "opus", name: "Opus", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { id: "sonnet", name: "Sonnet", efforts: ["low", "medium", "high"] },
  { id: "haiku", name: "Haiku", efforts: [] },
];

/** The same for Codex, whose models are named by identifier rather than by alias. */
export const SAMPLE_CODEX_MODELS: AgentModel[] = [
  { id: "gpt-5.1-codex-max", name: "gpt-5.1-codex-max", efforts: ["low", "medium", "high", "xhigh"] },
  { id: "gpt-5.1-codex-mini", name: "gpt-5.1-codex-mini", efforts: ["medium", "high"] },
];

/** The same for Grok Build. */
export const SAMPLE_GROK_MODELS: AgentModel[] = [
  { id: "grok-code-fast-1", name: "grok-code-fast-1", efforts: ["low", "high"] },
  { id: "grok-4", name: "grok-4", efforts: [] },
];

/** What the fixture daemon lists per agent when a scenario says nothing else. */
export const SAMPLE_MODELS: Record<Agent, AgentModel[]> = {
  claude: SAMPLE_CLAUDE_MODELS,
  codex: SAMPLE_CODEX_MODELS,
  grok: SAMPLE_GROK_MODELS,
};
