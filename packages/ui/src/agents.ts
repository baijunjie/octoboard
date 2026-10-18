import type { Translate } from "./i18n/catalog";
import type {
  Account,
  AccountField,
  Agent,
  AgentAvailability,
  Console,
  Project,
  ConsoleEffortField,
  ConsoleModelField,
  ConsoleProjectEffortField,
  ConsoleProjectModelField,
  ProjectEffortField,
  ProjectModelField,
} from "./protocol";

/** Display labels for the three supported agents (see "Rows, names and keyboard focus" in
 * docs/product/sidebar.md). */
export const AGENT_LABEL: Record<Agent, string> = {
  claude: "Claude Code",
  codex: "Codex",
  grok: "Grok Build",
};

/** The agents' names as the menu bar icon's menu uses them, shorter than the labels above ("Claude
 * quota", not "Claude Code quota"). Brand names, so not translated. */
export const AGENT_SHORT_NAME: Record<Agent, string> = {
  claude: "Claude",
  codex: "Codex",
  grok: "Grok",
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

/** The console fields holding each agent's console-session model and reasoning effort. */
export const AGENT_CONSOLE_MODEL_FIELD = {
  claude: "claude_console_session_model",
  codex: "codex_console_session_model",
  grok: "grok_console_session_model",
} as const satisfies Record<Agent, ConsoleModelField>;

export const AGENT_CONSOLE_EFFORT_FIELD = {
  claude: "claude_console_session_effort",
  codex: "codex_console_session_effort",
  grok: "grok_console_session_effort",
} as const satisfies Record<Agent, ConsoleEffortField>;

/** The console fields holding each agent's model and reasoning effort for project sessions. */
export const AGENT_CONSOLE_PROJECT_MODEL_FIELD = {
  claude: "claude_project_session_model",
  codex: "codex_project_session_model",
  grok: "grok_project_session_model",
} as const satisfies Record<Agent, ConsoleProjectModelField>;

export const AGENT_CONSOLE_PROJECT_EFFORT_FIELD = {
  claude: "claude_project_session_effort",
  codex: "codex_project_session_effort",
  grok: "grok_project_session_effort",
} as const satisfies Record<Agent, ConsoleProjectEffortField>;

/** The project fields holding each agent's model and reasoning effort for the project's sessions. */
export const AGENT_PROJECT_MODEL_FIELD = {
  claude: "claude_session_model",
  codex: "codex_session_model",
  grok: "grok_session_model",
} as const satisfies Record<Agent, ProjectModelField>;

export const AGENT_PROJECT_EFFORT_FIELD = {
  claude: "claude_session_effort",
  codex: "codex_session_effort",
  grok: "grok_session_effort",
} as const satisfies Record<Agent, ProjectEffortField>;

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

/** Whether `agent` is listed where a choice among agents is made: unless it is known to be
 * missing, or is missing but still `held` — what the control currently holds stays on screen
 * rather than silently turning into another value. */
function isAgentListed(
  agentAvailability: Map<Agent, AgentAvailability>,
  agent: Agent,
  held?: Agent | null,
): boolean {
  return !isAgentUnavailable(agentAvailability, agent) || agent === held;
}

/** The options of the agent pickers: the agents that may be picked, and the one the control `held`
 * if that one is missing, named as not installed and unselectable, so the value stays selected and
 * the reason it cannot be chosen again is on screen. An agent known to be missing and not held is
 * left out. An agent whose availability is not yet determined is listed, since nothing is known to
 * be missing yet. */
export function agentPickerOptions(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
  held?: Agent | null,
): { value: Agent; label: string; isDisabled?: boolean }[] {
  return AGENT_OPTIONS.filter((option) => isAgentListed(agentAvailability, option.value, held)).map((option) =>
    isAgentUnavailable(agentAvailability, option.value)
      ? { value: option.value, label: t("agents.notInstalled", { agent: option.label }), isDisabled: true }
      : option,
  );
}

/** One pair's model and reasoning effort, each `null` while unset. */
export type LaunchPair = { model: string | null; effort: string | null };

/** The model and reasoning effort a project session of `agent` takes from its console when
 * neither it nor its project names one: the console's project session pair, `null` for the agent's
 * own default. */
export function consoleProjectLaunch(owner: Console, agent: Agent): LaunchPair {
  return {
    model: owner[AGENT_CONSOLE_PROJECT_MODEL_FIELD[agent]] ?? null,
    effort: owner[AGENT_CONSOLE_PROJECT_EFFORT_FIELD[agent]] ?? null,
  };
}

/** The model and reasoning effort a project session of `agent` runs with when it chose neither,
 * each value taken from the project, else from the console's project session pair, else `null`,
 * the agent's own default. */
export function projectSessionLaunch(owner: Console, project: Project, agent: Agent): LaunchPair {
  const fallback = consoleProjectLaunch(owner, agent);
  return {
    model: project[AGENT_PROJECT_MODEL_FIELD[agent]] ?? fallback.model,
    effort: project[AGENT_PROJECT_EFFORT_FIELD[agent]] ?? fallback.effort,
  };
}

/** What a session of `agent` takes when it chooses nothing: a project session's project value,
 * else its console's project session one; a console session's console session one. */
export function inheritedSessionLaunch(owner: Console, project: Project | undefined, agent: Agent): LaunchPair {
  if (project) return projectSessionLaunch(owner, project, agent);
  return {
    model: owner[AGENT_CONSOLE_MODEL_FIELD[agent]] ?? null,
    effort: owner[AGENT_CONSOLE_EFFORT_FIELD[agent]] ?? null,
  };
}

/** `inherited` with each value an override sets: absent follows `inherited`, `null` is the agent's
 * own default. */
export function launchWithOverrides(
  inherited: LaunchPair,
  overrides: { model?: string | null; effort?: string | null } | undefined,
): LaunchPair {
  return {
    model: overrides?.model === undefined ? inherited.model : overrides.model,
    effort: overrides?.effort === undefined ? inherited.effort : overrides.effort,
  };
}

/** The agent a project session of `owner` is of when neither the session nor its project names
 * one. */
export function projectSessionAgent(owner: Pick<Console, "default_agent" | "project_session_agent">): Agent {
  return owner.project_session_agent ?? owner.default_agent;
}

/** The options of a picker that is not about using an agent but about setting one up (an account
 * may be added before its agent is installed): every agent, selectable, with those known to be
 * missing named as not installed. */
export function agentSetupOptions(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
): { value: Agent; label: string }[] {
  return AGENT_OPTIONS.map((option) =>
    isAgentUnavailable(agentAvailability, option.value)
      ? { value: option.value, label: t("agents.notInstalled", { agent: option.label }) }
      : option,
  );
}

/** The accounts grouped by agent, in the order the agents are listed elsewhere, each agent present
 * even with no account of its own; within an agent the accounts keep the order they are stored in. */
export function accountsByAgent(accounts: Account[]): { agent: Agent; accounts: Account[] }[] {
  return AGENT_OPTIONS.map(({ value }) => ({ agent: value, accounts: accounts.filter((a) => a.agent === value) }));
}
