import React from "react";

import { DaemonErrorMessage } from "../components/alerts/DaemonErrorMessage";
import { codedMessageNode } from "../components/alerts/errorParts";
import { InlineReference } from "../i18n/InlineReference";
import { hasCodeParams, type ErrorPresentation } from "../errors/errorPresentation";
import { globalParam, type PlainMessageKey } from "../i18n/catalog";
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

/** Why a read of a change's bodies failed (`message`, the wording of a `ChangeBodiesError`, and `messageKey`, the
 * viewer's own sentence it is), as a node for a sentence that sets names as chips: the coded failure behind it where
 * there is one, the viewer's own sentence in the live language, any other wording as the text it is. */
export function BodiesFailureReason({ message, messageKey, failure }: { message: string; messageKey?: PlainMessageKey; failure?: { code: string; params: MessageParams } }): React.ReactElement {
  if (failure && hasCodeParams(failure)) return <>{codedMessageNode(failure, message, "summary")}</>;
  if (messageKey) return <Message id={messageKey} />;
  return <>{isolate(message)}</>;
}
