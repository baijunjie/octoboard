import type { Agent, Console, Project, Session } from "../protocol";

/** What still uses an agent: the consoles that have it as a default, the projects that have it as theirs, and
 * the sessions of it that are not archived. */
export interface AgentUsage {
  consoles: string[];
  projects: string[];
  sessions: string[];
}

/** What uses `agent`, by name in the order the records are held; a session is named by `sessionName`. Removing an
 * agent from the added list leaves all of it as it is. */
export function agentUsage(
  agent: Agent,
  consoles: ReadonlyMap<string, Console>,
  projects: ReadonlyMap<string, Project>,
  sessions: ReadonlyMap<string, Session>,
  sessionName: (session: Session) => string,
): AgentUsage {
  return {
    consoles: [...consoles.values()].filter((c) => c.default_agent === agent || c.project_session_agent === agent).map((c) => c.name),
    projects: [...projects.values()].filter((p) => p.default_agent === agent).map((p) => p.name),
    sessions: [...sessions.values()].filter((s) => s.agent === agent && s.status !== "archived").map(sessionName),
  };
}
