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

/** Renders the dialog and returns its root and daemon, so a test can query the rendered body, read
 * the requests sent and unmount it. */
function renderDialog(
  sessions: Session[],
  initial: Partial<State> = {},
  consoleOverride: Partial<typeof parentConsole> = {},
): { container: HTMLElement; root: ReturnType<typeof createRoot>; daemon: Daemon } {
  const daemon = fakeDaemon(initial);
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <SessionDialog
          console={{ ...parentConsole, ...consoleOverride }}
          project={project}
          sessions={sessions}
          onClose={() => {}}
          onOpened={() => {}}
        />
      </DaemonProvider>,
    ),
  );
  return { container, root, daemon };
}

const hub1 = sessionOf("s-hub-1", parentConsole.id, undefined, "Hub 1", "idle", { colour: "teal", started_at: 100 });
const hub2 = sessionOf("s-hub-2", parentConsole.id, undefined, "Hub 2", "working", { colour: "rose", started_at: 200 });
const hubArchived = sessionOf("s-hub-3", parentConsole.id, undefined, "Hub 3", "archived", { colour: "azure" });

const buttons = () => Array.from(document.body.querySelectorAll("button"));
const optionElements = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));

/** Renders the dialog over `sessions`, picks the owner option named `choose` if given, submits, and
 * returns the `open_session` request sent plus the owner select's option labels (empty when the
 * dialog offers no choice). */
async function openWith(sessions: Session[], choose?: string): Promise<{ request: Record<string, unknown>; options: string[] }> {
  const { container, root, daemon } = renderDialog(sessions);
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  // The owner select is the one trigger showing its default, "none".
  const ownerTrigger = buttons().find((b) => b.textContent?.includes("No console session"));
  let options: string[] = [];
  if (ownerTrigger) {
    await act(async () => ownerTrigger.click());
    options = optionElements().map((o) => o.textContent ?? "");
    if (choose) await act(async () => optionElements().find((o) => o.textContent === choose)?.click());
  }
  await act(async () => buttons().find((b) => b.textContent === "Open")?.click());
  const request = vi.mocked(daemon.request).mock.calls[0][0] as unknown as Record<string, unknown>;
  act(() => root.unmount());
  container.remove();
  return { request, options };
}

it.each([
  { name: "defaults to none and offers every live console session", sessions: [hub1, hub2, hubArchived], choose: undefined, options: ["No console session", "Hub 2", "Hub 1"], boundTo: undefined },
  { name: "binds to the console session chosen", sessions: [hub1, hub2, hubArchived], choose: "Hub 1", options: ["No console session", "Hub 2", "Hub 1"], boundTo: "s-hub-1" },
  { name: "offers no choice with no live console session", sessions: [hubArchived], choose: undefined, options: [], boundTo: undefined },
])("$name", async ({ sessions, choose, options, boundTo }) => {
  const result = await openWith(sessions, choose);
  expect(result.options).toEqual(options);
  expect(result.request.bound_to).toBe(boundTo);
});

it("keeps the owner select, back on none, when the chosen console session is archived under it", async () => {
  const { container, root, daemon } = renderDialog([hub1]);
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  await act(async () => buttons().find((b) => b.textContent?.includes("No console session"))?.click());
  await act(async () => optionElements().find((o) => o.textContent === "Hub 1")?.click());
  expect(buttons().some((b) => b.textContent === "Hub 1")).toBe(true);

  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <SessionDialog
          console={parentConsole}
          project={project}
          sessions={[{ ...hub1, status: "archived" }]}
          onClose={() => {}}
          onOpened={() => {}}
        />
      </DaemonProvider>,
    ),
  );
  const ownerTrigger = buttons().find((b) => b.textContent?.includes("No console session"));
  expect(ownerTrigger).toBeDefined();
  await act(async () => ownerTrigger?.click());
  expect(optionElements().map((o) => o.textContent)).toEqual(["No console session"]);

  await act(async () => buttons().find((b) => b.textContent === "Open")?.click());
  expect(vi.mocked(daemon.request).mock.calls[0][0]).toMatchObject({ type: "open_session", bound_to: undefined });
  act(() => root.unmount());
  container.remove();
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
  const { container, root, daemon } = renderDialog(
    [],
    {
      agentAvailability: new Map<Agent, AgentAvailability>([
        ["claude", { agent: "claude", availability: "unavailable" }],
        ["codex", { agent: "codex", availability: "available" }],
        ["grok", { agent: "grok", availability: "available" }],
      ]),
      settings: {
        auto_sync_repositories: false,
        accounts: [{ id: "a-1", agent: "codex", name: "Work", config_dir: "/home/me/.codex-work" }],
      },
    },
    { codex_account_id: "a-1" },
  );
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  expect(document.body.textContent).toContain("Codex (Work)");

  const open = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Open");
  await act(async () => open?.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", agent: "codex", account: "a-1" }));
  act(() => root.unmount());
  container.remove();
});
