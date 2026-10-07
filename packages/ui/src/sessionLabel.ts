import { AGENT_LABEL } from "./agents";
import type { MessageKey, Translate } from "./i18n/catalog";
import { joinPhrases } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import type { Console, Project, Session, SessionStatus } from "./protocol";

/** Where to tell the user a session is, since the daemon's `Session` record itself only carries
 * ids. A console session has no project, so it is named for its console instead. */
export function sessionLocation(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): string {
  if (session.project_id) return projects.get(session.project_id)?.name ?? t("session.location.project");
  const owner = consoles.get(session.console_id);
  return owner ? t("session.location.consoleSession", { console: owner.name }) : t("session.location.ownConsoleSession");
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

/** A session row's accessible name: the row is one button, so its icons are not announced, and
 * the title, agent, status and pin all have to be in this. `owner`, the console session this
 * (project) session is bound to, folds in the fact the binding badge shows visually — the badge
 * carries no information its tooltip does not, so it carries none the row's own label does not
 * either. */
export function sessionAriaLabel(t: Translate, language: Language, session: Session, owner?: Session): string {
  const base = t(session.pinned ? "sidebar.session.ariaLabelPinned" : "sidebar.session.ariaLabel", {
    title: session.title,
    agent: AGENT_LABEL[session.agent],
    status: statusLabel(t, session.status),
  });
  return owner ? joinPhrases(language, [base, t("sidebar.session.boundTo", { name: owner.title })]) : base;
}
