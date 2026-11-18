import type { Translate } from "../../i18n/catalog";
import { errorNotification } from "../../store";
import { errorAlertParts } from "./errorAlertParts";
import { errorAlerts, type AcknowledgementView } from "./errorAlerts";

/** The failure modal's view of `error` titled `title`, for a dialog whose purpose a refusal has ended: the explanation
 * and its raw text behind Error details. Undefined for a failure classified as unexpected, which has report-a-bug
 * guidance and belongs in an error alert. It builds `AcknowledgementView` (`errorAlerts.ts`) from the parts of an alert,
 * which import the alert queue, so it cannot live in that file. */
export function acknowledgementViewOf(t: Translate, error: unknown, title: string): AcknowledgementView | undefined {
  const request = errorNotification(error, { title: undefined });
  const { body, details, remedyInBody, attached } = errorAlertParts(t, request);
  const view = errorAlerts.view({ message: request.message, error: request.error, title, content: body, attached, details, remedyInBody });
  return view.classification.kind === "actionable" ? { ...view, classification: view.classification } : undefined;
}
