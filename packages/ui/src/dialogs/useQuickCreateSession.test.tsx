// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AGENT_OPTIONS } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import type { NewSessionRequest } from "./dialogRequest";
import { useQuickCreateSession } from "./useQuickCreateSession";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const parent = consoleOf("c-1", "Main");
const project = projectOf("p-1", parent.id, "Search API");
const hub = sessionOf("s-hub", parent.id, undefined, "Hub", "idle");

function mount(initial: Partial<State> = {}) {
  const daemon: Daemon = {
    store: createStateStore(initial),
    request: vi.fn(async () => ({ type: "session_opened", session: sessionOf("s-new", parent.id, project.id, "", "idle") })) as Daemon["request"],
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const onOpened = vi.fn();
  let quickCreate!: (target: NewSessionRequest) => void;
  function Host({ consoleId }: { consoleId: string }) {
    quickCreate = useQuickCreateSession(onOpened, consoleId);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const showConsole = (consoleId: string) => act(() => root.render(<DaemonProvider value={daemon}><Host consoleId={consoleId} /></DaemonProvider>));
  showConsole(parent.id);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const run = (target: NewSessionRequest) => act(async () => quickCreate(target));
  return { daemon, onOpened, run, showConsole };
}

const stored = (...sessions: Session[]) => ({ sessions: new Map(sessions.map((s) => [s.id, s])) });

it("opens the session on the dialog's defaults without asking anything, and selects it", async () => {
  const { daemon, onOpened, run } = mount();
  await run({ kind: "new-session", console: parent, project, binding: { kind: "choose" } });
  expect(daemon.request).toHaveBeenCalledWith({
    type: "open_session",
    console_id: "c-1",
    project_id: "p-1",
    bound_to: undefined,
    agent: "claude",
    account: null,
    title: "Search API session",
  });
  expect(onOpened).toHaveBeenCalledWith("s-new");
});

it("binds to the console session whose focus mode it was pressed in", async () => {
  const { daemon, run } = mount(stored(hub));
  await run({ kind: "new-session", console: parent, project, binding: { kind: "bound", to: hub } });
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ bound_to: "s-hub" }));
});

it("opens a console session with no project", async () => {
  const { daemon, run } = mount();
  await run({ kind: "new-console-session", console: parent });
  expect(daemon.request).toHaveBeenCalledWith(expect.not.objectContaining({ project_id: expect.anything() }));
});

it("reports a refusal as a toast and selects nothing", async () => {
  const { daemon, onOpened, run } = mount();
  const refusal = new DaemonRequestError("No such agent", "agent_unavailable", {});
  vi.mocked(daemon.request).mockRejectedValue(refusal);
  await run({ kind: "new-console-session", console: parent });
  expect(daemon.toastError).toHaveBeenCalledTimes(1);
  expect(onOpened).not.toHaveBeenCalled();
});

it("refuses an owner that is no longer live, as the dialog does, and sends nothing", async () => {
  const { daemon, run } = mount(stored({ ...hub, status: "archived" }));
  await run({ kind: "new-session", console: parent, project, binding: { kind: "bound", to: hub } });
  expect(daemon.request).not.toHaveBeenCalled();
  expect(daemon.toastError).toHaveBeenCalledTimes(1);
});

it("sends nothing with no agent available", async () => {
  const none = new Map(AGENT_OPTIONS.map(({ value }) => [value, { agent: value, availability: "unavailable" as const }]));
  const { daemon, run } = mount({ agentAvailability: none });
  await run({ kind: "new-console-session", console: parent });
  expect(daemon.request).not.toHaveBeenCalled();
});

it("does not select the session when the window moved to another console while it was opening", async () => {
  const { daemon, onOpened, run, showConsole } = mount();
  let reply!: (value: unknown) => void;
  vi.mocked(daemon.request).mockReturnValue(new Promise((resolve) => (reply = resolve)) as never);
  await run({ kind: "new-console-session", console: parent });
  showConsole("c-2");
  await act(async () => reply({ type: "session_opened", session: sessionOf("s-new", parent.id, undefined, "", "idle") }));
  expect(onOpened).not.toHaveBeenCalled();
});
