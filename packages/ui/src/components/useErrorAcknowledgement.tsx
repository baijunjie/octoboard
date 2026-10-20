import { useEffect } from "react";

import { errorAlerts } from "../errorAlerts";
import type { ErrorPresentation } from "../errorPresentation";
import type { ErrorRecovery } from "../errorRecovery";
import { errorText } from "../errorText";
import { DaemonErrorMessage } from "./DaemonErrorMessage";

/** A durable failed state keeps its recovery controls after the error has been acknowledged. */
export function useErrorAcknowledgement(message: ErrorPresentation | undefined, title?: string | React.ReactElement, error?: unknown, recovery?: ErrorRecovery): void {
  useEffect(() => {
    if (message === undefined) return;
    errorAlerts.report({ message: errorText(message), error: error ?? message, recovery, title, content: <DaemonErrorMessage error={message} /> });
  }, [message, title, error, recovery]);
}
