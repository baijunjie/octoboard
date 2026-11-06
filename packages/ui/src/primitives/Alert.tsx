import { Alert as HeroAlert } from "@heroui/react";
import React from "react";

export type AlertStatus = "warning" | "danger";

/** HeroUI's alert: a framed block of a warning or failure among other content, assembled from its `AlertIndicator`,
 * its `AlertContent` holding an `AlertTitle` and `AlertDescription`s, and whatever stands after them. */
export function Alert({ status, className, children }: {
  status: AlertStatus;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroAlert status={status} className={className}>{children}</HeroAlert>;
}

/** An alert's status glyph, decorative: the title says what happened. */
export function AlertIndicator(): React.ReactElement {
  return <HeroAlert.Indicator aria-hidden="true" />;
}

/** The text of an alert, its title and descriptions. */
export function AlertContent({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <HeroAlert.Content className={className}>{children}</HeroAlert.Content>;
}

export function AlertTitle({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroAlert.Title>{children}</HeroAlert.Title>;
}

/** A line under an alert's title. It is a `span`, so it takes inline content only. */
export function AlertDescription({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroAlert.Description>{children}</HeroAlert.Description>;
}
