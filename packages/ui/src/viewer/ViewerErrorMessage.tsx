import React from "react";

import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { InlineReference } from "../components/InlineReference";
import type { ErrorPresentation } from "../errorPresentation";
import { globalParam } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { viewerErrorKey } from "./viewerError";

export function ViewerErrorMessage({ error, path }: { error: ErrorPresentation; path: string }): React.ReactElement {
  const t = useT();
  const key = viewerErrorKey(error, path);
  if (key === "viewer.error.permissionDenied.summary") {
    // The system's own text is the failure's Details, not part of the sentence.
    return <span><Message id={key} params={{ appName: <InlineReference dir="auto" wrap>{globalParam("appName")}</InlineReference> } as never} /></span>;
  }
  if (key) return <>{t(key)}</>;
  return <DaemonErrorMessage error={error} part="summary" />;
}
