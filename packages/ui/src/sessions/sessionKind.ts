import type { Session, SessionKind } from "../protocol";
import { knownValues } from "../values/knownValue";

/** Whether `raw` is a kind of session this build knows. */
export const isSessionKind = knownValues<SessionKind>({ console: true, project: true });

/** Whether a console lists `session` among its console sessions: a console session, or one of a kind this build does
 * not know (a newer daemon's) that belongs to no project, which is shown there and nothing console-session-specific is
 * offered for it (those read `kind === "console"`). */
export function listedAmongConsoleSessions(session: Pick<Session, "kind" | "project_id">): boolean {
  return session.kind === "console" || (!isSessionKind(session.kind) && !session.project_id);
}
