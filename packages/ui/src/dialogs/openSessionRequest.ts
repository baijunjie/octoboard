import { initialChoice, type AccountChoice } from "../accountChoices";
import type { Account, Agent, AgentAvailability, Console, Project, RequestBody } from "../protocol";
import type { NewSessionBinding } from "./dialogRequest";

export type OpenSessionRequest = Extract<RequestBody, { type: "open_session" }>;

/** The `open_session` request for what the session dialog settled: `choice` is its agent and
 * account, `ownerId` the console session the session reports to, `overrides` the model and effort
 * the user changed (absent, they follow the parent). */
export function openSessionRequest({ console: parentConsole, project, ownerId, resumeId, choice, title, overrides }: {
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
    title: title || undefined,
    ...overrides,
  };
}

/** The request the session dialog sends when submitted untouched: no name, the agent and account it
 * opens on (`initialChoice`), the model and effort left to follow the parent, and no owner unless
 * `binding` fixes one (a console session's focus mode). */
export function defaultOpenSessionRequest(
  parentConsole: Console,
  project: Project | undefined,
  binding: NewSessionBinding,
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
): OpenSessionRequest {
  return openSessionRequest({
    console: parentConsole,
    project,
    ownerId: binding.kind === "bound" ? binding.to.id : undefined,
    choice: initialChoice(parentConsole, project, accounts, agentAvailability),
  });
}
