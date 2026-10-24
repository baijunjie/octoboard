import { Alert } from "@heroui/react";
import React from "react";

import { CodeToneContext } from "./codeTone";

export type AlertStatus = "default" | "accent" | "success" | "warning" | "danger";

const FILLS: Record<AlertStatus, string> = {
  default: "alert-fill-default",
  accent: "alert-fill-accent",
  success: "alert-fill-success",
  warning: "alert-fill-warning",
  danger: "alert-fill-danger",
};

/** HeroUI's alert frame, with a shared tone for every code literal inside it. For the Alert wrappers
 * only, which compose the parts inside it: `TintedAlert` for a static one, `ErrorAlert` for a failure. */
export function AlertFrame({
  status = "default",
  className = "",
  children,
  ...props
}: React.HTMLAttributes<HTMLDivElement> & { status?: AlertStatus }): React.ReactElement {
  return (
    <CodeToneContext.Provider value={status}>
      <Alert {...props} status={status} className={`${FILLS[status]} ${className}`}>
        {children}
      </Alert>
    </CodeToneContext.Provider>
  );
}
