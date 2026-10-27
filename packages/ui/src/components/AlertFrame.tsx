import { Alert } from "@heroui/react";
import React from "react";

import { CodeToneContext } from "./codeTone";

export type AlertStatus = "warning" | "danger";

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
  ...props
}: React.HTMLAttributes<HTMLDivElement> & { status: AlertStatus }): React.ReactElement {
  return (
    <CodeToneContext.Provider value={status}>
      <Alert {...props} status={status} className={`${FILLS[status]} ${className}`}>
        {children}
      </Alert>
    </CodeToneContext.Provider>
  );
}
