import React, { useEffect, useState } from "react";

import { ConfirmConsequences } from "../../components/dialog/ConfirmConsequences";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { CodedError } from "../../errors/codedError";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import { SESSION_BINDING_CHANGED, SESSION_NOT_BOUND } from "../../protocol";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { useDaemon, useDaemonStore } from "../../store";

/** The confirmation of releasing a session from its owner, which only the session itself can undo.
 * Closes without a word when the binding is already gone: the session was deleted or released, or its owner
 * is gone, or the daemon refuses the release as `session_not_bound`. An archived session keeps its binding, but
 * the menu offers no Unbind on an archived row, so the dialog closes for it too. Any other refusal is shown
 * and the dialog stays, `session_binding_changed` included: the session is bound to another owner by then,
 * which the dialog reads live and so names. */
export function UnbindSessionDialog({ sessionId, onClose }: { sessionId: string; onClose: () => void }): React.ReactElement | null {
  const t = useT();
  const { request, store } = useDaemon();
  const session = useDaemonStore((s) => s.sessions.get(sessionId));
  const owner = useDaemonStore((s) => (session?.bound_to ? s.sessions.get(session.bound_to) : undefined));
  // The release itself clears the binding, which must not read as the session having gone.
  const [releasing, setReleasing] = useState(false);
  const gone = !releasing && (!session || !owner || session.status === "archived");
  useEffect(() => {
    if (gone) onClose();
  }, [gone]);
  if (!session || !owner) return null;
  const ownerName = <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, owner)}</InlineReference>;
  return (
    <ConfirmDialog
      title={t("dialog.unbindSession.title")}
      subject={<Message id="dialog.unbindSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference>, owner: ownerName }} />}
      message={t("dialog.unbindSession.answersToYou")}
      consequences={<ConfirmConsequences lines={[{ key: "owner", text: t("dialog.unbindSession.consequence") }]} />}
      confirmLabel={t("dialog.unbindSession.confirm")}
      errorTitle={t("dialog.unbindSession.error")}
      onCancel={onClose}
      onConfirm={async () => {
        setReleasing(true);
        try {
          await request({ type: "release_session", session: session.id });
        } catch (err) {
          const code = err instanceof CodedError ? err.code : undefined;
          // Refused as changed while still bound (to another owner, which the dialog then names): the user
          // decides again. Once the store shows it unbound there is nothing left to decide.
          if (code === SESSION_NOT_BOUND || (code === SESSION_BINDING_CHANGED && !store.getState().sessions.get(sessionId)?.bound_to)) {
            onClose();
            return;
          }
          setReleasing(false);
          throw err;
        }
        onClose();
      }}
    />
  );
}
