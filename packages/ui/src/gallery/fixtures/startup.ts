import type { Scenario } from "../scenario";

const GROUP = "Startup and connection";

export const startupScenarios: Scenario[] = [
  { id: "startup-no-address", group: GROUP, title: "No daemon address", startup: { kind: "noAddress" } },
  {
    id: "startup-daemon-failed",
    group: GROUP,
    title: "The daemon failed to start",
    startup: { kind: "daemonFailed", error: "another instance already holds the lock at /Users/dev/.octoboard/daemon.lock" },
  },
  { id: "startup-crash", group: GROUP, title: "The UI crashed", description: "The error boundary's fallback, with Try again.", startup: { kind: "crash" } },
];
