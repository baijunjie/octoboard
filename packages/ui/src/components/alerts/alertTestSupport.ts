import { type MockInstance, vi } from "vitest";

import { errorAlerts } from "./errorAlerts";

const sinks = new Set<(message: string) => void>();
let spy: MockInstance<typeof errorAlerts.report> | null = null;

/** Hands the message of every error alert raised without a daemon (`raiseError` from `components/alerts/raiseError`) to
 * `error`, where the daemon's own toasts go, and still queues it. Install it before the tree's passive effects run
 * (before rendering, or from a layout effect of the host), so an alert raised as the tree mounts is seen. Several hosts
 * mounted at once share one spy, and each install gets its own sink even for the same `error`, so each receives every
 * message: spying on `report` again would wrap the existing mock in itself. Returns the cleanup: it stops forwarding to
 * `error`, and once no host forwards any more restores `errorAlerts.report` and acknowledges what is left queued. */
export function forwardAlerts(error: (message: string) => void): () => void {
  if (!spy) {
    const report = errorAlerts.report.bind(errorAlerts);
    spy = vi.spyOn(errorAlerts, "report").mockImplementation((alert, options) => {
      for (const sink of sinks) sink(alert.message);
      report(alert, options);
    });
  }
  const sink = (message: string) => error(message);
  sinks.add(sink);
  return () => {
    sinks.delete(sink);
    if (sinks.size) return;
    spy?.mockRestore();
    spy = null;
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  };
}
