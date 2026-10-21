import { Alert } from "@heroui/react";
import React from "react";

import { CodeToneContext, type CodeColor } from "./codeTone";

type AlertStatus = "default" | "accent" | "success" | "warning" | "danger";

const COLORS: Record<AlertStatus, CodeColor> = {
  default: "default",
  accent: "primary",
  success: "success",
  warning: "warning",
  danger: "danger",
};

const FILLS: Record<AlertStatus, string> = {
  default: "alert-fill-default",
  accent: "alert-fill-accent",
  success: "alert-fill-success",
  warning: "alert-fill-warning",
  danger: "alert-fill-danger",
};

/** HeroUI's alert frame, with a shared tone for every code literal inside it. */
export function TintedAlert({
  status = "default",
  className = "",
  children,
  ...props
}: React.HTMLAttributes<HTMLDivElement> & { status?: AlertStatus }): React.ReactElement {
  return (
    <CodeToneContext.Provider value={COLORS[status]}>
      <Alert {...props} status={status} className={`${FILLS[status]} ${className}`}>
        {children}
      </Alert>
    </CodeToneContext.Provider>
  );
}
