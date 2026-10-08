// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import type { Agent, AgentAvailability, Session } from "../protocol";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { SessionDialog } from "./SessionDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const parentConsole = consoleOf("c-1", "Main");
const project = projectOf("p-1", parentConsole.id, "Search API");

function fakeDaemon(initial: Partial<State> = {}): Daemon {
  return {
    store: createStateStore(initial),
    request: vi.fn(),
    toastError: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** An `agentAvailability` map with every agent `unavailable` — "no agent available at all". Named
 * distinctly from `agents.ts`'s own `noAgentAvailable`, which this is only a fixture for. */
function everyAgentUnavailable(): Map<Agent, AgentAvailability> {
  const agents: Agent[] = ["claude", "codex", "grok"];
  return new Map(agents.map((agent) => [agent, { agent, availability: "unavailable" as const }]));
}

/** Renders the dialog and returns its root, so a test can query the rendered body and unmount it. */
function renderDialog(sessions: Session[], initial: Partial<State> = {}): { container: HTMLElement; root: ReturnType<typeof createRoot> } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={fakeDaemon(initial)}>
        <SessionDialog console={parentConsole} project={project} sessions={sessions} onClose={() => {}} onOpened={() => {}} />
      </DaemonProvider>,
    ),
  );
  return { container, root };
}

/** Renders the dialog and returns its "report to console session" checkbox input. */
function renderCheckbox(sessions: Session[]): HTMLInputElement {
  const { container, root } = renderDialog(sessions);
  const checkbox = document.body.querySelector('input[type="checkbox"]') as HTMLInputElement;
  act(() => root.unmount());
  container.remove();
  return checkbox;
}

it("disables the report-to-console-session checkbox when the console has no live console session", () => {
  const checkbox = renderCheckbox([]);
  expect(checkbox.disabled).toBe(true);
});

it("enables the checkbox once a live console session exists", () => {
  const checkbox = renderCheckbox([sessionOf("s-console", parentConsole.id, undefined, "Hub 1", "idle")]);
  expect(checkbox.disabled).toBe(false);
});

it("shows the install prompt when no agent is available at all", () => {
  // HeroUI's `Modal` renders into a portal at `document.body`, not into the container itself.
  const { container, root } = renderDialog([], { agentAvailability: everyAgentUnavailable() });
  expect(document.body.textContent).toContain("Make sure one of them is on your PATH to open a session.");
  act(() => root.unmount());
  container.remove();
});

it("opens on a selectable agent, with the console's account for it, and sends both", async () => {
  // Claude Code, the console's default agent, is known to be missing; Codex is the first left.
  const daemon = fakeDaemon({
    agentAvailability: new Map<Agent, AgentAvailability>([
      ["claude", { agent: "claude", availability: "unavailable" }],
      ["codex", { agent: "codex", availability: "available" }],
      ["grok", { agent: "grok", availability: "available" }],
    ]),
    settings: {
      auto_sync_repositories: false,
      accounts: [{ id: "a-1", agent: "codex", name: "Work", config_dir: "/home/me/.codex-work" }],
    },
  });
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <SessionDialog
          console={{ ...parentConsole, codex_account_id: "a-1" }}
          project={project}
          sessions={[]}
          onClose={() => {}}
          onOpened={() => {}}
        />
      </DaemonProvider>,
    ),
  );
  expect(document.body.textContent).toContain("Codex (Work)");

  const open = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Open");
  await act(async () => open?.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", agent: "codex", account: "a-1" }));
  act(() => root.unmount());
  container.remove();
});
