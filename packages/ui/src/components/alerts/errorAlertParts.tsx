import type React from "react";

import { DaemonErrorMessage } from "./DaemonErrorMessage";
import { remedyInBody } from "./errorPartPresence";
import { ownTextOf, rawErrorDetails } from "./errorParts";
import { messageKey } from "../../errors/codes";
import { messageCode } from "../../errors/daemonMessage";
import { isStructuredFailure } from "../../errors/errorPresentation";
import { isMessageKey, type MessageKey, type Translate } from "../../i18n/catalog";
import type { NotificationRequest } from "../../store";

type AlertTitleKey = Extract<MessageKey, `${string}.alertTitle`>;

/** The catalog key of the title an error alert takes for what failed (`daemon.<code>.alertTitle`),
 * where the code has one: it says what failed without naming the session, which the alert's body
 * does. The variant the failure is worded under has its own title where the catalog gives one
 * (`daemon.<code>.<variant>.alertTitle`, a draft cleared only in part). A failed launch's
 * `daemonError` carries its cause's code, so its own is read off `daemonTitle`. */
function alertTitleKey({ daemonError, daemonTitle }: NotificationRequest): AlertTitleKey | undefined {
  const code = daemonTitle ? daemonTitle.code.replace(/\.title$/, "") : daemonError?.code;
  const variant = !daemonTitle && daemonError ? `${messageKey(messageCode(daemonError.code, daemonError.params))}.alertTitle` : undefined;
  if (variant && isMessageKey(variant)) return variant as AlertTitleKey;
  const key = `${messageKey(code ?? "")}.alertTitle`;
  return isMessageKey(key) ? (key as AlertTitleKey) : undefined;
}

/** The title, body, "Error details" and attached block an error alert takes from an error request. The title is a plain name for what
 * failed and never carries a name; the code's own `alertTitle` wins over the one the call site names, unless the
 * request keeps its own (`keepTitle`), and a notice to acknowledge that neither titles takes the title all such
 * notices share; which session it is about is for the caller to say ahead of the body. The body
 * is the daemon's structure where it words the message, else the message as worded in this language; the raw text it
 * carries is the details, never the body. Text the user wrote that the failure carries (`detailLabel`) is `attached`:
 * left out of the body and shown after it, outside the alert's description. */
export function errorAlertParts(t: Translate, request: NotificationRequest): { title: string | undefined; body: React.ReactNode; details: React.ReactNode; remedyInBody: boolean; attached: React.ReactNode } {
  const { daemonError } = request;
  const titleKey = alertTitleKey(request);
  const shown = daemonError && isStructuredFailure(daemonError) ? daemonError : request.message;
  // Text the user wrote that the failure carries goes after the body, outside what describes the alert.
  const attached = request.content === undefined ? ownTextOf(shown) : undefined;
  return {
    title: titleKey && !request.keepTitle ? t(titleKey) : request.title ?? (request.kind === "acknowledged" ? t("notice.acknowledged.title") : undefined),
    body: request.content ?? <DaemonErrorMessage error={shown} ownTextApart={attached !== undefined} />,
    details: rawErrorDetails(daemonError ?? request.message),
    remedyInBody: remedyInBody(request.content, shown),
    attached,
  };
}
