import { useEffect, useRef } from "react";

import { useStableHandler } from "../../components/hooks/useLatest";
import { useT } from "../../i18n/react";
import { useDaemon } from "../../store";
import { raiseError } from "../../components/alerts/raiseError";

/** The daemon checks the unused state atomically before deleting and cleaning up native records. */
export function DeleteUnusedSession({ sessionId, onClose }: { sessionId: string; onClose: () => void }): null {
  const t = useT();
  const { request } = useDaemon();
  const close = useStableHandler(onClose);
  const deletion = useRef<ReturnType<typeof request> | undefined>(undefined);
  useEffect(() => {
    let active = true;
    // Reuse the request when Strict Mode reconnects this effect.
    if (deletion.current === undefined) deletion.current = request({ type: "delete_session", session: sessionId, only_if_unused: true });
    void deletion.current.then(() => {
      if (active) close();
    }, (err: unknown) => {
      if (!active) return;
      raiseError(err, { title: t("dialog.deleteSession.failedTitle"), session: sessionId });
      close();
    });
    return () => { active = false; };
  }, [sessionId, request, close, t]);
  return null;
}
