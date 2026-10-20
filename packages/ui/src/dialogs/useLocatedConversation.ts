import { useEffect, useState } from "react";

import { useLatest } from "../components/useLatest";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import type { ConversationLocation, Event, RequestBody } from "../protocol";

/** `idle`: nothing to ask about. `pending`: the answer for the ID entered now is not in yet, which
 * includes the wait before it is asked. `ready`: the daemon has answered, with `location`. `failed`:
 * the daemon refused or answered with something else, with `error`. */
export type ConversationLookup =
  | { status: "idle" | "pending" }
  | { status: "ready"; location: ConversationLocation }
  | { status: "failed"; error: ErrorPresentation };

type Answer = { key: string; location?: ConversationLocation; error?: ErrorPresentation };

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
  const [answered, setAnswered] = useState<Answer>();
  const latest = useLatest({ request, project, id, t });

  useEffect(() => {
    // An answer held from an earlier run is no answer for this one, even for the same ID: the
    // conversation may have been recorded, or archived, since it was asked.
    setAnswered(undefined);
    if (key === undefined) return;
    let current = true;
    const timer = setTimeout(() => {
      const { request, project, id, t } = latest.current;
      request({ type: "locate_conversation_to_resume", project, agent_session_id: id! }).then(
        (event) => {
          if (!current) return;
          setAnswered(
            event.type === "conversation_located"
              ? { key, location: event.location }
              : { key, error: t("dialog.session.resumeLookupFailed") },
          );
        },
        (err: Error) => current && setAnswered({ key, error: errorPresentation(err) }),
      );
    }, delayMs);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [key, delayMs]);

  if (key === undefined) return { status: "idle" };
  if (answered?.key !== key) return { status: "pending" };
  return answered.location ? { status: "ready", location: answered.location } : { status: "failed", error: answered.error! };
}
