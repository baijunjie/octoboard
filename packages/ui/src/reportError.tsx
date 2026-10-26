import type { ReactNode } from "react";

import { DaemonErrorMessage } from "./components/DaemonErrorMessage";
import { rawErrorDetails } from "./components/errorParts";
import type { ErrorRecovery } from "./errorRecovery";
import { errorAlerts } from "./errorAlerts";
import { errorPresentation } from "./errorPresentation";

/** Preserves daemon metadata while sending a failure to the acknowledgement queue. The alert's body is the
 * failure in the application's own words (or `content`), and the raw text it carries is its "Error details". The
 * `title` says in plain words what failed; without one the alert is titled for the kind of failure. */
export function reportError(error: unknown, session?: string, recovery?: ErrorRecovery, content?: ReactNode, title?: string): void {
  const presentation = typeof error === "string" ? error : errorPresentation(error);
  errorAlerts.report({
    message: typeof presentation === "string" ? presentation : presentation.message,
    session,
    error,
    recovery,
    title,
    content: content ?? <DaemonErrorMessage error={presentation} />,
    details: rawErrorDetails(presentation),
  });
}
