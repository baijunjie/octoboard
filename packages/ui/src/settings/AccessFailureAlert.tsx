import React from "react";

import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { Message } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import type { RemoteAccessFailure } from "../protocol";
import { failureReasonKey } from "./remoteAccessFailure";
import { SettingsSearchBlock } from "./settingsSearchFilter";
import type { SettingsSearchId } from "./settingsSearch";

/** The alert between the Access section's rows that says why a way in is on but not served: `title`, the client's
 * sentence for the failure's reason on that `way` (none for a reason this build does not word there), and the
 * daemon's own words behind Details. `announce` is for a failure that appears after the section has painted. */
export function AccessFailureAlert({ id, title, failure, way, announce }: {
  id: SettingsSearchId;
  title: string;
  failure: RemoteAccessFailure;
  way: "remote" | "browser";
  announce: boolean;
}): React.ReactElement {
  const reasonKey: PlainMessageKey | undefined = failureReasonKey(way, failure.reason);
  return (
    <SettingsSearchBlock id={id}>
      <div className="groove-b py-4">
        <ErrorAlert announce={announce} title={title} details={<DiagnosticBlock>{failure.detail}</DiagnosticBlock>}>
          {reasonKey && <Message id={reasonKey} />}
        </ErrorAlert>
      </div>
    </SettingsSearchBlock>
  );
}
