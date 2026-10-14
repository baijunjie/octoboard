import { noAgentAvailable } from "../agents";
import type { NewSessionBinding } from "../dialogs/dialogRequest";
import type { Agent, AgentAvailability, Console, Project, Session } from "../protocol";
import { consoleSessionAbove } from "../sidebar/order";
import type { FocusTarget } from "../sidebar/types";

export type TerminalEmptyState =
  | { kind: "add-project" }
  | { kind: "new-session"; project: Project; binding: NewSessionBinding }
  | { kind: "new-console-session" }
  | { kind: "no-agents" };

/** The next step while the terminal has no selection. Archives do not supply a live session;
 * project focus considers only the sessions its own list shows, not another owner's team.
 * A selected terminal and the archive overlay suppress the offer at their call site. */
export function terminalEmptyState(
  ready: boolean,
  console: Console | undefined,
  focus: FocusTarget | undefined,
  projects: Project[],
  sessions: Session[],
  availability: Map<Agent, AgentAvailability>,
): TerminalEmptyState | undefined {
  if (!ready || !console) return undefined;
  const inConsole = sessions.filter((s) => s.console_id === console.id);
  const listedProjects = projects.filter((p) => p.console_id === console.id);
  if (focus && "consoleSession" in focus) return undefined;
  if (focus && "project" in focus) {
    const byId = new Map(sessions.map((s) => [s.id, s]));
    if (inConsole.some((s) => s.project_id === focus.project.id && s.status !== "archived" && !consoleSessionAbove(s, (id) => byId.get(id)))) return undefined;
    return noAgentAvailable(availability)
      ? { kind: "no-agents" }
      : { kind: "new-session", project: focus.project, binding: { kind: "unbound" } };
  }
  if (inConsole.some((s) => s.status !== "archived")) return undefined;
  if (listedProjects.length === 0) return { kind: "add-project" };
  if (noAgentAvailable(availability)) return { kind: "no-agents" };
  return listedProjects.length === 1
    ? { kind: "new-session", project: listedProjects[0], binding: { kind: "choose" } }
    : { kind: "new-console-session" };
}
