import type { Session, SessionStatus } from "../protocol";
import { knownValues } from "../values/knownValue";

/** Whether `raw`, a status the daemon sent, is one this client knows (`SessionStatus`); a newer daemon's is not, and is
 * the `unknown` kind (`sessionStatusKind`). */
export const isKnownSessionStatus = knownValues<SessionStatus>({ starting: true, working: true, waiting_user: true, dormant: true, idle: true, interrupted: true, archived: true });

/** What a status glyph and label stand for: the wire statuses, *unknown* for a status this client does not
 * know (a newer daemon's, which the record is still shown with and nothing acts on), and four kinds the client
 * derives from an `idle` session, none of them a status of the daemon's (the session is as ready for a
 * message as any idle one): *paused*, a session whose last turn ended abnormally (`Session.paused`);
 * *working in background*, work left running (`Session.background_tasks`);
 * *delegated*, work in progress in the sessions under it (`delegatingSessionIds`); and *done*, a
 * session that has a conversation — a turn taken, or one resumed at open — with nothing left going
 * (`Session.has_conversation`). */
export type StatusKind = SessionStatus | "unknown" | "paused" | "background" | "delegated" | "done";

/** Whether an idle session has work left running in the background. */
export function runsInBackground(session: Pick<Session, "status" | "background_tasks">): boolean {
  return session.status === "idle" && !!session.background_tasks?.length;
}

/** The wire statuses of a session at work: starting, working, or dormant, whose background agents may be at work. */
const WORKING_STATUSES: ReadonlySet<string> = new Set<SessionStatus>(["starting", "working", "dormant"]);

/** Whether `session` is at work: starting, working or dormant, or idle with work left running in the background (a
 * paused one included). A session waiting for the user is not, which is what separates what holds the computer awake
 * from what holds a quit up (`quitHolders`). The daemon's `keep_awake::session_is_working` decides what holds the
 * computer awake and mirrors this function; the two must stay in step. */
export function sessionIsWorking(session: Session): boolean {
  return WORKING_STATUSES.has(session.status) || runsInBackground(session);
}

/** Whether `session` is still at its turn or may go on without anyone acting: working, waiting for the user (whose
 * answer lets it go on), showing its background agents, or idle with work left running in the background. A session
 * still starting is not counted here, as it has no process yet; and one shown waiting only for a usage-limit dialog
 * (`Session.at_usage_limit`) is at rest, its turn having stopped on the account's limit, which is what a handoff is
 * for, unless it also has work left running in the background (the daemon serves its `background_tasks` then, though
 * its status is not `idle`). What a handoff waits out, as the daemon's `refuse_unless_it_can_be_handed_off` does, over
 * the same notion of the status (`status_for_switch`). */
export function sessionIsBusy(session: Pick<Session, "status" | "background_tasks" | "at_usage_limit">): boolean {
  if (session.status === "waiting_user") return !session.at_usage_limit || !!session.background_tasks?.length;
  return session.status === "working" || session.status === "dormant" || runsInBackground(session);
}

/** Whether any session bound to `ownerId` can still send it something without the owner or the user acting first: it
 * is starting or busy (`sessionIsBusy`). A child at rest (idle with nothing left in the background, interrupted or
 * archived) cannot. Messages queued for a child, which the records do not show, are the daemon's to judge. */
export function hasChildThatCanReport(sessions: Iterable<Session>, ownerId: string): boolean {
  for (const child of sessions) {
    if (child.bound_to === ownerId && childCanReport(child)) return true;
  }
  return false;
}

/** Whether `child`, a session bound to an owner, can still send the owner something without anyone acting first: it is
 * starting or busy (`sessionIsBusy`). */
export function childCanReport(child: Pick<Session, "status" | "background_tasks" | "at_usage_limit">): boolean {
  return child.status === "starting" || sessionIsBusy(child);
}

/** The ids of the sessions with delegated work in progress: those some session in `sessions` that is
 * starting, working, dormant, waiting for the user or idle with work left running in the background
 * (`runsInBackground`) is bound to, directly or through the sessions between (`bound_to`), whatever their own
 * status. One pass: each such session walks up its chain of owners. The chain carries a seen set, so a binding
 * that leads back on itself ends the walk. */
export function delegatingSessionIds(sessions: Iterable<Session>): ReadonlySet<string> {
  const byId = new Map<string, Session>();
  const active: Session[] = [];
  for (const session of sessions) {
    byId.set(session.id, session);
    if (session.status === "starting" || session.status === "working" || session.status === "waiting_user" || session.status === "dormant" || runsInBackground(session)) active.push(session);
  }
  const delegating = new Set<string>();
  for (const session of active) {
    const seen = new Set([session.id]);
    for (let owner: string | null | undefined = session.bound_to; owner && !seen.has(owner); owner = byId.get(owner)?.bound_to) {
      seen.add(owner);
      delegating.add(owner);
    }
  }
  return delegating;
}

/** The kind of a session by what it does on its own first: any wire status but `idle` (`unknown` for one this client
 * does not know, which is neither live nor working and which nothing is offered for), then whether its
 * last turn ended abnormally (`paused`), then work left in the background, then work delegated to the sessions under it (`delegating`, see
 * `delegatingSessionIds`), then whether its conversation has begun (`has_conversation`). */
export function sessionStatusKind(session: Pick<Session, "id" | "status" | "paused" | "background_tasks" | "has_conversation">, delegating: ReadonlySet<string>): StatusKind {
  if (!isKnownSessionStatus(session.status)) return "unknown";
  if (session.status !== "idle") return session.status;
  if (session.paused) return "paused";
  if (runsInBackground(session)) return "background";
  if (delegating.has(session.id)) return "delegated";
  return session.has_conversation ? "done" : "idle";
}

/** What a status glyph and label show: a `StatusKind`, or one of two kinds a session shows over its own status:
 * *switching account*, while an account switch holds it (`holdsAccountSwitch`), and *reconnecting*, while the daemon
 * cannot reach its process (`holdsPendingLink`) and the status it stores may be stale. Account switches also affect
 * status ordering. Reconnecting affects whether the session counts as waiting for the user (`waitsForUser`). */
export type ShownStatusKind = StatusKind | "switching" | "reconnecting";

/** Whether an account switch holds `session`: the daemon says one does (`Session.switching_account`), or an
 * account-switch batch of this window has not finished with it, queued or running (`queuedSwitches`, the store's
 * `queuedAccountSwitches`, which the daemon knows nothing of). The store keeps the daemon's record as sent, so a
 * session's own `switching_account` alone misses the second; whatever waits on a switch asks this. */
export function holdsAccountSwitch(session: Pick<Session, "id" | "switching_account">, queuedSwitches: ReadonlySet<string>): boolean {
  return session.switching_account === true || queuedSwitches.has(session.id);
}

/** Whether a session shows *switching account* over its own status: an account switch holds it
 * (`holdsAccountSwitch`), and an archived session is past any switch. */
export function showsSwitching(session: Pick<Session, "id" | "status" | "switching_account">, queuedSwitches: ReadonlySet<string>): boolean {
  return holdsAccountSwitch(session, queuedSwitches) && session.status !== "archived";
}

/** Whether the daemon cannot reach the process of `session` (`Session.holder_link` is `pending`) while it is live. It
 * is the daemon's own retry to resolve: the session keeps its stored status, which may be stale until the link is
 * back. An archived or interrupted record has no process to reach, so a stale flag on one means nothing. */
export function holdsPendingLink(session: Pick<Session, "status" | "holder_link">): boolean {
  return session.holder_link === "pending" && isLive(session.status);
}

/** What `session` shows as its status: *reconnecting* over everything (`holdsPendingLink`), else *switching account*
 * (`showsSwitching`), else its `StatusKind`. */
export function sessionShownStatusKind(
  session: Pick<Session, "id" | "status" | "paused" | "background_tasks" | "has_conversation" | "switching_account" | "holder_link">,
  delegating: ReadonlySet<string>,
  queuedSwitches: ReadonlySet<string>,
): ShownStatusKind {
  if (holdsPendingLink(session)) return "reconnecting";
  return showsSwitching(session, queuedSwitches) ? "switching" : sessionStatusKind(session, delegating);
}

/** The statuses surfaced by the menu bar list and console avatars. Other statuses stay on session rows, so a
 * reconnecting session is none of Done, Idle or Waiting for you there. */
export const SUMMARY_STATUSES = ["waiting_user", "paused", "done", "idle"] as const satisfies readonly ShownStatusKind[];

export function isSummaryStatus(status: ShownStatusKind): boolean {
  return SUMMARY_STATUSES.some((kind) => kind === status);
}

/** Statuses meaning the session has a running process or is about to: `starting` is one whose agent
 * is still being launched, so it has no terminal to attach to yet. Together with `NO_PROCESS_STATUSES` it leaves out
 * only a status this client does not know. Typed on its members, so that one that is not a `SessionStatus` is an error
 * here. */
export const LIVE_STATUSES: ReadonlySet<string> = new Set<SessionStatus>(["starting", "working", "waiting_user", "dormant", "idle"]);

/** Statuses meaning no process is running for this session — both are resumable via
 * `resume_session`. */
export const NO_PROCESS_STATUSES: ReadonlySet<string> = new Set<SessionStatus>(["interrupted", "archived"]);

/** Whether `status`, as the daemon sent it, is a live one; a status this client does not know is not. */
export function isLive(status: string): boolean {
  return LIVE_STATUSES.has(status);
}

/** Whether `status`, as the daemon sent it, is one without a process; a status this client does not know is not
 * (nothing resumes a session on a guess). */
export function hasNoProcess(status: string): boolean {
  return NO_PROCESS_STATUSES.has(status);
}
