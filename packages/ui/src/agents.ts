import type { Translate } from "./i18n/catalog";
import type { Agent, AgentAvailability, ConfigDirField } from "./protocol";

/** Display labels for the three supported agents (see "Rows, names and keyboard focus" in
 * docs/product/sidebar.md). */
export const AGENT_LABEL: Record<Agent, string> = {
  claude: "Claude Code",
  codex: "Codex",
  grok: "Grok Build",
};

export const AGENT_OPTIONS: { value: Agent; label: string }[] = (Object.keys(AGENT_LABEL) as Agent[]).map(
  (value) => ({ value, label: AGENT_LABEL[value] }),
);

/** The console field holding each agent's config directory, and the agent's usual default, shown as
 * the input's placeholder. */
export const AGENT_CONFIG_DIR: Record<Agent, { field: ConfigDirField; placeholder: string }> = {
  claude: { field: "claude_config_dir", placeholder: "~/.claude" },
  codex: { field: "codex_config_dir", placeholder: "~/.codex" },
  grok: { field: "grok_config_dir", placeholder: "~/.grok" },
};

/** True once availability has been determined and found none at all — never while it has not yet
 * been determined, which must not be read as "none available". Determination lands for every
 * agent together, from one shared snapshot, so a mix of `not_determined` and a determined state
 * never happens in practice; this still checks every entry rather than just one, so it reads
 * correctly even so. */
export function noAgentAvailable(agentAvailability: Map<Agent, AgentAvailability>): boolean {
  return AGENT_OPTIONS.every((option) => agentAvailability.get(option.value)?.availability === "unavailable");
}

/** An agent picker's options (the session dialog's and the console dialog's), each carrying
 * whether it may be picked and, when it may not, a label saying so: an unavailable agent is shown
 * named rather than hidden, so the reason it cannot be picked is on screen, but it cannot be
 * selected either. An agent whose availability is not yet determined is left selectable, since
 * nothing is known to be missing yet. */
export function agentPickerOptions(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
): { value: Agent; label: string; isDisabled?: boolean }[] {
  return AGENT_OPTIONS.map((option) => {
    const unavailable = agentAvailability.get(option.value)?.availability === "unavailable";
    return unavailable
      ? { value: option.value, label: t("agents.notInstalled", { agent: option.label }), isDisabled: true }
      : option;
  });
}
