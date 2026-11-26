import type { Session, StandingNotice } from "../protocol";

/** What tells one standing notice of a session from every other: its session, code, `detail` param and the time it was
 * raised (`Session.standing_notices`). A notice raised again, by a later launch or with a new reading, has a new time,
 * so a dismissal of the earlier one does not hide it. */
export function standingNoticeKey(session: string, notice: StandingNotice): string {
  return JSON.stringify([session, notice.code, notice.params.detail ?? null, notice.at]);
}

/** The session a `standingNoticeKey` belongs to. */
export function standingNoticeSession(key: string): string {
  const [session] = JSON.parse(key) as [string];
  return session;
}

/** The standing notices of `session` this window shows: those on its record that the user has not dismissed here, in
 * the order the daemon raised them (the newest last). */
export function shownStandingNotices(session: Session | undefined, dismissed: ReadonlySet<string>): readonly StandingNotice[] {
  const notices = session?.standing_notices;
  if (!session || !notices?.length) return NO_NOTICES;
  const shown = notices.filter((notice) => !dismissed.has(standingNoticeKey(session.id, notice)));
  return shown.length === notices.length ? notices : shown;
}

const NO_NOTICES: readonly StandingNotice[] = [];
