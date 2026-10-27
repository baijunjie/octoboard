import { useState } from "react";

import { errorAlertParts } from "../errorAlertParts";
import { ActionableError } from "../actionableError";
import { errorAlerts, type AcknowledgementView } from "../errorAlerts";
import { errorToast } from "../store";

/** A dialog that cannot be used as it stands (what it would choose from has gone while it was open) turns into a
 * failure modal titled `title` that says why, with only acknowledgement (see `Dialog`'s `failure`): an
 * `ActionableError` always classifies as actionable, so the modal never carries report-a-bug guidance. The result is
 * what to pass there: it is the explanation as of the moment the dialog was blocked, and stays until the dialog is
 * closed, or, for a dialog reused across subjects, until `resetKey` changes. Structured errors retain the metadata
 * needed to render names in the explanation. */
export function useFailureWhenBlocked(blocked: boolean, message: string | ActionableError, title: string, resetKey?: string): AcknowledgementView | undefined {
  const [held, setHeld] = useState<{ key: string | undefined; failure: AcknowledgementView }>();
  let current = held?.key === resetKey ? held : undefined;
  if (blocked && !current) {
    const request = errorToast(typeof message === "string" ? new ActionableError(message, "none") : message);
    const { body, details, remedyInBody } = errorAlertParts(request);
    const view = errorAlerts.view({ message: request.message, error: request.error, title, content: body, details, remedyInBody });
    if (view.classification.kind !== "actionable") throw new Error("An ActionableError classified as unexpected");
    current = { key: resetKey, failure: { ...view, classification: view.classification } };
    setHeld(current);
  }
  return current?.failure;
}
