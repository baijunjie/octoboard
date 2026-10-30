// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { press } from "../components/detailsTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { accountOf, consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { currentLanguage, setLanguage } from "../i18n/language";
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

function mount(session: Session, others: Session[] = [], branches: SessionBranchItem[] | "fail" | "failTwice" = [], availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {}, added: Agent[] | null = null, accountQuotas: AccountQuota[] = [], settings: Partial<Settings> = {}) {
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
    agentAvailability: new Map((Object.entries(availability) as [Agent, AgentAvailability["availability"]][]).map(([agent, state]) => [agent, { agent, availability: state, default_account_dir: null } as AgentAvailability])),
  });
  let inspections = 0;
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "inspect_session_branches" && branches === "failTwice" && ++inspections <= 2) throw new Error("git failed");
    if (body.type === "inspect_session_branches" && branches === "failTwice") return { type: "session_branches_inspected", id: null, items: [] };
    if (body.type === "inspect_session_branches" && branches === "fail") throw new Error("git failed");
    if (body.type === "inspect_session_branches") return { type: "session_branches_inspected", id: null, items: branches };
    if (body.type === "hand_off_session") return { type: "session_opened", id: null, session: sessionOf("s-new", parentConsole.id, session.project_id ?? undefined, session.title, "working", { agent: "codex" }) };
    return { type: "ack", id: null };
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
const elapse = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

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
      windows: [{ kind: "weekly", label: null, used_percent: 1, resets_at: now + 3_600_000, window_minutes: 7 * 24 * 60 }], email: null,
    },
  ], {
    auto_select_account: autoSelect,
    accounts: [accountOf("soon", "codex", "Soon", "/tmp/codex-soon")],
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

const mark = () => document.body.querySelector<HTMLElement>('button[aria-label="Branches to move unknown"]');
const popoverRetry = () => [...document.body.querySelectorAll<HTMLElement>("[data-status-mark-popover] button")].find((button) => button.textContent === "Try again");
const announcements = () => [...document.body.querySelectorAll('[role="status"]')].map((node) => node.textContent).filter((text) => text?.includes("Could not check"));

it("says the branches could not be checked rather than that nothing moves, in a mark after the heading", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { request } = mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle"), [], "fail");
  await elapse(0);
  const text = (document.body.textContent ?? "").replace(/[\u2066-\u2069]/g, "");
  expect(text).not.toContain("it is responsible for no branches");
  // A mark right after the heading, not an Alert and not a line of text beside it.
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(text).not.toContain("Could not check which branches it is responsible for");
  const heading = document.body.querySelector("h3")!;
  expect(heading.textContent).toBe("Moves to the new session");
  expect(heading.nextElementSibling).toBe(mark());
  expect(document.body.querySelector("li")).toBeNull();
  // The popover holds the sentence, the failure's own sentence and Try again.
  press(mark()!);
  const popover = document.body.querySelector('[role="dialog"][aria-label="Branches to move unknown"]')!;
  expect(popover).not.toBeNull();
  expect(popover.textContent).toContain("Could not check which branches it is responsible for");
  expect(popover.textContent).toContain("Something unexpected went wrong.");
  expect(popoverRetry()).toBeDefined();
  // It arrived after the dialog opened, so the dialog speaks it once, after the opening focus move.
  expect(announcements()).toEqual([]);
  await elapse(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(announcements()).toEqual(["Could not check which branches it is responsible for."]);
  const asked = () => request.mock.calls.filter(([body]) => body.type === "inspect_session_branches").length;
  expect(asked()).toBe(1);
  const spoken: string[] = [];
  const observer = new MutationObserver((records) => records.forEach((record) => record.addedNodes.forEach((node) => {
    if (node.textContent?.includes("Could not check")) spoken.push(node.textContent);
  })));
  observer.observe(document.body, { childList: true, subtree: true });
  onTestFinished(() => observer.disconnect());
  await act(async () => popoverRetry()!.click());
  expect(asked()).toBe(2);
  await elapse(0);
  // The retry failed again in the same words: the mark stays, and the sentence is written, so spoken, again.
  expect(mark()).not.toBeNull();
  expect(announcements()).toEqual(["Could not check which branches it is responsible for."]);
  expect(spoken).toEqual(["Could not check which branches it is responsible for."]);
});

it("keeps focus inside the dialog when a retry of the check succeeds and takes the mark away", async () => {
  mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle"), [], "failTwice");
  await settle();
  const dialog = document.body.querySelector('[role="dialog"]')!;
  act(() => mark()!.focus());
  press(mark()!);
  await act(async () => popoverRetry()!.click());
  await settle();
  // The retry failed again: the mark stays.
  expect(mark()).not.toBeNull();
  act(() => mark()!.focus());
  press(mark()!);
  await act(async () => popoverRetry()!.click());
  await settle();
  // The retry succeeded: the mark, which held focus, is gone, and focus did not fall to the page.
  expect(mark()).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(dialog.contains(document.activeElement)).toBe(true);
});

it("turns into an acknowledgement-only failure modal when no other agent can be picked", async () => {
  const { request, onClose, toastError } = mount(hub, [], [], { codex: "unavailable", grok: "unavailable" });
  await settle();
  const dialog = document.body.querySelector('[role="alertdialog"]');
  expect(dialog?.textContent).toContain("No other agent is available to take this session over.");
  expect(dialog?.querySelector("form")).toBeNull();
  expect(dialog?.querySelector('[data-slot="alert-dialog-icon"], [data-slot="modal-icon"]')).not.toBeNull();
  // Only the acknowledgement is offered, and it is the one thing that closes the dialog.
  expect(Array.from(dialog?.querySelectorAll("button") ?? []).map((button) => button.textContent)).toEqual(["OK"]);
  expect(onClose).not.toHaveBeenCalled();
  expect(toastError).not.toHaveBeenCalled();
  expect(request.mock.calls.some(([body]) => body.type === "hand_off_session")).toBe(false);
  await act(async () => buttons().find((b) => b.textContent === "OK")?.click());
  expect(onClose).toHaveBeenCalledOnce();
});

it("also turns into the failure modal when the only other agents are not added", async () => {
  const { onClose, toastError } = mount(hub, [], [], { claude: "available", codex: "available", grok: "available" }, ["claude"]);
  await settle();
  expect(document.body.querySelector('[role="alertdialog"]')?.textContent).toContain("No other agent is available");
  expect(onClose).not.toHaveBeenCalled();
  expect(toastError).not.toHaveBeenCalled();
});

it("shows the receiving agent's only account in an account picker, its usage drawn in the option", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle");
  const quota: AccountQuota = {
    agent: "codex", account: null, fetched_at: Date.now(), plan: null, unavailable: null,
    windows: [{ kind: "session", label: null, used_percent: 30, resets_at: Date.now() + 3_600_000, window_minutes: 300 }], email: null,
  };
  mount(session, [], [], { claude: "available", codex: "available", grok: "unavailable" }, null, [quota]);
  await settle();
  const triggers = [...document.body.querySelectorAll('[data-slot="select-trigger"]')];
  const accountPicker = triggers.find((trigger) => trigger.getAttribute("aria-label")?.includes("Codex account"));
  expect(accountPicker?.textContent).toContain("Default");
  expect(triggers[0].getAttribute("aria-describedby")).toBeNull();
  expect(document.body.querySelectorAll('[class*="quota-meter--"]')).toHaveLength(1);
});

it.each([["en", "handed off. A new session"], ["zh-Hans", "移交。另一个智能体的新会话"]] as const)("joins the two sentences as %s does", (language, joined) => {
  const before = currentLanguage();
  setLanguage(language);
  onTestFinished(() => setLanguage(before));
  mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle"));
  const sentence = document.body.querySelector("[role=dialog] p.text-sm")!;
  expect(sentence.textContent).toContain(joined);
  if (language === "en") {
    const whitespaceOnly = Array.from(sentence.childNodes).filter((node) => node.nodeType === Node.TEXT_NODE && node.textContent!.trim() === "");
    expect(whitespaceOnly).toEqual([]);
  }
});
