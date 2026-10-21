// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { AccountQuota, Agent, AgentAvailability, Session, SessionBranchItem, Settings } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { HandoffDialog } from "./HandoffDialog";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const parentConsole = consoleOf("c-1", "Main");
const project = projectOf("p-1", parentConsole.id, "Search API");
const hub = sessionOf("s-hub", parentConsole.id, undefined, "Hub", "idle");
const branch = (name: string): SessionBranchItem => ({
  kind: "branch", repository: "/r", branch: name, path: null, uncommitted: false, project: project.id, session: "s-1", state: "removable", detail: null,
});

function mount(session: Session, others: Session[] = [], branches: SessionBranchItem[] | "fail" = [], availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {}, added: Agent[] | null = null, accountQuotas: AccountQuota[] = [], settings: Partial<Settings> = {}) {
  const sessions = new Map([session, ...others].map((s) => [s.id, s]));
  const initial: Partial<State> = {
    consoles: new Map([[parentConsole.id, parentConsole]]),
    projects: new Map([[project.id, project]]),
    sessions,
  };
  const store = createStateStore(initial);
  store.setState({
    settings: { ...store.getState().settings, added_agents: added, ...settings },
    accountQuotas,
    agentAvailability: new Map((Object.entries(availability) as [Agent, AgentAvailability["availability"]][]).map(([agent, state]) => [agent, { agent, availability: state } as AgentAvailability])),
  });
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "inspect_session_branches" && branches === "fail") throw new Error("git failed");
    if (body.type === "inspect_session_branches") return { type: "session_branches_inspected", items: branches };
    if (body.type === "hand_off_session") return { type: "session_opened", session: sessionOf("s-new", parentConsole.id, session.project_id ?? undefined, session.title, "working", { agent: "codex" }) };
    return { type: "ack" };
  });
  const daemon = { store, request, toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "" } as unknown as Daemon;
  const onClose = vi.fn();
  const onHandedOff = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={daemon}><HandoffDialog session={session} onClose={onClose} onHandedOff={onHandedOff} /></DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { request, onClose, onHandedOff, toastError: daemon.toastError };
}

const buttons = () => Array.from(document.body.querySelectorAll("button"));
const settle = () => act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

it("says what moves for a project session, then hands off to another agent and selects the receiving session", async () => {
  const owned = sessionOf("s-bound", parentConsole.id, project.id, "Bound", "archived", { bound_to: "s-1" });
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle", { bound_to: hub.id });
  const { request, onClose, onHandedOff } = mount(session, [hub, owned], [branch("feat/a"), branch("feat/b")]);
  await settle();

  const text = document.body.textContent ?? "";
  expect(text).toContain("2 branches it is responsible for");
  expect(text).toContain("1 session it owns, archived ones included");
  expect(text).toContain("Its place under Hub");

  await act(async () => buttons().find((b) => b.textContent === "Hand off")?.click());
  await settle();
  const sent = request.mock.calls.map(([body]) => body).find((body) => body.type === "hand_off_session");
  expect(sent).toMatchObject({ type: "hand_off_session", session: "s-1", agent: "codex", account: null, terminal_appearance: "dark" });
  expect(onHandedOff).toHaveBeenCalledExactlyOnceWith("s-new");
  expect(onClose).toHaveBeenCalledOnce();
});

it.each([
  [true, "soon"],
  [false, null],
])("hands off to the account whose weekly quota resets soonest only while automatic selection is on (%s)", async (autoSelect, expected) => {
  const now = Date.now();
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle");
  const { request } = mount(session, [], [], {}, null, [
    {
      agent: "codex", account: "soon", fetched_at: now, plan: null, unavailable: null,
      windows: [{ kind: "weekly", label: null, used_percent: 1, resets_at: now + 3_600_000, window_minutes: 7 * 24 * 60 }],
    },
  ], {
    auto_select_account: autoSelect,
    accounts: [{ id: "soon", agent: "codex", name: "Soon", config_dir: "/tmp/codex-soon" }],
  });
  await settle();
  await act(async () => buttons().find((button) => button.textContent === "Hand off")?.click());
  await settle();
  const sent = request.mock.calls.map(([body]) => body).find((body) => body.type === "hand_off_session");
  expect(sent).toMatchObject({ type: "hand_off_session", agent: "codex", account: expected });
});

it("asks for no branches of a console session and says when nothing moves", async () => {
  const { request } = mount(hub);
  await settle();
  expect(request.mock.calls.some(([body]) => body.type === "inspect_session_branches")).toBe(false);
  expect(document.body.textContent).toContain("it is responsible for no branches, owns no sessions and reports to no session");
});

it("says the branches could not be checked rather than that nothing moves", async () => {
  mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle"), [], "fail");
  await settle();
  const text = document.body.textContent ?? "";
  expect(text).toContain("Could not check which branches it is responsible for.");
  expect(text).not.toContain("it is responsible for no branches");
  expect(document.body.querySelector("h3")?.textContent).toBe("Moves to the new session");
});

it("closes and reports a plain alert, with no action, when no other agent can be picked", async () => {
  const { request, onClose, toastError } = mount(hub, [], [], { codex: "unavailable", grok: "unavailable" });
  await settle();
  expect(onClose).toHaveBeenCalled();
  expect(toastError).toHaveBeenCalledWith(expect.objectContaining({ message: "No other agent is available to take this session over.", recovery: "none" }));
  expect(request.mock.calls.some(([body]) => body.type === "hand_off_session")).toBe(false);
});

it("also closes when the only other agents are not added", async () => {
  const { onClose, toastError } = mount(hub, [], [], { claude: "available", codex: "available", grok: "available" }, ["claude"]);
  await settle();
  expect(onClose).toHaveBeenCalled();
  expect(toastError).toHaveBeenCalledOnce();
});

it("shows the receiving agent's only account in an account picker, its usage drawn in the option", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle");
  const quota: AccountQuota = {
    agent: "codex", account: null, fetched_at: Date.now(), plan: null, unavailable: null,
    windows: [{ kind: "session", label: null, used_percent: 30, resets_at: Date.now() + 3_600_000, window_minutes: 300 }],
  };
  mount(session, [], [], { claude: "available", codex: "available", grok: "unavailable" }, null, [quota]);
  await settle();
  const triggers = [...document.body.querySelectorAll('[data-slot="select-trigger"]')];
  const accountPicker = triggers.find((trigger) => trigger.getAttribute("aria-label")?.includes("Codex account"));
  expect(accountPicker?.textContent).toContain("Default");
  expect(triggers[0].getAttribute("aria-describedby")).toBeNull();
  expect(document.body.querySelectorAll('[class*="quota-meter--"]')).toHaveLength(1);
});
