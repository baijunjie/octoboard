// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { openDetails } from "../components/detailsTestSupport";
import { errorAlerts } from "../errorAlerts";
import { isolate } from "../i18n/isolate";
import type { Event, HistoryMessage, Session, SessionContext } from "../protocol";
import { SessionHistoryView } from "./SessionHistoryView";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const daemon = vi.hoisted(() => ({ request: vi.fn<(body: unknown) => Promise<unknown>>() }));
const store = vi.hoisted(() => ({ snapshotEpoch: 0, consoles: new Map(), projects: new Map(), sessions: new Map(), homeDir: "/home/dev" }));
vi.mock("../errorAlerts", () => ({ errorAlerts: { report: vi.fn() } }));
vi.mock("../store", () => ({
  useDaemon: () => daemon,
  useDaemonStore: (select: (state: typeof store) => unknown) => select(store),
  useErrorRecords: (select: (state: typeof store) => unknown) => select(store),
}));
vi.mock("../platform/react", () => ({ usePlatform: () => ({ links: undefined }) }));
vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));
vi.mock("../viewer/MarkdownDocument", () => ({
  MarkdownDocument: ({ text }: { text: string }) => <div data-testid="markdown">{text}</div>,
}));

const session = { id: "s-1", started_at: 1700000000000, ended_at: 1700000600000 };
const roots: Root[] = [];
beforeEach(() => {
  daemon.request.mockReset();
  store.snapshotEpoch = 0;
  store.sessions = new Map();
  vi.mocked(errorAlerts.report).mockClear();
});
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
});

const history: HistoryMessage[] = [
  { speaker: "octoboard", text: "## Goal\n\nShip it", turn: 1 },
  { speaker: "agent", text: "**Planned**", turn: 1 },
  { speaker: "user", text: "Why is it slow?", turn: 2, at: 1_792_334_800_000 },
  { speaker: "agent", text: "The query scans the table.", turn: 2 },
];

async function mount(reply: Event | Promise<Event> | Error, context?: SessionContext): Promise<HTMLElement> {
  daemon.request.mockImplementation(async () => {
    if (reply instanceof Error) throw reply;
    return reply;
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  await act(async () => root.render(<SessionHistoryView session={{ ...session, context }} />));
  // Settle the lazy renderer even when this is the first test to show an agent reply.
  if (!vi.isFakeTimers()) await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  return container;
}

const articles = (container: HTMLElement) =>
  [...container.querySelectorAll("article")].map((node) => document.getElementById(node.getAttribute("aria-labelledby")!)?.textContent);

it("shows the context size beside the heading, before the dates", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history }, { tokens: 130000, window: 200000, used_percent: 65 });
  const header = container.querySelector("header")!;
  const badge = header.querySelector<HTMLElement>('[role="img"]')!;
  expect(header.querySelector("h2")!.compareDocumentPosition(badge) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(badge.compareDocumentPosition(header.querySelector("time")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(badge.getAttribute("aria-label")).toContain(`${isolate("65%")} used`);
  expect(badge.querySelector("[data-variant] > .progress-circle")).not.toBeNull();

});

it("shows no context badge in the header for a session whose size is unknown", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history });
  expect(container.querySelector('header [role="img"]')).toBeNull();
});

it("shows the session's archive and creation times even when its history is unavailable", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: [], unavailable: true });
  const times = [...container.querySelectorAll<HTMLTimeElement>("header time")];
  expect(times.map((time) => time.dateTime)).toEqual(["2023-11-14T22:23:20.000Z", "2023-11-14T22:13:20.000Z"]);
  expect(times[0].textContent).toContain("Archived");
  expect(times[1].textContent).toContain("Created");
});

it("shows user messages and agent replies while omitting Octoboard messages", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history });
  expect(daemon.request).toHaveBeenCalledWith({ type: "read_session_history", session: "s-1" });
  expect(articles(container)).toEqual(["Agent", "You", "Agent"]);
  expect(container.textContent).toContain("Why is it slow?");
  expect(container.textContent).toContain("The query scans the table.");
  expect(container.textContent).not.toContain("Ship it");
  expect(container.querySelector("[role=switch]")).toBeNull();
  expect(container.querySelector("h2")?.textContent).toBe("Session history");
  expect(container.querySelector("[role=region]")?.getAttribute("aria-label")).toBe("Messages of this session");
});

it("renders agent replies as Markdown without a toggle", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history });
  expect([...container.querySelectorAll("[data-testid=markdown]")].map((node) => node.textContent)).toEqual([
    "**Planned**",
    "The query scans the table.",
  ]);
});

it("says nothing was said in a session with no messages", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: [] });
  expect(container.textContent).toContain("No messages in this session");
  expect(container.querySelector("article")).toBeNull();
});

it("points at reopening when the record is unavailable", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: [], unavailable: true });
  expect(container.textContent).toContain("History unavailable");
  // The record's state, not a failed load, so it is a neutral empty panel rather than a failure or an
  // Alert: the heading says what is missing, the line why, and the remedy under it, with no Try again
  // and no Details button.
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(panel.querySelector(".bg-danger-soft")).toBeNull();
  expect(panel.querySelector("h2")?.textContent).toBe("History unavailable");
  expect(panel.querySelector("p")?.textContent).toBe("The agent's record of this session could not be read.");
  expect(panel.querySelectorAll("p")[1]?.textContent).toBe("Reopen the session to see its terminal.");
  expect(panel.querySelector("button")).toBeNull();
  // The status region speaks it, so the panel is silent and does not announce itself as well.
  expect(panel.getAttribute("aria-hidden")).toBe("true");
  expect(container.querySelector("[role=alert]")).toBeNull();
});

it("shows a failed request as an error with Try again, and reads again when it is pressed", async () => {
  const container = await mount(new Error("gone"));
  expect(container.textContent).toContain("Could not load the history of this session");
  expect(container.textContent).toContain("Something unexpected went wrong.");
  expect(container.textContent).not.toContain("gone");
  // It arrived after the view painted, so the title and the reason announce themselves, and nothing else does.
  const announcement = container.querySelector("[role=alert]")!;
  expect(announcement.textContent).toBe("Could not load the history of this sessionSomething unexpected went wrong.");
  // The reason and Try again are inside the one panel.
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(openDetails(panel).textContent).toBe("gone");
  expect([...panel.querySelectorAll("button")].map((button) => button.textContent)).toEqual(["Try again", "Details"]);
  expect(errorAlerts.report).not.toHaveBeenCalled();
  expect(container.textContent).not.toContain("History unavailable");
  daemon.request.mockResolvedValue({ type: "session_history", session: "s-1", messages: history });
  const retry = [...container.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  await act(async () => retry.click());
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledTimes(2);
  expect(container.querySelector("[role=alert]")).toBeNull();
  expect(articles(container)).toEqual(["Agent", "You", "Agent"]);
});

it("reads a failed request again when a fresh snapshot arrives, but a loaded one is left alone", async () => {
  const container = await mount(new Error("gone"));
  daemon.request.mockResolvedValue({ type: "session_history", session: "s-1", messages: history });
  store.snapshotEpoch = 1;
  const root = roots[0]!;
  await act(async () => root.render(<SessionHistoryView session={session} />));
  await act(async () => {});
  expect(articles(container)).toEqual(["Agent", "You", "Agent"]);
  const reads = daemon.request.mock.calls.length;
  store.snapshotEpoch = 2;
  await act(async () => root.render(<SessionHistoryView session={session} />));
  expect(daemon.request.mock.calls.length).toBe(reads);
});

it("treats a history containing only Octoboard messages as empty", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history.filter((message) => message.speaker === "octoboard") });
  expect(container.textContent).toContain("No messages in this session");
  expect(container.querySelector("article")).toBeNull();
});

it("speaks an unavailable and an empty history through a status", async () => {
  vi.useFakeTimers();
  try {
    const unavailable = await mount({ type: "session_history", session: "s-1", messages: [], unavailable: true });
    await act(async () => void vi.advanceTimersByTime(200));
    expect(unavailable.querySelector("[role=status]")?.textContent).toContain("Reopen the session to see its terminal.");
    const empty = await mount({ type: "session_history", session: "s-1", messages: [] });
    await act(async () => void vi.advanceTimersByTime(200));
    expect(empty.querySelector("[role=status]")?.textContent).toContain("No messages in this session");
  } finally {
    vi.useRealTimers();
  }
});

it("shows a loading state until the reply arrives", async () => {
  daemon.request.mockImplementation(() => new Promise(() => {}));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  await act(async () => root.render(<SessionHistoryView session={session} />));
  await act(async () => void (await new Promise((resolve) => setTimeout(resolve, 200))));
  expect(container.querySelector("[role=status]")?.textContent).toBe("Loading history…");
  expect(container.querySelector("article")).toBeNull();
});

it("keeps focus in the view when Try again goes away under it, on the view while the messages are not there yet", async () => {
  const container = await mount(new Error("gone"));
  let answer!: (reply: unknown) => void;
  daemon.request.mockImplementation(() => new Promise((resolve) => { answer = resolve; }));
  const retry = [...container.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  retry.focus();
  await act(async () => retry.click());
  expect(document.activeElement).toBe(container.querySelector("section"));
  await act(async () => answer({ type: "session_history", session: "s-1", messages: history }));
  expect(container.querySelector("[role=region][tabindex]")).not.toBeNull();
  expect(document.activeElement).toBe(container.querySelector("section"));
});


it("retains the daemon reason for rich rendering inside the Alert, which announces it once and opens no error alert dialog", async () => {
  const failure = new DaemonRequestError("Session missing-session was not found.", "unknown_session", { session: "missing-session" });
  const container = await mount(failure);
  expect(container.querySelector("code")?.textContent).toBe("missing-session");
  expect(container.querySelectorAll("[role=alert]")).toHaveLength(1);
  expect(container.querySelector("[role=alert]")?.querySelector("button")).toBeNull();
  expect(errorAlerts.report).not.toHaveBeenCalled();
  await act(async () => roots[0]!.render(<SessionHistoryView session={session} />));
  expect(container.querySelectorAll("[role=alert]")).toHaveLength(1);
  expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Try again")).toBe(true);
});

const owner = { id: "owner-1", title: "Release captain", role: "console" } as Session;

async function mountChild(boundTo: string | undefined, onSelectSession: ((session: Session) => void) | null = vi.fn()): Promise<HTMLElement> {
  daemon.request.mockResolvedValue({ type: "session_history", session: "s-1", messages: history });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  await act(async () => root.render(<SessionHistoryView session={{ ...session, bound_to: boundTo }} onSelectSession={onSelectSession ?? undefined} />));
  await act(async () => {});
  return container;
}

it("links an owned session's history to its owner under the heading, and pressing it selects the owner", async () => {
  store.sessions = new Map([[owner.id, owner]]);
  const onSelectSession = vi.fn();
  const container = await mountChild("owner-1", onSelectSession);
  const link = [...container.querySelectorAll("header button")].find((button) => button.textContent?.includes("Release captain"))!;
  expect(link.closest("p")?.textContent).toBe("Reports to Release captain");
  await act(async () => (link as HTMLButtonElement).click());
  expect(onSelectSession).toHaveBeenCalledWith(owner);
});

it("rings the owner link only when it has keyboard focus itself, not when the history section does", async () => {
  store.sessions = new Map([[owner.id, owner]]);
  const container = await mountChild("owner-1");
  const section = container.querySelector<HTMLElement>("section")!;
  const link = container.querySelector<HTMLElement>("header button")!;
  act(() => setInteractionModality("keyboard"));
  act(() => section.focus());
  expect(section.hasAttribute("data-focus-visible")).toBe(true);
  expect(link.hasAttribute("data-focus-visible")).toBe(false);
  act(() => link.focus());
  expect(link.hasAttribute("data-focus-visible")).toBe(true);
});

it("shows no owner line for an unbound session or an owner the client does not know", async () => {
  store.sessions = new Map([[owner.id, owner]]);
  expect((await mountChild(undefined)).querySelector("header p")).toBeNull();
  expect((await mountChild("deleted-owner")).querySelector("header p")).toBeNull();
});

it("shows no owner line when nothing can show the owner", async () => {
  store.sessions = new Map([[owner.id, owner]]);
  expect((await mountChild("owner-1", null)).querySelector("header p")).toBeNull();
});
