import type { ReactNode } from "react";

import { DaemonErrorMessage } from "./components/DaemonErrorMessage";
import type { ErrorRecovery } from "./errorRecovery";
import { errorAlerts } from "./errorAlerts";
import { errorPresentation } from "./errorPresentation";

/** Preserves daemon metadata while sending a failure to the acknowledgement queue. */
export function reportError(error: unknown, session?: string, recovery?: ErrorRecovery, content?: ReactNode): void {
  const presentation = typeof error === "string" ? error : errorPresentation(error);
  errorAlerts.report({
    message: typeof presentation === "string" ? presentation : presentation.message,
    session,
    error,
    recovery,
    content: content ?? <DaemonErrorMessage error={presentation} />,
  });
}
