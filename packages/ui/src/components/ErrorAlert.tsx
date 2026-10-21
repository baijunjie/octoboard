import { Alert } from "@heroui/react";
import React from "react";

import { TintedAlert } from "./TintedAlert";

/** Durable failure content; the dedicated error dialog owns runtime failure announcements. */
export function ErrorAlert({
  title,
  announce = false,
  children,
  className = "",
}: {
  title?: React.ReactNode;
  announce?: boolean;
  children?: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return (
    <div className={`min-w-0 shrink-0 text-start ${className}`}>
      <TintedAlert status="danger">
        <Alert.Indicator aria-hidden="true" />
        <Alert.Content role={announce ? "alert" : undefined}>
          {title && <Alert.Title>{title}</Alert.Title>}
          {children && <Alert.Description>{children}</Alert.Description>}
        </Alert.Content>
      </TintedAlert>
    </div>
  );
}
