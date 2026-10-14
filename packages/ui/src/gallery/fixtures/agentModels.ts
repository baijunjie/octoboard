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

/** The same for Codex, whose model names and effort levels come from `codex debug models`. */
export const SAMPLE_CODEX_MODELS: AgentModel[] = [
  { id: "gpt-6.1-sol", name: "GPT-6.1-Sol", efforts: ["low", "medium", "high", "xhigh", "max", "ultra"] },
  { id: "gpt-6-astra", name: "GPT-6-Astra", efforts: ["low", "medium", "high", "xhigh", "max", "ultra"] },
  { id: "gpt-6-sol", name: "GPT-6-Sol", efforts: ["low", "medium", "high", "xhigh", "max", "ultra"] },
  { id: "gpt-6-luna", name: "GPT-6-Luna", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { id: "gpt-5.6-sol", name: "GPT-5.6-Sol", efforts: ["low", "medium", "high", "xhigh", "max", "ultra"] },
  { id: "gpt-5.6-terra", name: "GPT-5.6-Terra", efforts: ["low", "medium", "high", "xhigh", "max", "ultra"] },
  { id: "gpt-5.6-luna", name: "GPT-5.6-Luna", efforts: ["low", "medium", "high", "xhigh", "max"] },
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
