import type { Agent, ConfigDirField } from "./protocol";

/** Display labels for the three agents the MVP supports (see "Agent adapters" in docs/mvp.md). */
export const AGENT_LABEL: Record<Agent, string> = {
  claude: "Claude Code",
  codex: "Codex",
  grok: "Grok Build",
};

export const AGENT_OPTIONS: { value: Agent; label: string }[] = (
  Object.keys(AGENT_LABEL) as Agent[]
).map((value) => ({ value, label: AGENT_LABEL[value] }));

/** The console field holding each agent's config directory, and the agent's usual default, shown as
 * the input's placeholder. */
export const AGENT_CONFIG_DIR: Record<Agent, { field: ConfigDirField; placeholder: string }> = {
  claude: { field: "claude_config_dir", placeholder: "~/.claude" },
  codex: { field: "codex_config_dir", placeholder: "~/.codex" },
  grok: { field: "grok_config_dir", placeholder: "~/.grok" },
};
