import { useShallow } from "zustand/react/shallow";

import type { Session } from "../protocol";
import { useDaemonStore } from "../store";
import { consoleStatus } from "../sidebar/order";
import type { ShownStatusKind } from "../sessionLabel";
import { countWaitingInTree } from "../waiting";
import { indexSessions, sessionsOf } from "./sessionIndex";

/** Every session of a console, console sessions included. The array is the same one until a
 * session of this console is added, removed or changed, so an event about another console's
 * session does not render what reads it. */
export function useConsoleSessions(consoleId: string): Session[] {
  return useDaemonStore((s) => sessionsOf(indexSessions(s.sessions).byConsole, consoleId));
}

/** Every session of a project, kept the same way as `useConsoleSessions`. */
export function useProjectSessions(projectId: string): Session[] {
  return useDaemonStore((s) => sessionsOf(indexSessions(s.sessions).byProject, projectId));
}

/** A console's console sessions, the same array while none of them changes. */
export function useOwnerSessions(consoleId: string): Session[] {
  return useDaemonStore(useShallow((s) => sessionsOf(indexSessions(s.sessions).byConsole, consoleId).filter((x) => x.role === "console")));
}

/** A console's archived console sessions, the same array while none of them changes: all a console's
 * menu reads of its sessions. */
export function useArchivedOwnerSessions(consoleId: string): Session[] {
  return useDaemonStore(
    useShallow((s) =>
      sessionsOf(indexSessions(s.sessions).byConsole, consoleId).filter((x) => x.role === "console" && x.status === "archived"),
    ),
  );
}

/** The avatar status selected from all sessions belonging to this console. */
export function useConsoleStatus(consoleId: string): ShownStatusKind | undefined {
  return useDaemonStore((s) => consoleStatus(sessionsOf(indexSessions(s.sessions).byConsole, consoleId)));
}

/** Whether any session, in any status, is bound to the session `ownerId`. */
export function useHasBoundSessions(ownerId: string): boolean {
  return useDaemonStore((s) => indexSessions(s.sessions).boundOwnerIds.has(ownerId));
}

/** The session `id` names, or none: it changes only when that one session does. */
export function useSession(id: string | null | undefined): Session | undefined {
  return useDaemonStore((s) => (id ? s.sessions.get(id) : undefined));
}

/** The project the session `id` names belongs to, none for a console session or an unknown id: it
 * changes only when that does, not with the session's every status event. */
export function useSessionProjectId(id: string | null | undefined): string | null | undefined {
  return useDaemonStore((s) => (id ? s.sessions.get(id)?.project_id : undefined));
}

/** How many sessions are waiting for the user in the sidebar's tree (`countWaitingInTree`): a number,
 * so a status event that changes nothing about who waits renders nothing. */
export function useWaitingCount(): number {
  return useDaemonStore((s) => countWaitingInTree(s.consoles, s.projects, s.sessions));
}

/** The ids of the sessions waiting for the user, the same array until one starts or stops
 * waiting: a status event that changes nothing about who waits renders nothing. */
export function useWaitingIds(): string[] {
  return useDaemonStore(
    useShallow((s) => {
      const ids: string[] = [];
      for (const session of s.sessions.values()) if (session.status === "waiting_user") ids.push(session.id);
      return ids;
    }),
  );
}
