import { noAgentState, type NoAgentReason } from "../agents";
import type { NewSessionBinding } from "../dialogs/dialogRequest";
import type { Agent, AgentAvailability, Console, Project, Session } from "../protocol";
import { consoleSessionAbove } from "../sidebar/order";
import type { FocusTarget } from "../sidebar/types";

export type TerminalEmptyState =
  | { kind: "add-project" }
  | { kind: "new-session"; project: Project; binding: NewSessionBinding }
  | { kind: "new-console-session" }
  | { kind: "no-agents"; reason: NoAgentReason };

/** The next step while the terminal has no selection. Archives do not supply a live session;
 * project focus considers only the sessions its own list shows, not another owner's team.
 * A selected terminal and the archive overlay suppress the offer at their call site. `added` is the
 * user's added agents, `null` while the daemon has not decided them. */
export function terminalEmptyState(
  ready: boolean,
  console: Console | undefined,
  focus: FocusTarget | undefined,
  projects: Project[],
  sessions: Session[],
  availability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
): TerminalEmptyState | undefined {
  if (!ready || !console) return undefined;
  const inConsole = sessions.filter((s) => s.console_id === console.id);
  const listedProjects = projects.filter((p) => p.console_id === console.id);
  const reason = noAgentState(availability, added);
  const noAgents: TerminalEmptyState | undefined = reason && { kind: "no-agents", reason };
  if (focus && "consoleSession" in focus) return undefined;
  if (focus && "project" in focus) {
    const byId = new Map(sessions.map((s) => [s.id, s]));
    if (inConsole.some((s) => s.project_id === focus.project.id && s.status !== "archived" && !consoleSessionAbove(s, (id) => byId.get(id)))) return undefined;
    return noAgents ?? { kind: "new-session", project: focus.project, binding: { kind: "unbound" } };
  }
  if (!inConsole.some((s) => s.role === "console" && s.status !== "archived")) {
    return noAgents ?? { kind: "new-console-session" };
  }
  if (listedProjects.length === 0) return { kind: "add-project" };
  if (listedProjects.length > 1) return undefined;
  if (noAgents) return noAgents;
  return { kind: "new-session", project: listedProjects[0], binding: { kind: "choose" } };
}
