import type { MessageKey, Translate } from "./i18n/catalog";
import type { Console, Project, Session, SessionStatus } from "./protocol";

/** Where to tell the user a session is, since the daemon's `Session` record itself only carries
 * ids. A hub session has no project, so it is named for its console instead. */
export function sessionLocation(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): string {
  if (session.project_id) return projects.get(session.project_id)?.name ?? t("session.location.project");
  const owner = consoles.get(session.console_id);
  return owner ? t("session.location.hub", { console: owner.name }) : t("session.location.ownHub");
}

/** The catalog message naming each status, for the "Session statuses" table in
 * docs/product/sessions.md. A row's own `aria-label` wins over one on an element nested inside it,
 * so the wire enum must never be the only place a status is put into words. */
const STATUS_KEY = {
  working: "session.status.working",
  waiting_user: "session.status.waitingUser",
  idle: "session.status.idle",
  interrupted: "session.status.interrupted",
  archived: "session.status.archived",
} as const satisfies Record<SessionStatus, MessageKey>;

export function statusLabel(t: Translate, status: SessionStatus): string {
  return t(STATUS_KEY[status]);
}
