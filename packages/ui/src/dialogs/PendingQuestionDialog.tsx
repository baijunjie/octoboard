import React, { useEffect, useLayoutEffect, useState } from "react";

import { useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { useSessionRequestDialogProps } from "./SessionRequestDialog";
import { useTrustPromptDialogProps } from "./TrustPromptDialog";

/**
 * The one dialog for what the daemon is waiting on the user for: the trust prompts, oldest first,
 * then the session requests, oldest first. One `ConfirmDialog` serves them all and stays mounted
 * from one to the next, its `resetKey` naming the current one, because remounting the modal would
 * leave its focus scope restoring focus to an element no longer in the page. Every one of them
 * comes up unasked, so none of them takes an Enter before the user moves focus.
 *
 * A request already on screen stays there while it is waiting, rather than being swapped under the
 * user's pointer for a trust prompt or for a request that sorts before it; the next one follows.
 */
export function PendingQuestionDialog(): React.ReactElement | null {
  const trustPrompt = useDaemonStore((s) => s.trustPrompts[0]);
  const requests = useDaemonStore((s) => s.sessionRequests);
  const sessionTitle = useDaemonStore((s) => (trustPrompt ? s.sessions.get(trustPrompt.session)?.title : undefined));
  const [onScreen, setOnScreen] = useState<string>();
  const request = requests.find((r) => r.requestId === onScreen) ?? requests[0];
  const requestFirst = request !== undefined && (trustPrompt === undefined || onScreen === request.requestId);
  const trust = useTrustPromptDialogProps(requestFirst ? undefined : trustPrompt, sessionTitle);
  const requestDialog = useSessionRequestDialogProps(requestFirst ? request : undefined);
  const shownRequest = requestFirst ? request.requestId : undefined;
  useEffect(() => setOnScreen(shownRequest), [shownRequest]);
  const props = trust ?? requestDialog;
  // Comes up from the commit after the one that raised the first question. A dialog opened in the same commit (a
  // question pushed in the task of the press that opens it) hides the rest of the page with react-aria's
  // `ariaHideOutside` from a passive effect of that commit, and with the question already in the document it would hide
  // the question too, for good: the question's own hider never shows what another one hid. React runs those passive
  // effects before it renders again, and the state set from this layout effect makes that render a synchronous one in
  // the same task.
  const asked = props !== undefined;
  const [shown, setShown] = useState(false);
  useLayoutEffect(() => setShown(asked), [asked]);
  return props && shown ? <ConfirmDialog {...props} unprompted /> : null;
}

/** Whether `PendingQuestionDialog` shows anything, for whatever keeps out of a modal's way. */
export function usePendingQuestion(): boolean {
  return useDaemonStore((s) => s.trustPrompts.length > 0 || s.sessionRequests.length > 0);
}
