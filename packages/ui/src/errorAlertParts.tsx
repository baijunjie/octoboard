import type React from "react";

import { DaemonErrorMessage } from "./components/DaemonErrorMessage";
import { remedyInBody } from "./components/errorPartPresence";
import { rawErrorDetails } from "./components/errorParts";
import { isStructuredFailure } from "./errorPresentation";
import { isMessageKey, type MessageKey } from "./i18n/catalog";
import { t } from "./i18n/language";
import type { ToastRequest } from "./store";

type AlertTitleKey = Extract<MessageKey, `daemon.${string}.alertTitle`>;

/** The catalog key of the title an error alert takes for what failed (`daemon.<code>.alertTitle`),
 * where the code has one: it says what failed without naming the session, which the alert's body
 * does. A failed launch's `daemonError` carries its cause's code, so its own is read off
 * `daemonTitle`. */
function alertTitleKey({ daemonError, daemonTitle }: ToastRequest): AlertTitleKey | undefined {
  const code = daemonTitle ? daemonTitle.code.replace(/\.title$/, "") : daemonError?.code;
  const key = `daemon.${code}.alertTitle`;
  return isMessageKey(key) ? (key as AlertTitleKey) : undefined;
}

/** The title, body and "Error details" an error alert takes from an error request. The title is a plain name for what
 * failed and never carries a name; the code's own `alertTitle` wins over the one the call site names; which session it
 * is about is for the caller to say ahead of the body. The body is the daemon's structure where it words the message,
 * else the message as worded in this language; the raw text it carries is the details, never the body. */
export function errorAlertParts(request: ToastRequest): { title: string | undefined; body: React.ReactNode; details: React.ReactNode; remedyInBody: boolean } {
  const { daemonError } = request;
  const titleKey = alertTitleKey(request);
  const shown = daemonError && isStructuredFailure(daemonError) ? daemonError : request.message;
  return {
    title: titleKey ? t(titleKey) : request.title,
    body: request.content ?? <DaemonErrorMessage error={shown} />,
    details: rawErrorDetails(daemonError ?? request.message),
    remedyInBody: remedyInBody(request.content, shown),
  };
}
