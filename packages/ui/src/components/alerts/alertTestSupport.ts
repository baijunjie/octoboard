import { vi } from "vitest";

import { errorAlerts } from "./errorAlerts";
import { acknowledgeAllAlerts } from "../../test/alertQueue";

type Report = typeof errorAlerts.report;

const sinks = new Set<(message: string) => void>();
// The method itself, taken before any test can have spied on it, for a test's own spy that only watches it.
const original: Report = errorAlerts.report.bind(errorAlerts);
/** What `report` is while some host forwards, and how to put back what it was. */
let installed: { mock: Report; uninstall: () => void } | null = null;

const forwarding = (next: Report): Report => (alert, options) => {
  for (const sink of sinks) sink(alert.message);
  next(alert, options);
};

/** Makes `errorAlerts.report` forward to the sinks, unless it already does. Whether it does is told by which function
 * `report` is now rather than by a flag: `vi.restoreAllMocks()` takes the forwarder away behind this module's back. A
 * `mockReset()` of the forwarding mock is not noticed, since the same function stays in place. A test that mocked
 * `report` itself keeps its mock, which goes on recording calls and doing what the test made it do once the message is
 * forwarded: spying again would only hand back that mock, and taking it as what to forward to would have the mock call
 * itself. */
function install(): void {
  const current = errorAlerts.report;
  if (installed && current === installed.mock) return;
  if (vi.isMockFunction(current)) {
    const previous = current.getMockImplementation();
    current.mockImplementation(forwarding(previous ?? original));
    installed = { mock: current, uninstall: () => current.mockImplementation(previous ?? original) };
  } else {
    const spy = vi.spyOn(errorAlerts, "report").mockImplementation(forwarding(original));
    installed = { mock: errorAlerts.report, uninstall: () => spy.mockRestore() };
  }
}

/** Hands the message of every error alert to `error`, where the daemon's own notifications go, and still queues it (or
 * does what a test's own mock of `report` does with it). Install it before the tree's passive effects run (before
 * rendering, or from a layout effect of the host), so an alert raised as the tree mounts is seen. Several hosts
 * mounted at once share one forwarder, and each install gets its own sink even for the same `error`, so each receives
 * every message once. Returns the cleanup: it stops forwarding to `error`, and once no host forwards any more puts
 * `errorAlerts.report` back as it found it and acknowledges what is left queued. */
export function forwardAlerts(error: (message: string) => void): () => void {
  install();
  const sink = (message: string) => error(message);
  sinks.add(sink);
  return () => {
    sinks.delete(sink);
    if (sinks.size) return;
    installed?.uninstall();
    installed = null;
    acknowledgeAllAlerts();
  };
}
