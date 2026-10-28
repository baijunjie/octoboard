import { choiceGroups, settledInitialChoice, type AccountChoice } from "../accountChoices";
import type { Translate } from "../i18n/catalog";
import type { AccountQuota, AccountSharing, Agent, AgentAvailability, Console, Project, RequestBody, Settings } from "../protocol";
import type { NewSessionBinding, NewSessionRequest } from "./dialogRequest";

export type OpenSessionRequest = Extract<RequestBody, { type: "open_session" }>;

/** The `open_session` request for what the session dialog settled: `choice` is its agent and
 * account, `ownerId` the console session the session reports to, `overrides` the model and effort
 * the user changed (absent, they follow the parent). */
export function openSessionRequest(t: Translate, { console: parentConsole, project, ownerId, resumeId, choice, title, overrides, appearance }: {
  console: Console;
  project?: Project;
  ownerId?: string;
  resumeId?: string;
  choice: AccountChoice;
  title?: string;
  overrides?: { model?: string | null; effort?: string | null };
  /** The window's resolved appearance, sent as `terminal_appearance`. */
  appearance: "light" | "dark";
}): OpenSessionRequest {
  return {
    type: "open_session",
    console_id: parentConsole.id,
    ...(project ? { project_id: project.id, bound_to: ownerId } : {}),
    ...(resumeId !== undefined ? { resume_agent_session_id: resumeId } : {}),
    agent: choice.agent,
    account: choice.account,
    title: title?.trim() || t("session.defaultName", { name: project?.name ?? parentConsole.name }),
    terminal_appearance: appearance,
    ...overrides,
  };
}

/** What a session-creating control reads from the store to settle the choice a session opens on. */
export interface QuickCreateState {
  settings: Pick<Settings, "accounts" | "added_agents" | "auto_select_account" | "disabled_default_accounts" | "account_order">;
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
  accountSharing?: Map<string, AccountSharing>;
}

/** The agent and account the session dialog opens on (`settledInitialChoice`) for a session in `parentConsole`
 * and `project`, with the window that still keeps the account from running a session. When
 * `auto_select_account` is on, weekly quota ranks the accounts and another agent is tried if necessary. */
function settledQuickChoice(parentConsole: Console, project: Project | undefined, state: QuickCreateState, now: number) {
  const { accounts, added_agents, auto_select_account } = state.settings;
  const groups = choiceGroups(accounts, state.agentAvailability, added_agents, () => "", { quotas: state.accountQuotas, now }, state.accountSharing, state.settings.disabled_default_accounts, state.settings.account_order);
  return settledInitialChoice(
    parentConsole,
    project,
    accounts,
    state.agentAvailability,
    added_agents,
    groups,
    auto_select_account ? { now } : undefined,
  );
}

/** Whether the settled choice for `target` has no usable account after any automatic fallback,
 * so that creating with defaults has nothing to run on. The dialog still opens, as the user can pick
 * another agent there. */
export function quickCreateUnavailable(target: NewSessionRequest, state: QuickCreateState, now: number): boolean {
  const { exhaustedBy, changingSharing, signedOut, disabled } = settledQuickChoice(target.console, target.kind === "new-session" ? target.project : undefined, state, now);
  return exhaustedBy !== undefined || changingSharing || signedOut || disabled;
}

/** The request the untouched session dialog would send, including its localized name and any fixed owner.
 * Automatic selection can try other agents. With no usable account, no request is sent. */
export function defaultOpenSessionRequest(
  t: Translate,
  parentConsole: Console,
  project: Project | undefined,
  binding: NewSessionBinding,
  state: QuickCreateState,
  now: number,
  appearance: "light" | "dark",
): { request: OpenSessionRequest } | { unavailable: true } {
  const { choice, exhaustedBy, changingSharing, signedOut, disabled } = settledQuickChoice(parentConsole, project, state, now);
  if (disabled || exhaustedBy || signedOut || changingSharing) return { unavailable: true };
  return {
    request: openSessionRequest(t, {
      console: parentConsole,
      project,
      ownerId: binding.kind === "bound" ? binding.to.id : undefined,
      choice,
      appearance,
    }),
  };
}
