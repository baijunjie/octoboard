import type { Agent } from "./protocol";

/** Display labels for the three supported agents (see "The console → project → session menu" in
 * docs/product/sessions.md). */
export const AGENT_LABEL: Record<Agent, string> = {
  claude: "Claude Code",
  codex: "Codex",
  grok: "Grok Build",
};
