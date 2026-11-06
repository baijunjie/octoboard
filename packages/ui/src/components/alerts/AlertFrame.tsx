import React from "react";

import { CodeToneContext } from "../../i18n/codeTone";
import { Alert, type AlertStatus } from "../../primitives";

export type { AlertStatus };

const FILLS: Record<AlertStatus, string> = {
  warning: "alert-fill-warning",
  danger: "alert-fill-danger",
};

/** HeroUI's alert frame, with a shared tone for every code literal inside it. For `ErrorAlert` only, which composes
 * the parts inside it (`FailureAlert` is built on `ErrorAlert`). */
export function AlertFrame({
  status,
  className = "",
  children,
}: { status: AlertStatus; className?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <CodeToneContext.Provider value={status}>
      <Alert status={status} className={`${FILLS[status]} ${className}`}>
        {children}
      </Alert>
    </CodeToneContext.Provider>
  );
}
