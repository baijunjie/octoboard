import React from "react";

import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { codedMessageNode } from "../components/errorParts";
import { InlineReference } from "../components/InlineReference";
import { hasCodeParams, type ErrorPresentation } from "../errorPresentation";
import { globalParam } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { MessageParams } from "../protocol";
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

/** Why a read of a change's bodies failed (`message`, the wording of a `ChangeBodiesError`), as a node for a sentence that
 * sets names as chips: the coded failure behind it where there is one, the viewer's own permission sentence with the
 * application's name as a chip, any other wording as the text it is. */
export function BodiesFailureReason({ message, failure }: { message: string; failure?: { code: string; params: MessageParams } }): React.ReactElement {
  const t = useT();
  if (failure && hasCodeParams(failure)) return <>{codedMessageNode(failure, message, "summary")}</>;
  if (message === t("viewer.error.permissionDenied.summary")) return <Message id="viewer.error.permissionDenied.summary" />;
  return <>{isolate(message)}</>;
}
