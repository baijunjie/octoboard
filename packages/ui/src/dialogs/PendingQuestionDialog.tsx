import React, { useEffect, useState } from "react";

import type { Session } from "../protocol";
import { useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { useConsoleRequestDialogProps } from "./ConsoleRequestDialog";
import { useTrustPromptDialogProps } from "./TrustPromptDialog";

/**
 * The one dialog for what the daemon is waiting on the user for: the trust prompts, oldest first,
 * then the requests for a console session, oldest first. One `ConfirmDialog` serves them all and
 * stays mounted from one to the next, its `resetKey` naming the current one, because remounting the
 * modal would leave its focus scope restoring focus to an element no longer in the page.
 *
 * A request for a console session already on screen stays there when a trust prompt arrives,
 * rather than being swapped for it under the user's pointer; the trust prompt follows it.
 */
export function PendingQuestionDialog({
  onConsoleSessionStarted,
}: {
  onConsoleSessionStarted: (consoleSession: Session, requestingProject: string | null) => void;
}): React.ReactElement | null {
  const trustPrompt = useDaemonStore((s) => s.trustPrompts[0]);
  const consoleRequest = useDaemonStore((s) => s.consoleRequests[0]);
  const sessionTitle = useDaemonStore((s) => (trustPrompt ? s.sessions.get(trustPrompt.session)?.title : undefined));
  const [onScreen, setOnScreen] = useState<string>();
  const requestFirst =
    consoleRequest !== undefined && (trustPrompt === undefined || onScreen === consoleRequest.requestId);
  const trust = useTrustPromptDialogProps(requestFirst ? undefined : trustPrompt, sessionTitle);
  const request = useConsoleRequestDialogProps(requestFirst ? consoleRequest : undefined, onConsoleSessionStarted);
  const shownRequest = requestFirst ? consoleRequest?.requestId : undefined;
  useEffect(() => setOnScreen(shownRequest), [shownRequest]);
  const props = trust ?? request;
  return props ? <ConfirmDialog {...props} /> : null;
}

/** Whether `PendingQuestionDialog` shows anything, for whatever keeps out of a modal's way. */
export function usePendingQuestion(): boolean {
  return useDaemonStore((s) => s.trustPrompts.length > 0 || s.consoleRequests.length > 0);
}
