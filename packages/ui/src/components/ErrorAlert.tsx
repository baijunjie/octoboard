import { Alert } from "@heroui/react";
import React from "react";

import { TintedAlert } from "./TintedAlert";

/** A standalone failure; initial page content stays silent while later failures announce once. */
export function ErrorAlert({
  title,
  headingLevel = 3,
  announce = true,
  children,
  className = "",
}: {
  title?: React.ReactNode;
  headingLevel?: 1 | 2 | 3;
  announce?: boolean;
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  const Heading = `h${headingLevel}` as const;
  return (
    <TintedAlert status="danger" className={`min-w-0 shrink-0 text-start shadow-none ${className}`}>
      <Alert.Indicator aria-hidden="true" />
      <Alert.Content className="min-w-0">
        <div role={announce ? "alert" : undefined} className="flex w-full min-w-0 flex-col gap-3 text-sm text-foreground [overflow-wrap:anywhere]">
          {title && <Heading className={`${headingLevel === 1 ? "text-lg" : "text-base"} font-semibold text-foreground`}>{title}</Heading>}
          {title ? " " : null}
          {children}
        </div>
      </Alert.Content>
    </TintedAlert>
  );
}
