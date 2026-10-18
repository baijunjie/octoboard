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
}

const listeners = new Set<() => void>();
let nextId = 0;
let queue: readonly (ErrorAlertRequest & { id: number; classification: ErrorClassification; signature: string })[] = [];
const notify = () => listeners.forEach((listener) => listener());

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
    const item = { ...request, id: ++nextId, classification, signature };
    queue = options.immediate ? [item, ...queue] : [...queue, item];
    notify();
  },
  acknowledge(): void {
    queue = queue.slice(1);
    notify();
  },
};
