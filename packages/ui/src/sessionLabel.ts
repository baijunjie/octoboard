import { AGENT_LABEL } from "./agents";
import type { MessageKey, Translate } from "./i18n/catalog";
import { joinPhrases } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import type { Account, Console, Project, Session, SessionStatus } from "./protocol";
import type { Activity } from "./sidebar/order";

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

/** A session's title with where it is, for a line naming it outside the sidebar. A session titled
 * after its project would name it twice, so it reads as the title alone. */
export function sessionWithLocation(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): string {
  const location = sessionLocation(t, session, consoles, projects);
  return location === session.title ? session.title : t("session.withLocation", { session: session.title, location });
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

/** The name a session's account is shown under: the default account's own name for a session on
 * it, the account's name while it is stored, else the directory the session recorded — or nothing
 * when it recorded none. */
export function sessionAccountName(t: Translate, session: Session, accounts: Account[]): string | undefined {
  if (!session.account_id) return t("settings.accounts.defaultName");
  return accounts.find((account) => account.id === session.account_id)?.name ?? session.config_dir ?? undefined;
}

/** What the agent's mark on a session's row, card or archive list entry says in its tooltip: the
 * account the session runs under, or the agent's own name where that cannot be resolved. */
export function sessionAccountTooltip(t: Translate, session: Session, accounts: Account[]): string {
  return sessionAccountName(t, session, accounts) ?? AGENT_LABEL[session.agent];
}

/** A session's agent followed by the account it runs under, as plain text: for an accessible name
 * or a tooltip, where `AgentAccountText` is not needed. */
export function sessionAgentLabel(t: Translate, session: Session, accounts: Account[]): string {
  const agent = AGENT_LABEL[session.agent];
  const account = sessionAccountName(t, session, accounts);
  return account === undefined ? agent : t("session.agentAccount", { agent, account });
}

/** A session row's accessible name: the row is one button, so its icons are not announced, and the
 * title, agent and its account, status and pin all have to be in this. `owner` is the session this
 * (project) session is bound to, and naming it here is what tells assistive technology who it
 * reports to. For a console session that is the fact the binding badge shows visually — the badge
 * carries no information its tooltip does not, so it carries none the row's own label does not
 * either. For a project session there is no badge at all: the row is listed under its owner, and
 * this name is the only place that nesting is put into words. */
export function sessionAriaLabel(
  t: Translate,
  language: Language,
  session: Session,
  accounts: Account[],
  owner?: Session,
): string {
  const base = t(session.pinned ? "sidebar.session.ariaLabelPinned" : "sidebar.session.ariaLabel", {
    title: session.title,
    agent: sessionAgentLabel(t, session, accounts),
    status: statusLabel(t, session.status),
  });
  return owner ? joinPhrases(language, [base, t("sidebar.session.boundTo", { name: owner.title })]) : base;
}

/** The message that names a console or a project together with what is going on beneath it, for its
 * accessible name; `undefined` for one with nothing going on, whose own name says enough. A pinned
 * project's variant says so too. */
export function activityLabelKey(activity: Activity, pinned = false) {
  switch (activity) {
    case "waiting":
      return pinned ? ("sidebar.activity.waitingPinned" as const) : ("sidebar.activity.waiting" as const);
    case "working":
      return pinned ? ("sidebar.activity.workingPinned" as const) : ("sidebar.activity.working" as const);
    case "running":
      return pinned ? ("sidebar.activity.runningPinned" as const) : ("sidebar.activity.running" as const);
    default:
      return undefined;
  }
}
