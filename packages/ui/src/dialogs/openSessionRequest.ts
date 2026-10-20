import { choiceGroups, settledInitialChoice, type AccountChoice } from "../accountChoices";
import type { Translate } from "../i18n/catalog";
import type { AccountQuota, Agent, AgentAvailability, Console, Project, RequestBody, Settings } from "../protocol";
import type { NewSessionBinding, NewSessionRequest } from "./dialogRequest";

export type OpenSessionRequest = Extract<RequestBody, { type: "open_session" }>;

/** The `open_session` request for what the session dialog settled: `choice` is its agent and
 * account, `ownerId` the console session the session reports to, `overrides` the model and effort
 * the user changed (absent, they follow the parent). */
export function openSessionRequest(t: Translate, { console: parentConsole, project, ownerId, resumeId, choice, title, overrides }: {
  console: Console;
  project?: Project;
  ownerId?: string;
  resumeId?: string;
  choice: AccountChoice;
  title?: string;
  overrides?: { model?: string | null; effort?: string | null };
}): OpenSessionRequest {
  return {
    type: "open_session",
    console_id: parentConsole.id,
    ...(project ? { project_id: project.id, bound_to: ownerId } : {}),
    ...(resumeId !== undefined ? { resume_agent_session_id: resumeId } : {}),
    agent: choice.agent,
    account: choice.account,
    title: title?.trim() || t("session.defaultName", { name: project?.name ?? parentConsole.name }),
    ...overrides,
  };
}

/** What a session-creating control reads from the store to settle the choice a session opens on. */
export interface QuickCreateState {
  settings: Pick<Settings, "accounts" | "added_agents">;
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
}

/** The agent and account the session dialog opens on (`settledInitialChoice`) for a session in `parentConsole`
 * and `project`, with the window that still keeps the account from running a session. */
function settledQuickChoice(parentConsole: Console, project: Project | undefined, state: QuickCreateState, now: number) {
  const { accounts, added_agents } = state.settings;
  const groups = choiceGroups(accounts, state.agentAvailability, added_agents, () => "", { quotas: state.accountQuotas, now });
  return settledInitialChoice(parentConsole, project, accounts, state.agentAvailability, added_agents, groups);
}

/** Whether the agent a session created where `target` would be has all its accounts used up, so that
 * creating with the defaults has nothing to run on. The dialog still opens, as the user can pick
 * another agent there. */
export function quickCreateUsedUp(target: NewSessionRequest, state: QuickCreateState, now: number): boolean {
  return settledQuickChoice(target.console, target.kind === "new-session" ? target.project : undefined, state, now).exhaustedBy !== undefined;
}

/** The request the session dialog sends when submitted untouched: the localized default name, the agent and account it
 * opens on (`settledInitialChoice`), the model and effort left to follow the parent, and no owner unless
 * `binding` fixes one (a console session's focus mode). With the inherited account's quota used up it
 * opens on another account of the agent; with every account of the agent used up there
 * is nothing to send, and `usedUp` says so instead (`quickCreateUsedUp`). */
export function defaultOpenSessionRequest(
  t: Translate,
  parentConsole: Console,
  project: Project | undefined,
  binding: NewSessionBinding,
  state: QuickCreateState,
  now: number,
): { request: OpenSessionRequest } | { usedUp: true } {
  const { choice, exhaustedBy } = settledQuickChoice(parentConsole, project, state, now);
  if (exhaustedBy) return { usedUp: true };
  return {
    request: openSessionRequest(t, {
      console: parentConsole,
      project,
      ownerId: binding.kind === "bound" ? binding.to.id : undefined,
      choice,
    }),
  };
}
