import type { Translate } from "./i18n/catalog";
import type { Account, AccountField, Agent, AgentAvailability } from "./protocol";

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

/** The agent's usual config directory, shown as the account form's placeholder. */
export const AGENT_DEFAULT_CONFIG_DIR: Record<Agent, string> = {
  claude: "~/.claude",
  codex: "~/.codex",
  grok: "~/.grok",
};

/** The console field holding each agent's account reference. */
export const AGENT_ACCOUNT_FIELD: Record<Agent, AccountField> = {
  claude: "claude_account_id",
  codex: "codex_account_id",
  grok: "grok_account_id",
};

/** Whether availability has been determined and found `agent` missing — never while it has not
 * yet been determined, which must not be read as "not installed". */
export function isAgentUnavailable(agentAvailability: Map<Agent, AgentAvailability>, agent: Agent): boolean {
  return agentAvailability.get(agent)?.availability === "unavailable";
}

/** `wanted` when it may be picked, else the first agent that may, so a control that opens on a
 * default never opens on an agent known to be missing. When none may be picked the answer is
 * `wanted`: nothing selectable exists to land on, and the session dialog refuses to submit anyway. */
export function selectableAgent(agentAvailability: Map<Agent, AgentAvailability>, wanted: Agent): Agent {
  if (!isAgentUnavailable(agentAvailability, wanted)) return wanted;
  return AGENT_OPTIONS.find((option) => !isAgentUnavailable(agentAvailability, option.value))?.value ?? wanted;
}

/** True once availability has been determined and found none at all — never while it has not yet
 * been determined, which must not be read as "none available". Determination lands for every
 * agent together, from one shared snapshot, so a mix of `not_determined` and a determined state
 * never happens in practice; this still checks every entry rather than just one, so it reads
 * correctly even so. */
export function noAgentAvailable(agentAvailability: Map<Agent, AgentAvailability>): boolean {
  return AGENT_OPTIONS.every((option) => isAgentUnavailable(agentAvailability, option.value));
}

/** The options of the agent pickers outside the session dialog, each carrying whether it may be picked and, when it
 * may not, a label saying so: an unavailable agent is shown named rather than hidden, so the reason
 * it cannot be picked is on screen, but it cannot be selected either. An agent whose availability
 * is not yet determined is left selectable, since nothing is known to be missing yet. */
export function agentPickerOptions(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
): { value: Agent; label: string; isDisabled?: boolean }[] {
  return AGENT_OPTIONS.map((option) =>
    isAgentUnavailable(agentAvailability, option.value)
      ? { value: option.value, label: t("agents.notInstalled", { agent: option.label }), isDisabled: true }
      : option,
  );
}

/** The accounts grouped by agent, in the order the agents are listed elsewhere, each agent present
 * even with no account of its own; within an agent the accounts keep the order they are stored in. */
export function accountsByAgent(accounts: Account[]): { agent: Agent; accounts: Account[] }[] {
  return AGENT_OPTIONS.map(({ value }) => ({ agent: value, accounts: accounts.filter((a) => a.agent === value) }));
}
