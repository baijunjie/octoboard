import { Alert } from "@heroui/react";
import React from "react";

import { AlertFrame, type AlertStatus } from "./AlertFrame";

/** An Alert with its status tint: the indicator, the `title` (always there, so the indicator sits on
 * its line), the optional `description` under it and, as `children`, the Alert's own details (a list of
 * the items it is about, say) after both. The details are drawn in the description's own text style,
 * in a block of their own since `Alert.Description` is a single text run. */
export function TintedAlert({
  title,
  description,
  children,
  ...props
}: Omit<React.HTMLAttributes<HTMLDivElement>, "title"> & {
  status?: AlertStatus;
  title: React.ReactNode;
  description?: React.ReactNode;
}): React.ReactElement {
  return (
    <AlertFrame {...props}>
      <Alert.Indicator aria-hidden="true" />
      <Alert.Content>
        <Alert.Title>{title}</Alert.Title>
        {description && <Alert.Description>{description}</Alert.Description>}
        {children && <div data-slot="alert-details" className="alert__description min-w-0 self-stretch">{children}</div>}
      </Alert.Content>
    </AlertFrame>
  );
}
