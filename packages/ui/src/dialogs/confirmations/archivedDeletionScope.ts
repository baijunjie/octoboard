import type { Session } from "../../protocol";
import { boundArchivedSessions } from "../../sessions/order";
import type { DialogRequest } from "../dialogRequest";

type DeleteArchivedRequest = Extract<DialogRequest, { kind: "delete-archived" }>;

/** What deleting the archived sessions of `request`'s scope takes, from `sessions`: `count` is how many
 * the archive view lists for that scope, `withThem` how many archived sessions go along with those, and
 * `targetIds` every session the deletion removes. All of a console's archived console sessions take
 * their archived bound sessions with them; a project's archive, and a console session's own, do not
 * reach any console session. */
export function archivedDeletionScope(sessions: Session[], request: DeleteArchivedRequest): { count: number; withThem: number; targetIds: string[] } {
  const consoleSessions = request.project || request.consoleSession
    ? []
    : sessions.filter((s) => s.console_id === request.console.id && s.role === "console" && s.status === "archived");
  const withThem = consoleSessions.flatMap((owner) => boundArchivedSessions(sessions, owner.id)).length;
  const roots = request.project
    ? sessions.filter((s) => s.project_id === request.project!.id && s.status === "archived")
    : request.consoleSession
      ? boundArchivedSessions(sessions, request.consoleSession.id)
      : consoleSessions;
  const targets = new Map(roots.flatMap((s) => [s, ...boundArchivedSessions(sessions, s.id)]).map((s) => [s.id, s]));
  return { count: roots.length, withThem, targetIds: Array.from(targets.keys()) };
}
