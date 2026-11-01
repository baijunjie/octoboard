import type { Event } from "../protocol";
import { TRUST_FAILURE_CODES } from "../protocol";

type SessionNotice = Extract<Event, { type: "session_notice" }>;

/** Keeps a window from telling a failed trust go-ahead twice. A `confirm_trust` failing with one of
 * `TRUST_FAILURE_CODES` is both rejected to the window that pressed it, which raises an alert for it, and broadcast
 * as a `session_notice`, and the two reach the window in either order. For the pressing window the notice is
 * therefore redundant, but only when it is that request's own failure: a notice arriving while a go-ahead is in
 * flight may as well be a later failure of an earlier go-ahead (the agent's trust entry failing to carry over after
 * the press was answered), so it is held until the request settles and emitted unless the rejection matches it.
 * When the rejection comes first, a token swallows the notice that follows.
 *
 * A snapshot means the notice may never come (lag, reconnect), so it drops the tokens and starts a new generation;
 * requests in flight are not touched, since their replies still arrive. */
export function createOwnTrustNotices(emit: (notice: SessionNotice) => void) {
  const sessions = new Map<string, { inFlight: number; held: SessionNotice[] }>();
  const tokens = new Set<string>();
  let generation = 0;
  const token = (session: string, code: string) => `${session}\n${code}`;

  const flush = (session: string) => {
    const entry = sessions.get(session);
    if (!entry || entry.inFlight > 0) return;
    sessions.delete(session);
    entry.held.forEach(emit);
  };

  return {
    /** Registers a go-ahead sent for `session`; the returned function is called once with the code it was rejected
     * with, or nothing when it succeeded or failed without a code. Held notices of a session are emitted when its
     * last go-ahead in flight settles, as a notice may belong to any of them. */
    begin(session: string): (rejection?: string) => void {
      const entry = sessions.get(session) ?? { inFlight: 0, held: [] };
      sessions.set(session, entry);
      entry.inFlight += 1;
      const sentIn = generation;
      return (rejection) => {
        entry.inFlight -= 1;
        if (rejection !== undefined && TRUST_FAILURE_CODES.has(rejection)) {
          const index = entry.held.findIndex((notice) => notice.code === rejection);
          if (index >= 0) entry.held.splice(index, 1);
          // A snapshot since the request was sent may have cost the notice that came before it, so a token then
          // would only risk swallowing an unrelated one later: better a duplicate than a lost notice.
          else if (sentIn === generation) tokens.add(token(session, rejection));
        }
        flush(session);
      };
    },

    /** Takes `notice` over when it is the pressing window's own and returns true: swallowed once by the token a
     * rejection left, or else held while a go-ahead is in flight and `emit`ted when it settles unless that rejection
     * is its own. A token goes first, as it is a rejection's already settled claim. Returns false for a notice that is
     * nobody's to hold, which the caller shows. */
    claim(notice: SessionNotice): boolean {
      if (!TRUST_FAILURE_CODES.has(notice.code)) return false;
      if (tokens.delete(token(notice.session, notice.code))) return true;
      const entry = sessions.get(notice.session);
      if (!entry) return false;
      entry.held.push(notice);
      return true;
    },

    /** Forgets what a snapshot makes unreliable: the tokens, and the claim that a rejection can tell its notice
     * was not lost. Held notices stay, as they did arrive. */
    reset() {
      tokens.clear();
      generation += 1;
    },
  };
}
