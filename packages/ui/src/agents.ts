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
  Settings,
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

/** Where each agent's own instructions for installing it are: the agent's vendor's page, never ours. */
export const AGENT_INSTALL_URL: Record<Agent, string> = {
  claude: "https://code.claude.com/docs/en/setup",
  codex: "https://github.com/openai/codex",
  grok: "https://docs.x.ai/build/overview",
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

/** Whether `agent` is among the agents the user added (every agent while the daemon has not decided the list). */
export function isAgentAdded(added: Agent[] | null, agent: Agent): boolean {
  return listedAgents(added).includes(agent);
}

/** Whether `agent` may be picked where a session is to run: added and not known to be missing. */
export function isAgentPickable(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, agent: Agent): boolean {
  return isAgentAdded(added, agent) && !isAgentUnavailable(agentAvailability, agent);
}

/** The agents that may be picked, in the order they were added. */
export function pickableAgents(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null): Agent[] {
  return listedAgents(added).filter((agent) => isAgentPickable(agentAvailability, added, agent));
}

/** `wanted` when it may be picked, else the first added agent that may, so a control that opens on a
 * default never opens on an agent that is not added or known to be missing. When none may be picked the
 * answer is `wanted`: nothing selectable exists to land on, and the session dialog refuses to submit anyway. */
export function selectableAgent(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, wanted: Agent): Agent {
  if (isAgentPickable(agentAvailability, added, wanted)) return wanted;
  return pickableAgents(agentAvailability, added)[0] ?? wanted;
}

/** What a user without a usable agent has to do: add one that is installed, or install one. */
export type NoAgentReason = "add" | "install";

/** Why no session can be started, `undefined` while one can: no agent is both added and installed.
 * It is `"add"` when an agent not known to be missing is waiting to be added, else `"install"`. Never
 * set because availability has not yet been determined, which must not be read as "not installed".
 * Determination lands for every agent together, from one shared snapshot, so a mix of
 * `not_determined` and a determined state never happens in practice; this still checks every entry
 * rather than just one, so it reads correctly even so. */
export function noAgentState(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null): NoAgentReason | undefined {
  if (pickableAgents(agentAvailability, added).length > 0) return undefined;
  return AGENT_OPTIONS.some((option) => !isAgentUnavailable(agentAvailability, option.value)) ? "add" : "install";
}

/** Every agent: the added ones in their order, then the others in the usual one. */
export function agentsInListingOrder(added: Agent[] | null): Agent[] {
  const listed = listedAgents(added);
  return [...listed, ...AGENT_OPTIONS.map((option) => option.value).filter((agent) => !listed.includes(agent))];
}

/** The agents a dialog lists where an agent is chosen or configured: those that may be picked, in
 * the order they were added, then any other that `kept` names (what the control currently holds
 * stays on screen rather than silently turning into another value), the added ones first. An
 * agent whose availability is not yet determined is listed, since nothing is known to be missing
 * yet. */
export function listedPickerAgents(
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  kept: (agent: Agent) => boolean,
): Agent[] {
  const pickable = pickableAgents(agentAvailability, added);
  return [...pickable, ...agentsInListingOrder(added).filter((agent) => !pickable.includes(agent) && kept(agent))];
}

/** The badge shown after an agent's name where a dialog lists it, when it is listed only for what a control
 * holds: "Not installed" when it is missing, else "Not added"; none for an agent that may be picked. */
export function agentChoiceBadge(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  agent: Agent,
): string | undefined {
  if (isAgentUnavailable(agentAvailability, agent)) return t("agents.badge.notInstalled");
  return isAgentAdded(added, agent) ? undefined : t("agents.badge.notAdded");
}

/** The options of the agent pickers: the agents that may be picked, and the one the control `held`
 * if that one may not, badged as not installed or not added and unselectable, so the value stays
 * selected and the reason it cannot be chosen again is on screen. Any other agent that is not added
 * or is known to be missing is left out. */
export function agentPickerOptions(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  held?: Agent | null,
): { value: Agent; label: string; badge?: string; isDisabled?: boolean }[] {
  return listedPickerAgents(agentAvailability, added, (agent) => agent === held).map((agent) =>
    isAgentPickable(agentAvailability, added, agent)
      ? { value: agent, label: AGENT_LABEL[agent] }
      : { value: agent, label: AGENT_LABEL[agent], badge: agentChoiceBadge(t, agentAvailability, added, agent), isDisabled: true },
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

/** The agents Settings lists and an account may be set up for: the added ones in their order, or every agent while
 * the daemon has not decided the list (`added_agents` is `null`), which must not be read as "none added". */
export function listedAgents(added: Agent[] | null): Agent[] {
  return added ?? AGENT_OPTIONS.map((option) => option.value);
}

/** The options of a picker that is not about using an agent but about setting one up (an account
 * may be set up before its agent is installed): the `listed` agents in their order, selectable, with those known
 * to be missing badged as not installed. */
export function agentSetupOptions(
  t: Translate,
  agentAvailability: Map<Agent, AgentAvailability>,
  listed: Agent[],
): { value: Agent; label: string; badge?: string }[] {
  return listed.map((agent) => {
    const option = { value: agent, label: AGENT_LABEL[agent] };
    return isAgentUnavailable(agentAvailability, agent) ? { ...option, badge: t("agents.badge.notInstalled") } : option;
  });
}

/** The accounts grouped by agent, for `agents` (every agent by default) in that order, each agent present
 * even with no account of its own; within an agent the accounts keep the order they are stored in. */
export function accountsByAgent(
  accounts: Account[],
  agents: Agent[] = AGENT_OPTIONS.map((option) => option.value),
): { agent: Agent; accounts: Account[] }[] {
  return agents.map((agent) => ({ agent, accounts: accounts.filter((a) => a.agent === agent) }));
}

/** The name an agent's default account is shown under: the one the user gave it, else the current language's word. */
export function defaultAccountName(t: Translate, names: Settings["default_account_names"], agent: Agent): string {
  return isDefaultAccountRenamed(names, agent) ? names![agent]!.trim() : t("settings.accounts.defaultName");
}

/** Whether the user has given an agent's default account a name of its own. */
export function isDefaultAccountRenamed(names: Settings["default_account_names"], agent: Agent): boolean {
  return !!names?.[agent]?.trim();
}
