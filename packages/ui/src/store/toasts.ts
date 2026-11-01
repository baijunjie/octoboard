import type { ReactNode } from "react";

import { DaemonRequestError } from "../daemon-client";
import { daemonMessage, type RecordNames } from "../errors/daemonMessage";
import type { ErrorRecovery } from "../errors/errorRecovery";
import { errorPresentation, noticeErrorPresentation, noticeKind } from "../errors/errorPresentation";
import { errorText } from "../errors/errorText";
import type { Language } from "../i18n/languages";
import type { Event } from "../protocol";

/**
 * A window notification. Errors require acknowledgement in the shared error dialog; notices
 * remain timed toasts. The onToast name is retained for existing producers and consumers.
 */
export interface ToastRequest {
  kind: "error" | "notice";
  message: string;
  /** Optional markup; `message` stays plain for deduplication and text-only consumers. */
  content?: ReactNode;
  daemonTitle?: DaemonRequestError;
  /** The daemon's own code and params, kept for a structured rendering of the message (a literal
   * set as code), for a notice as well as an error. Absent for a message the UI worded itself. */
  daemonError?: DaemonRequestError;
  /** The original local exception, including its cause and platform error code. */
  error?: unknown;
  /** The session the toast is about, so the stack can say where it is. A daemon `notice` always
   * has one; a notice the UI raises itself (`toastNotice`) has none. An `error` has one only where
   * the call site knows which session the daemon's message is about — the daemon says "this
   * session" without naming it, having no notion of what the client calls it, so the alert's body
   * is the only thing that tells the user which one. */
  session?: string;
  /** The plain name of what failed, for an error alert whose code has no `alertTitle` of its own. */
  title?: string;
  /** The call site's `title` beats the code's own `alertTitle`; see `AlertOptions.keepTitle`. */
  keepTitle?: boolean;
  /** The remedy an error alert gives in place of the one its kind of failure has, where the call site knows a better
   * one (see `classifyError`). */
  recovery?: ErrorRecovery;
}

/** What a call site says about the error alert it raises (see `Daemon.raiseError`). */
export interface AlertOptions {
  /** The plain name of what failed, with no name in it. `undefined` only where the alert is titled for its kind, for an
   * error nothing caught (`main.tsx`), or where the request is built only for its body and details and the caller
   * titles the alert itself (`useFailureWhenBlocked`). */
  title: string | undefined;
  /** The session the failure is about; see `ToastRequest.session`. */
  session?: string;
  /** The remedy in place of the one the failure's kind has (see `classifyError`). */
  recovery?: ErrorRecovery;
  /** The body in place of the failure's own words. */
  content?: ReactNode;
  /** The call site's title beats the code's own `alertTitle`: for a dialog's submit or confirmation, whose alert always
   * takes the dialog's title. */
  keepTitle?: boolean;
}

/** The error request every raising route builds for `error` (`Daemon.raiseError` and `raiseError`), with the daemon's
 * code and params kept when it is an error that has them. */
export function errorToast(error: unknown, { title, session, recovery, content, keepTitle }: AlertOptions): ToastRequest {
  const presentation = errorPresentation(error);
  return { kind: "error", message: errorText(presentation), session, title, content, keepTitle, error, daemonError: presentation, recovery };
}

/** The two events the daemon sends that are shown as a toast: a notice about a session, and an `error` that answers no
 * request. */
export type BroadcastToastEvent = Extract<Event, { type: "session_notice" }> | Extract<Event, { type: "error" }>;

/** The toast to show for a broadcast of the daemon, worded in `language` with the names of `records`. A request's own
 * `error` reply carries its request id and reaches its caller as a rejected promise instead, so it is no broadcast and
 * is left to the caller to filter out. Pure, so that the gallery's daemon routes its fixtures through the same code. */
export function broadcastToast(event: BroadcastToastEvent, records: RecordNames, language: Language): ToastRequest {
  const message = daemonMessage(language, event.code, event.params, event.message, records);
  if (event.type === "error") {
    return { kind: "error", daemonError: new DaemonRequestError(event.message, event.code, event.params), message };
  }
  // A failed launch is presented as the failure it reports, laid out beneath a title that does
  // not repeat the reason; any other notice as itself.
  const failure = noticeErrorPresentation(event.code, event.params, event.message);
  return {
    kind: noticeKind(event.code, event.params),
    message,
    daemonTitle: failure ? new DaemonRequestError(event.message, `${event.code}.title`, event.params) : undefined,
    daemonError: failure ?? new DaemonRequestError(event.message, event.code, event.params),
    session: event.session,
  };
}
