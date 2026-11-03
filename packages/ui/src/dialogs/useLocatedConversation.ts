import { useEffect } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../errors/errorPresentation";
import { useT } from "../i18n/react";
import type { ConversationLocation, Event, RequestBody } from "../protocol";
import { shownError } from "./shownError";

/** `idle`: nothing to ask about. `pending`: the answer for the ID entered now is not in yet, which
 * includes the wait before it is asked. `ready`: the daemon has answered, with `location`. `failed`:
 * the daemon refused or answered with something else, with `error`. */
export type ConversationLookup =
  | { status: "idle" | "pending" }
  | { status: "ready"; location: ConversationLocation }
  | { status: "failed"; error: ErrorPresentation };

const PENDING: ConversationLookup = { status: "pending" };

/**
 * Asks the daemon where the conversation `id` is held (`locate_conversation_to_resume`, for
 * `project`) once `delayMs` has passed without `id` changing. `id` is `undefined` while there is
 * nothing to ask. Only the latest ID's answer counts: a reply for one the user has since changed, or
 * for a dialog that has closed, is dropped. A change of `refreshKey` asks again, for a caller whose
 * use of the answer depends on something the answer cannot see.
 */
export function useLocatedConversation(
  request: (body: RequestBody) => Promise<Event>,
  project: string,
  id: string | undefined,
  delayMs: number,
  refreshKey?: string,
): ConversationLookup {
  const t = useT();
  const key = id === undefined ? undefined : JSON.stringify([project, id, refreshKey]);
  const { value, send, reset, hold, asked, reconnects } = useLatestRequest<ConversationLookup>({
    scope: key, initial: PENDING,
  });
  const latest = useLatest({ request, project, id, send, t });

  useEffect(() => {
    // With nothing to ask, what was answered before is dropped, so coming back to the same entry starts pending.
    if (key === undefined) {
      if (asked()) reset();
      return;
    }
    // Whatever was asked or held before this run is no answer for it, even for the same ID: the conversation may
    // have been recorded, or archived, since it was asked.
    reset();
    hold(() => PENDING, key);
    const timer = setTimeout(() => {
      const { request, project, id, send } = latest.current;
      void send({
        request: () => request({ type: "locate_conversation_to_resume", project, agent_session_id: id! }),
        accept: (event) => {
          const lookup: ConversationLookup =
            event.type === "conversation_located"
              ? { status: "ready", location: event.location }
              : { status: "failed", error: latest.current.t("dialog.session.resumeLookupFailed") };
          return () => lookup;
        },
        failed: (failure, _held, t) => ({ status: "failed", error: shownError(failure, t) }),
      });
    }, delayMs);
    return () => clearTimeout(timer);
  }, [key, delayMs, reconnects]);

  return key === undefined ? { status: "idle" } : value;
}
