import type { ReactElement, ReactNode } from "react";

import { DaemonRequestError } from "./daemon-client";
import { errorPresentation } from "./errorPresentation";
import { errorText } from "./errorText";
import { classifyError, type ErrorClassification, type ErrorRecovery } from "./errorRecovery";

export interface ErrorAlertFailure {
  error: unknown;
  recovery?: ErrorRecovery;
}

export interface ErrorAlertRequest {
  message: string;
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
let focusFallback: (() => void) | undefined;
let fallbackTimer: ReturnType<typeof setTimeout> | undefined;
const cancelFallback = () => {
  clearTimeout(fallbackTimer);
  fallbackTimer = undefined;
};
const FOCUS_FALLBACK_DELAY_MS = 400;

function viewOf(request: ErrorAlertRequest): { view: ErrorAlertView; signature: string } {
  const failures = request.failures?.length ? request.failures : [{ error: request.error ?? request.message, recovery: request.recovery }];
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
    cancelFallback();
    queue = options.immediate ? [item, ...queue] : [...queue, item];
    notify();
  },
  acknowledge(): void {
    queue = queue.slice(1);
    notify();
    // An alert that came when a dialog closed itself has no control to give focus back to, so it is left on
    // `<body>`: it goes to the window's fallback instead.
    cancelFallback();
    if (queue.length === 0) {
      const fallback = focusFallback;
      // After the alert's exit transition, whose focus restore would otherwise undo it.
      fallbackTimer = setTimeout(() => {
        fallbackTimer = undefined;
        if (!document.activeElement || document.activeElement === document.body) fallback?.();
      }, FOCUS_FALLBACK_DELAY_MS);
    }
  },
  /** Where focus goes when acknowledging the last alert leaves it on `<body>`; returns what unregisters it. */
  setFocusFallback(fallback: () => void): () => void {
    focusFallback = fallback;
    return () => {
      if (focusFallback !== fallback) return;
      focusFallback = undefined;
      cancelFallback();
    };
  },
};
