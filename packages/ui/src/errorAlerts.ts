import type { ReactElement, ReactNode } from "react";

import { cancelFocusAfterModalCloses, focusAfterModalCloses } from "./components/regionFocus";
import { DaemonRequestError } from "./daemon-client";
import { errorPresentation } from "./errorPresentation";
import { errorText } from "./errorText";
import { classifyError, type ErrorClassification, type ErrorRecovery } from "./errorRecovery";

export interface ErrorAlertFailure {
  error: unknown;
  recovery?: ErrorRecovery;
}

/** A failure for an issue report. `unexpected` is the caller's own judgement that it is one, which stands where the
 * error alone would classify otherwise (a crash's stack that happens to mention an operating-system error); absent,
 * the failure is classified from its error. */
export interface ReportableFailure extends ErrorAlertFailure {
  unexpected?: boolean;
}

export interface ErrorAlertRequest {
  message: string;
  /** The body in place of `message`. Its box adds no spacing, so a body of several paragraphs lays them out itself
   * (`flex flex-col gap-4`, the gap the alert leaves before its remedy line). */
  content?: ReactNode;
  title?: string | ReactElement;
  session?: string;
  error?: unknown;
  recovery?: ErrorRecovery;
  failures?: readonly ErrorAlertFailure[];
  /** What the alert's "Error details" disclosure holds, in place of the diagnostics it works out from the
   * errors: for an alert whose failures each have details of their own, set apart by whose they are. `null` says
   * there are none, and the alert shows no disclosure; only when absent does it work them out. A single failure's
   * raw part (see `rawErrorDetails`) is passed here by `reportError`, and by `Toasts` and a failure modal from
   * `errorAlertParts`, and the startup screens pass their own block; they word the failure themselves, so the alert does not work
   * out a second copy of its raw text. */
  details?: ReactNode;
  /** The body already says what to do about the failure (a daemon message with a remedy of its own), so the alert adds
   * no remedy line of the failure's kind after it: that line is the general advice for the kind, and would repeat or
   * contradict it. Set where the body is the failure's own message: by `reportError`, and from `errorAlertParts`. */
  remedyInBody?: boolean;
  /** The plain text of what `details` holds, for an alert that has no `error` to read it from: the issue report an
   * unexpected failure offers is built from the errors and from this, never from the rendered `details`. */
  diagnostic?: string;
}

/** A request with what is worked out from its errors: the classification that sets the alert's guidance, the one
 * remedy of all its failures if there is one. */
export type ErrorAlertView = ErrorAlertRequest & { classification: ErrorClassification; sharedRecovery?: ErrorRecovery };
/** A view that only acknowledges: its classification is `actionable`, so it never carries the report-a-bug guidance of
 * an unexpected failure. What a dialog that turns into a failure modal shows (see `Dialog`'s `failure`). */
export type AcknowledgementView = ErrorAlertView & { classification: Extract<ErrorClassification, { kind: "actionable" }> };
type QueuedAlert = ErrorAlertView & { id: number; signature: string };

const listeners = new Set<() => void>();
let nextId = 0;
let queue: readonly QueuedAlert[] = [];
const notify = () => listeners.forEach((listener) => listener());

/** The failures an alert is about: those it lists, else its one `error`. Where it has none, the alert's message stands
 * for it in `"view"` (it is what the alert shows); in `"report"` its `diagnostic`, if it has one, does instead, the
 * message being worded for the user and no part of an issue report. For a report the alert is one classified
 * unexpected, and the one failure standing for it is then unexpected whatever its error alone would classify as; each
 * listed failure keeps its own classification, which the alert's is made of. */
export function alertFailures(request: ErrorAlertRequest, purpose: "view" | "report"): readonly ReportableFailure[] {
  if (request.failures?.length) return request.failures;
  const error = request.error ?? (purpose === "view" ? request.message : request.diagnostic);
  return error === undefined ? [] : [{ error, recovery: request.recovery, unexpected: purpose === "report" || undefined }];
}

function viewOf(request: ErrorAlertRequest): { view: ErrorAlertView; signature: string } {
  const failures = alertFailures(request, "view");
  const classifications = failures.map((failure) => classifyError(failure.error, failure.recovery));
  const classification = classifications.find((failure) => failure.kind === "unexpected") ?? classifications[0];
  const metadata = failures.map(({ error: source }, index) => [
    classifications[index],
    source instanceof DaemonRequestError && source.code !== "unexpected_error" ? [source.code, source.message, Object.entries(source.params).sort(([a], [b]) => a.localeCompare(b))] : errorText(errorPresentation(source)),
  ]);
  const signature = JSON.stringify([request.message, request.session, metadata]);
  // The one remedy of every failure the alert is about, if there is one (it can carry a button for it).
  const first = classifications[0];
  const sharedRecovery = first.kind === "actionable" && classifications.every((failure) => failure.kind === "actionable" && failure.recovery === first.recovery) ? first.recovery : undefined;
  return { view: { ...request, classification, sharedRecovery }, signature };
}

/** Errors survive screen changes and stay queued until the user acknowledges them. */
export const errorAlerts = {
  subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  },
  getSnapshot: () => queue,
  /** What an alert for `request` shows, for a dialog that turns into one itself instead of queueing it. */
  view(request: ErrorAlertRequest): ErrorAlertView {
    return viewOf(request).view;
  },
  report(request: ErrorAlertRequest, options: { immediate?: boolean } = {}): void {
    const { view, signature } = viewOf(request);
    if (queue.some((item) => item.signature === signature)) return;
    const item = { ...view, id: ++nextId, signature };
    cancelFocusAfterModalCloses();
    queue = options.immediate ? [item, ...queue] : [...queue, item];
    notify();
  },
  acknowledge(): void {
    queue = queue.slice(1);
    notify();
    // An alert that came when a dialog closed itself has no control to give focus back to, so it is left on
    // `<body>`: it goes to the window's fallback instead.
    if (queue.length === 0) focusAfterModalCloses();
    else cancelFocusAfterModalCloses();
  },
};
