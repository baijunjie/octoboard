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
   * there are none, and the alert shows no disclosure; only when absent does it work them out. */
  details?: ReactNode;
}

const listeners = new Set<() => void>();
let nextId = 0;
let queue: readonly (ErrorAlertRequest & { id: number; classification: ErrorClassification; sharedRecovery?: ErrorRecovery; signature: string })[] = [];
const notify = () => listeners.forEach((listener) => listener());
let focusFallback: (() => void) | undefined;
let fallbackTimer: ReturnType<typeof setTimeout> | undefined;
const cancelFallback = () => {
  clearTimeout(fallbackTimer);
  fallbackTimer = undefined;
};
const FOCUS_FALLBACK_DELAY_MS = 400;

/** Errors survive screen changes and stay queued until the user acknowledges them. */
export const errorAlerts = {
  subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  },
  getSnapshot: () => queue,
  report(request: ErrorAlertRequest, options: { immediate?: boolean } = {}): void {
    const failures = request.failures?.length ? request.failures : [{ error: request.error ?? request.message, recovery: request.recovery }];
    const classifications = failures.map((failure) => classifyError(failure.error, failure.recovery));
    const classification = classifications.find((failure) => failure.kind === "unexpected") ?? classifications[0];
    const metadata = failures.map(({ error: source }, index) => [
      classifications[index],
      source instanceof DaemonRequestError && source.code !== "unexpected_error" ? [source.code, source.message, Object.entries(source.params).sort(([a], [b]) => a.localeCompare(b))] : errorText(errorPresentation(source)),
    ]);
    const signature = JSON.stringify([request.message, request.session, metadata]);
    if (queue.some((item) => item.signature === signature)) return;
    // The one remedy of every failure the alert is about, if there is one (it can carry a button for it).
    const first = classifications[0];
    const sharedRecovery = first.kind === "actionable" && classifications.every((failure) => failure.kind === "actionable" && failure.recovery === first.recovery) ? first.recovery : undefined;
    const item = { ...request, id: ++nextId, classification, sharedRecovery, signature };
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
