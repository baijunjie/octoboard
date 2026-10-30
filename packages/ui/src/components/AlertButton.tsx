import React, { createContext, useContext } from "react";

import { Button, type ButtonProps } from "./Button";
import type { AlertStatus } from "./ErrorAlert";

/** The status a button takes its colour from: that of the compact Alert it stands in, or, outside an Alert, of
 * the line it belongs to (a warning line's Details); danger where nothing sets it. */
export const AlertStatusContext = createContext<AlertStatus>("danger");

/** A button at the end of a compact Alert, or on a status line outside one, in that status's colour:
 * the recovery action solid (`kind="action"`), Details soft (`kind="details"`). A danger Alert uses
 * HeroUI's `danger` and `danger-soft` variants; HeroUI has no warning variant, so a warning Alert's
 * buttons take the same two treatments from `.alert-button--warning` and `.alert-button--warning-soft`
 * in `style.css`. This is the one place the pairing is defined. */
export function AlertButton({ kind, className = "", ...props }: Omit<ButtonProps, "variant"> & { kind: "action" | "details" }): React.ReactElement {
  const status = useContext(AlertStatusContext);
  if (status === "danger") return <Button {...props} className={className} variant={kind === "action" ? "danger" : "danger-soft"} />;
  return <Button {...props} className={`${kind === "action" ? "alert-button--warning" : "alert-button--warning-soft"} ${className}`} variant="secondary" />;
}
