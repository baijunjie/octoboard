import React from "react";

import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { InlineChip } from "../components/InlineChip";
import type { ErrorPresentation } from "../errorPresentation";
import { globalParam } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { viewerErrorKey } from "./viewerError";

export function ViewerErrorMessage({ error, path }: { error: ErrorPresentation; path: string }): React.ReactElement {
  const t = useT();
  const key = viewerErrorKey(error, path);
  if (key === "viewer.error.permissionDenied" && typeof error !== "string") {
    return <span><Message id={key} params={{ appName: <InlineChip dir="auto" wrap size="inherit">{globalParam("appName")}</InlineChip>, detail: isolate(error.params.detail) } as never} /></span>;
  }
  if (key && key !== "viewer.error.permissionDenied") return <>{t(key)}</>;
  return <DaemonErrorMessage error={error} part="summary" />;
}
