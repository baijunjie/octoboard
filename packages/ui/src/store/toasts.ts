import type { ReactNode } from "react";

import type { DaemonRequestError } from "../daemon-client";
import type { ErrorRecovery } from "../errorRecovery";
import { errorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";

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
  /** The remedy an error alert gives in place of the one its kind of failure has, where the call site knows a better
   * one (see `classifyError`). */
  recovery?: ErrorRecovery;
}

/** The error toast `toastError` raises for `message`, with the daemon's code and params kept when it is an error that has them. */
export function errorToast(message: string | Error, session?: string, title?: string, recovery?: ErrorRecovery): ToastRequest {
  const presentation = typeof message === "string" ? message : errorPresentation(message);
  return { kind: "error", message: errorText(message), session, title, recovery, error: message,
    ...(typeof presentation !== "string" && { daemonError: presentation }) };
}
