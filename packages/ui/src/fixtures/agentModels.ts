import type { Agent, AgentModel } from "../protocol";

/** What the fixture daemon lists for Claude Code when a scenario says nothing else: its aliases,
 * best first, with the version each stands for and whether the account may use it (here all of
 * them), and the levels each takes spread so every case of the effort picker is on show — a full
 * set, a narrower one, and a model that takes none. */
export const SAMPLE_CLAUDE_MODELS: AgentModel[] = [
  { id: "fable", name: "Fable", version: "Fable 5.1", available: true, efforts: ["low", "medium", "high", "xhigh", "max"] },
  { id: "opus", name: "Opus", version: "Opus 5.5", available: true, efforts: ["low", "medium", "high", "xhigh", "max"] },
  { id: "sonnet", name: "Sonnet", version: "Sonnet 5.5", available: true, efforts: ["low", "medium", "high"] },
  { id: "haiku", name: "Haiku", version: "Haiku 5.5", available: true, efforts: [] },
];

/** The list of an account that cannot use the two best models (a Pro account): Fable and Opus are
 * listed but unavailable, so the model a session gets when nothing names one is Sonnet. */
export const SAMPLE_CLAUDE_MODELS_PRO: AgentModel[] = SAMPLE_CLAUDE_MODELS.map((model) =>
  model.id === "fable" || model.id === "opus" ? { ...model, available: false } : model,
);

/** The model the daemon picks as Claude Code's default when no Claude Code configuration names a
 * model: the first listed model known to be usable (`available: true`), or none where there is
 * not one, the list being empty or its availability unknown, and Claude Code then picks its own
 * default. */
export function defaultClaudeModel(models: AgentModel[]): string | undefined {
  return models.find((model) => model.available === true)?.id;
}

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
