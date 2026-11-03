import type { Translate } from "../i18n/catalog";
import {
  type Agent,
  type AgentAvailability,
  type Console,
  type Launch,
  type ModelDisplay,
  type Project,
  type ProviderInfo,
  type Settings,
} from "../protocol";
import { shownModel } from "./agentLaunchSettings";
import { providerLabel } from "./providers";

/** The ids of the providers the daemon serves, in its registry order (`[]` until the first snapshot). */
export function servedAgents(providers: readonly ProviderInfo[]): Agent[] {
  return providers.map(({ id }) => id);
}

/** A launch pair that sets neither the model nor the effort, for an agent whose entry a per-agent map lacks. */
const NO_LAUNCH: Launch = { model: null, effort: null };

/** Whether availability has been determined and found `agent` missing — never while it has not
 * yet been determined, which must not be read as "not installed". */
export function isAgentUnavailable(agentAvailability: Map<Agent, AgentAvailability>, agent: Agent): boolean {
  return agentAvailability.get(agent)?.availability === "unavailable";
}

/** Whether `agent` is among the agents the user added (every agent while the daemon has not decided the list). */
export function isAgentAdded(added: Agent[] | null, agent: Agent): boolean {
  return added === null || added.includes(agent);
}

/** Whether `agent` may be picked where a session is to run: added and not known to be missing. */
export function isAgentPickable(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, agent: Agent): boolean {
  return isAgentAdded(added, agent) && !isAgentUnavailable(agentAvailability, agent);
}

/** The agents that may be picked, in the order they were added. */
export function pickableAgents(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, providers: readonly ProviderInfo[]): Agent[] {
  return listedAgents(added, servedAgents(providers)).filter((agent) => isAgentPickable(agentAvailability, added, agent));
}

/** `wanted` when it may be picked, else the first added agent that may, so a control that opens on a
 * default never opens on an agent that is not added or known to be missing. When none may be picked the
 * answer is `wanted`: nothing selectable exists to land on, and the session dialog refuses to submit anyway. */
export function selectableAgent(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, providers: readonly ProviderInfo[], wanted: Agent): Agent {
  if (isAgentPickable(agentAvailability, added, wanted)) return wanted;
  return pickableAgents(agentAvailability, added, providers)[0] ?? wanted;
}

/** What a user without a usable agent has to do: add one that is installed, or install one. */
export type NoAgentReason = "add" | "install";

/** Why no session can be started, `undefined` while one can: no agent is both added and installed.
 * It is `"add"` when an agent not known to be missing is waiting to be added, else `"install"`. Never
 * set because availability has not yet been determined, which must not be read as "not installed", nor
 * before the daemon has served its providers (`providers` is empty), when no agent is known at all.
 * Determination lands for every agent together, from one shared snapshot, so a mix of
 * `not_determined` and a determined state never happens in practice; this still checks every entry
 * rather than just one, so it reads correctly even so. */
export function noAgentState(agentAvailability: Map<Agent, AgentAvailability>, added: Agent[] | null, providers: readonly ProviderInfo[]): NoAgentReason | undefined {
  if (providers.length === 0 || pickableAgents(agentAvailability, added, providers).length > 0) return undefined;
  return servedAgents(providers).some((agent) => !isAgentUnavailable(agentAvailability, agent)) ? "add" : "install";
}

/** Every served agent: the added ones in their order, then the others in the registry's. */
export function agentsInListingOrder(added: Agent[] | null, served: readonly Agent[]): Agent[] {
  const listed = listedAgents(added, served);
  return [...listed, ...served.filter((agent) => !listed.includes(agent))];
}

/** The agents a dialog lists where an agent is chosen or configured: those that may be picked, in
 * the order they were added, then any other that `kept` names (what the control currently holds
 * stays on screen rather than silently turning into another value), the added ones first. An
 * agent whose availability is not yet determined is listed, since nothing is known to be missing
 * yet. */
export function listedPickerAgents(
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  providers: readonly ProviderInfo[],
  kept: (agent: Agent) => boolean,
): Agent[] {
  const pickable = pickableAgents(agentAvailability, added, providers);
  return [...pickable, ...agentsInListingOrder(added, servedAgents(providers)).filter((agent) => !pickable.includes(agent) && kept(agent))];
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
  providers: readonly ProviderInfo[],
  held?: Agent | null,
): { value: Agent; label: string; badge?: string; isDisabled?: boolean }[] {
  return listedPickerAgents(agentAvailability, added, providers, (agent) => agent === held).map((agent) =>
    isAgentPickable(agentAvailability, added, agent)
      ? { value: agent, label: providerLabel(t, providers, agent) }
      : { value: agent, label: providerLabel(t, providers, agent), badge: agentChoiceBadge(t, agentAvailability, added, agent), isDisabled: true },
  );
}

/** The model and reasoning effort a project session of `agent` takes from its console when
 * neither it nor its project names one: the console's project session pair, `null` for the agent's
 * own default. */
export function consoleProjectLaunch(owner: Console, agent: Agent): Launch {
  return owner.project_session_launch[agent] ?? NO_LAUNCH;
}

/** The model and reasoning effort a project session of `agent` runs with when it chose neither,
 * each value taken from the project, else from the console's project session pair, else `null`,
 * the agent's own default. A stored model that is no model (see `shownModel`, which `display` is the agent's for) counts as unset at
 * each level, as the daemon reads it at launch, so it falls through to the next. */
export function projectSessionLaunch(owner: Console, project: Project, agent: Agent, display: ModelDisplay | undefined): Launch {
  const fallback = consoleProjectLaunch(owner, agent);
  const own = project.launch[agent] ?? NO_LAUNCH;
  return {
    model: shownModel(display, own.model) ?? shownModel(display, fallback.model),
    effort: own.effort ?? fallback.effort,
  };
}

/** What a session of `agent` takes when it chooses nothing: a project session's project value,
 * else its console's project session one; a console session's console session one. */
export function inheritedSessionLaunch(owner: Console, project: Project | undefined, agent: Agent, display: ModelDisplay | undefined): Launch {
  if (project) return projectSessionLaunch(owner, project, agent, display);
  const launch = owner.launch[agent] ?? NO_LAUNCH;
  return { model: shownModel(display, launch.model), effort: launch.effort };
}

/** `inherited` with each value an override sets: absent follows `inherited`, `null` is the agent's
 * own default. */
export function launchWithOverrides(
  inherited: Launch,
  overrides: { model?: string | null; effort?: string | null } | undefined,
): Launch {
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

/** The agents Settings lists and an account may be set up for: the added ones in their order, or every served agent
 * while the daemon has not decided the list (`added_agents` is `null`), which must not be read as "none added". */
export function listedAgents(added: Agent[] | null, served: readonly Agent[]): Agent[] {
  return added ?? [...served];
}

/** The options of a picker that is not about using an agent but about setting one up (an account
 * may be set up before its agent is installed): the `listed` agents in their order, selectable, with those known
 * to be missing badged as not installed. */
export function agentSetupOptions(
  t: Translate,
  providers: readonly ProviderInfo[],
  agentAvailability: Map<Agent, AgentAvailability>,
  listed: Agent[],
): { value: Agent; label: string; badge?: string }[] {
  return listed.map((agent) => {
    const option = { value: agent, label: providerLabel(t, providers, agent) };
    return isAgentUnavailable(agentAvailability, agent) ? { ...option, badge: t("agents.badge.notInstalled") } : option;
  });
}

/** The name an agent's default account is shown under: the one the user gave it, else the current language's word. */
export function defaultAccountName(t: Translate, names: Settings["default_account_names"] | undefined, agent: Agent): string {
  return isDefaultAccountRenamed(names, agent) ? names![agent]!.trim() : t("settings.accounts.defaultName");
}

/** Whether the user has given an agent's default account a name of its own. */
export function isDefaultAccountRenamed(names: Settings["default_account_names"] | undefined, agent: Agent): boolean {
  return !!names?.[agent]?.trim();
}
