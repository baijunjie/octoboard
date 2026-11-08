import type { Session, SessionStatus } from "../protocol";

/** What a status glyph and label stand for: the wire statuses, and four kinds the client derives
 * from an `idle` session, none of them a status of the daemon's (the session is as ready for a
 * message as any idle one): *paused*, a session whose last turn ended abnormally (`Session.paused`);
 * *working in background*, work left running (`Session.background_tasks`);
 * *delegated*, work in progress in the sessions under it (`delegatingSessionIds`); and *done*, a
 * session that has a conversation — a turn taken, or one resumed at open — with nothing left going
 * (`Session.has_conversation`). */
export type StatusKind = SessionStatus | "paused" | "background" | "delegated" | "done";

/** Whether an idle session has work left running in the background. */
export function runsInBackground(session: Pick<Session, "status" | "background_tasks">): boolean {
  return session.status === "idle" && !!session.background_tasks?.length;
}

/** The wire statuses of a session at work: starting, working, or dormant, whose background agents may be at work. */
const WORKING_STATUSES: ReadonlySet<SessionStatus> = new Set(["starting", "working", "dormant"]);

/** Whether `session` is at work: starting, working or dormant, or idle with work left running in the background (a
 * paused one included). A session waiting for the user is not, which is what separates what holds the computer awake
 * from what holds a quit up (`quitHolders`). */
export function sessionIsWorking(session: Session): boolean {
  return WORKING_STATUSES.has(session.status) || runsInBackground(session);
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

/** The kind of a session by what it does on its own first: any wire status but `idle`, then whether its
 * last turn ended abnormally (`paused`), then work left in the background, then work delegated to the sessions under it (`delegating`, see
 * `delegatingSessionIds`), then whether its conversation has begun (`has_conversation`). */
export function sessionStatusKind(session: Pick<Session, "id" | "status" | "paused" | "background_tasks" | "has_conversation">, delegating: ReadonlySet<string>): StatusKind {
  if (session.status !== "idle") return session.status;
  if (session.paused) return "paused";
  if (runsInBackground(session)) return "background";
  if (delegating.has(session.id)) return "delegated";
  return session.has_conversation ? "done" : "idle";
}

/** What a status glyph and label show: a `StatusKind`, or *switching account*, which a session
 * shows over its own status while an account switch holds it (`holdsAccountSwitch`). Only
 * what is drawn follows it; ordering and the other decisions about a session go by `StatusKind`. */
export type ShownStatusKind = StatusKind | "switching";

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

export function sessionShownStatusKind(
  session: Pick<Session, "id" | "status" | "paused" | "background_tasks" | "has_conversation" | "switching_account">,
  delegating: ReadonlySet<string>,
  queuedSwitches: ReadonlySet<string>,
): ShownStatusKind {
  return showsSwitching(session, queuedSwitches) ? "switching" : sessionStatusKind(session, delegating);
}

/** The statuses surfaced by the menu bar list and console avatars. Other statuses stay on session rows. */
export const SUMMARY_STATUSES = ["waiting_user", "paused", "done", "idle"] as const satisfies readonly ShownStatusKind[];

export function isSummaryStatus(status: ShownStatusKind): boolean {
  return SUMMARY_STATUSES.some((kind) => kind === status);
}

/** Statuses meaning the session has a running process or is about to: `starting` is one whose agent
 * is still being launched, so it has no terminal to attach to yet. The complement of
 * `NO_PROCESS_STATUSES`. */
export const LIVE_STATUSES: ReadonlySet<SessionStatus> = new Set(["starting", "working", "waiting_user", "dormant", "idle"]);

/** Statuses meaning no process is running for this session — both are resumable via
 * `resume_session`. */
export const NO_PROCESS_STATUSES: ReadonlySet<SessionStatus> = new Set(["interrupted", "archived"]);

export function isLive(status: SessionStatus): boolean {
  return LIVE_STATUSES.has(status);
}

export function hasNoProcess(status: SessionStatus): boolean {
  return NO_PROCESS_STATUSES.has(status);
}
