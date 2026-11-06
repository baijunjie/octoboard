import { Spinner } from "@heroui/react";
import React from "react";

/**
 * HeroUI's spinner as decoration beside a label that already says what is running. HeroUI renders it as a
 * `role="status"` named "Loading", which would be announced and found among the page's status regions, so it is
 * taken out of both.
 */
export function DecorativeSpinner({ size = "sm", className }: { size?: "sm" | "md"; className?: string }): React.ReactElement {
  return <Spinner color="current" size={size} role="presentation" aria-label={undefined} aria-hidden="true" className={className} />;
}
