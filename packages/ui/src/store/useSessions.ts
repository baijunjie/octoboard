import { useShallow } from "zustand/react/shallow";

import type { Session } from "../protocol";
import { useDaemonStore } from "./react";
import { topStatus } from "../sessions/order";
import { holdsAccountSwitch, isSummaryStatus, sessionShownStatusKind, type ShownStatusKind } from "../sessions/sessionStatus";
import { countWaitingInTree } from "../sessions/waiting";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";

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

/** The avatar status: the top eligible status among this console's sessions. */
export function useConsoleStatus(consoleId: string): ShownStatusKind | undefined {
  return useDaemonStore((s) => {
    const index = indexSessions(s.sessions);
    const listed = sessionsOf(index.byConsole, consoleId).filter((session) => isSummaryStatus(sessionShownStatusKind(session, index.delegatingIds, s.queuedAccountSwitches)));
    return topStatus(listed, index.delegatingIds, s.queuedAccountSwitches);
  });
}

/** Counts the shown statuses of a console's unarchived sessions, including its console sessions. */
export function useConsoleStatusCounts(consoleId: string): Partial<Record<ShownStatusKind, number>> {
  return useDaemonStore(useShallow((s) => {
    const index = indexSessions(s.sessions);
    const counts: Partial<Record<ShownStatusKind, number>> = {};
    for (const session of sessionsOf(index.byConsole, consoleId)) {
      if (session.status === "archived") continue;
      const status = sessionShownStatusKind(session, index.delegatingIds, s.queuedAccountSwitches);
      counts[status] = (counts[status] ?? 0) + 1;
    }
    return counts;
  }));
}

/** The top status (`topStatus`) of all sessions of a project, a kind, so a collapsed project's row renders again only
 * when its marker changes. */
export function useProjectStatus(projectId: string): ShownStatusKind | undefined {
  return useDaemonStore((s) => {
    const index = indexSessions(s.sessions);
    return topStatus(sessionsOf(index.byProject, projectId), index.delegatingIds, s.queuedAccountSwitches);
  });
}

/** The ids of the sessions with delegated work in progress (`delegatingSessionIds`): the same set
 * until a session starts or stops delegating, for what ranks or names sessions by their status. */
export function useDelegatingIds(): ReadonlySet<string> {
  return useDaemonStore((s) => indexSessions(s.sessions).delegatingIds);
}

/** The sessions an account-switch batch of this window has not finished with (`State.queuedAccountSwitches`): the
 * same set until a session joins or leaves one, for what ranks or shows sessions by their status. */
export function useQueuedAccountSwitches(): ReadonlySet<string> {
  return useDaemonStore((s) => s.queuedAccountSwitches);
}

/** What `session` shows as its status: a kind, so a row renders again only when its kind changes. */
export function useSessionShownStatus(session: Session): ShownStatusKind {
  return useDaemonStore((s) => sessionShownStatusKind(session, indexSessions(s.sessions).delegatingIds, s.queuedAccountSwitches));
}

/** Whether an account switch holds `session` (`holdsAccountSwitch`), the daemon's or one queued in this window; false
 * for no session. */
export function useAccountSwitchHeld(session: Pick<Session, "id" | "switching_account"> | undefined): boolean {
  return useDaemonStore((s) => session !== undefined && holdsAccountSwitch(session, s.queuedAccountSwitches));
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
