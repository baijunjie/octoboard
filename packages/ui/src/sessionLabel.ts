import { AGENT_LABEL, defaultAccountName } from "./agents";
import type { MessageKey, Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import { joinPhrases } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import type { Account, Console, Project, Session, SessionStatus, Settings } from "./protocol";
import type { Activity } from "./sidebar/order";

/** Keeps the agent's current task beside the session name the user created. Each name is
 * isolated separately so mixed reading directions cannot reorder them across the separator.
 * The result is for display only: a field that edits the name starts from `session.title`. */
export function sessionDisplayTitle(t: Translate, session: Session): string {
  return titleText(t, session, isolate);
}

/** A whole session choice label for matching and native options, without sentence isolation. */
export function sessionChoiceTitle(t: Translate, session: Session): string {
  return titleText(t, session, (value) => value);
}

function titleText(t: Translate, session: Session, mark: (value: string) => string): string {
  const terminalTitle = session.terminal_title?.trim();
  if (!terminalTitle || terminalTitle === session.title.trim()) return session.title;
  return t("session.displayTitle", { title: mark(terminalTitle), name: mark(session.title) });
}

/** Where to tell the user a session is, since the daemon's `Session` record itself only carries
 * ids. A console session has no project, so it is named for its console instead. The name comes
 * back isolated even where it is the whole answer rather than part of a sentence: a caller cannot
 * tell that case from the console session's wording, so isolating there instead would isolate the
 * sentence too. */
export function sessionLocation(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): string {
  if (session.project_id) {
    const project = projects.get(session.project_id);
    return project ? isolate(project.name) : t("session.location.project");
  }
  const owner = consoles.get(session.console_id);
  return owner ? t("session.location.consoleSession", { console: isolate(owner.name) }) : t("session.location.ownConsoleSession");
}

/** A session's title with where it is, for a line naming it outside the sidebar. A session titled
 * after its project would name it twice, so it reads as the title alone. */
export function sessionWithLocation(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): string {
  // Compared before either is isolated: the marks would only get in the way of the comparison.
  const project = session.project_id ? projects.get(session.project_id) : undefined;
  const title = sessionDisplayTitle(t, session);
  if (project?.name === session.title) return title;
  const location = sessionLocation(t, session, consoles, projects);
  return t("session.withLocation", { session: isolate(title), location });
}

/** What a status glyph and label stand for: the wire statuses, and *running in background*, which
 * is `idle` with work left running (`Session.background_tasks`) and has a glyph and a name of its
 * own. It is not a status of the daemon's: the session is as ready for a message as any idle one. */
export type StatusKind = SessionStatus | "background";

export function sessionStatusKind(session: Pick<Session, "status" | "background_tasks">): StatusKind {
  return session.status === "idle" && session.background_tasks?.length ? "background" : session.status;
}

/** The catalog message naming each status, for the "Session statuses" table in
 * docs/product/sessions.md. A row's own `aria-label` wins over one on an element nested inside it,
 * so the wire enum must never be the only place a status is put into words. */
const STATUS_KEY = {
  background: "session.status.background",
  starting: "session.status.starting",
  working: "session.status.working",
  waiting_user: "session.status.waitingUser",
  idle: "session.status.idle",
  interrupted: "session.status.interrupted",
  archived: "session.status.archived",
} as const satisfies Record<StatusKind, MessageKey>;

export function statusLabel(t: Translate, status: StatusKind): string {
  return t(STATUS_KEY[status]);
}

/** The account a session is shown under: the default account's own name for a session on it, the
 * account's name while it is stored, else the directory the session recorded (`isPath`, text of its
 * own direction) — or nothing when it recorded none. */
export function sessionAccount(t: Translate, session: Session, accounts: Account[], defaultNames: Settings["default_account_names"]): { name: string; isPath: boolean } | undefined {
  if (!session.account_id) return { name: defaultAccountName(t, defaultNames, session.agent), isPath: false };
  const stored = accounts.find((account) => account.id === session.account_id)?.name;
  if (stored !== undefined) return { name: stored, isPath: false };
  return session.config_dir != null ? { name: session.config_dir, isPath: true } : undefined;
}

/** The name a session's account is shown under: see `sessionAccount`. */
export function sessionAccountName(t: Translate, session: Session, accounts: Account[], defaultNames: Settings["default_account_names"]): string | undefined {
  return sessionAccount(t, session, accounts, defaultNames)?.name;
}

/** The agent and the account a session runs under, as one phrase: the agent part of the session's
 * accessible name, and what a dialog listing sessions reads out beside the agent's mark. */
export function sessionAgentLabel(t: Translate, session: Session, accounts: Account[], defaultNames: Settings["default_account_names"]): string {
  const agent = AGENT_LABEL[session.agent];
  const account = sessionAccountName(t, session, accounts, defaultNames);
  return account === undefined ? agent : t("session.agentAccount", { agent, account: isolate(account) });
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
  owner: Session | undefined,
  defaultNames: Settings["default_account_names"],
): string {
  const base = t(session.pinned ? "sidebar.session.ariaLabelPinned" : "sidebar.session.ariaLabel", {
    title: isolate(sessionDisplayTitle(t, session)),
    agent: sessionAgentLabel(t, session, accounts, defaultNames),
    status: statusLabel(t, sessionStatusKind(session)),
  });
  return owner ? joinPhrases(language, [base, t("sidebar.session.boundTo", { name: isolate(sessionDisplayTitle(t, owner)) })]) : base;
}

/** What an activity marker stands for, on its own, for the tooltip it carries; `undefined` for a
 * marker that is not drawn at all. The accessible name of the row or chip around it says the same
 * thing (see `activityLabelKey`), so the tooltip repeats rather than adds. */
export function activityLabel(t: Translate, activity: Activity): string | undefined {
  switch (activity) {
    case "waiting":
      return t("activity.waiting");
    case "working":
      return t("activity.working");
    case "running":
      return t("activity.running");
    default:
      return undefined;
  }
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
