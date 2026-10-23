// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { detailsButton, openDetails } from "../components/detailsTestSupport";
import { errorAlerts } from "../errorAlerts";
import type { Event, HistoryMessage } from "../protocol";
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

const roots: Root[] = [];
beforeEach(() => {
  daemon.request.mockReset();
  store.snapshotEpoch = 0;
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

async function mount(reply: Event | Promise<Event> | Error): Promise<HTMLElement> {
  daemon.request.mockImplementation(async () => {
    if (reply instanceof Error) throw reply;
    return reply;
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  await act(async () => root.render(<SessionHistoryView sessionId="s-1" />));
  // The lazily loaded renderer settles after the reply.
  await act(async () => {});
  return container;
}

const articles = (container: HTMLElement) =>
  [...container.querySelectorAll("article")].map((node) => document.getElementById(node.getAttribute("aria-labelledby")!)?.textContent);

it("asks for the session's history and lists the user's and Octoboard's messages by default", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history });
  expect(daemon.request).toHaveBeenCalledWith({ type: "read_session_history", session: "s-1" });
  expect(articles(container)).toEqual(["Octoboard", "You"]);
  expect(container.textContent).toContain("Why is it slow?");
  expect(container.textContent).not.toContain("The query scans the table.");
  expect(container.querySelector("h2")?.textContent).toBe("Session history");
  expect(container.querySelector("[role=region]")?.getAttribute("aria-label")).toBe("Messages of this session");
});

it("shows each turn's reply, rendered as Markdown, once the toggle is on", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history });
  const toggle = container.querySelector<HTMLInputElement>("input[role=switch]")!;
  expect(toggle.getAttribute("aria-labelledby")).not.toBeNull();
  await act(async () => toggle.click());
  expect(articles(container)).toEqual(["Octoboard", "Agent", "You", "Agent"]);
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
  // A read failure that leaves the pane empty is an empty-state panel in the danger tone, not an
  // Alert: the heading says what failed, the line why, and the remedy under it, with no Details button
  // as there is no raw text.
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(panel.querySelector("h2")?.textContent).toBe("History unavailable");
  expect(panel.querySelector("p")?.textContent).toBe("The agent's record of this session could not be read.");
  expect(panel.querySelectorAll("p")[1]?.textContent).toBe("Reopen the session to see its terminal.");
  expect(detailsButton(panel)).toBeUndefined();
  // The status region speaks it, so the panel does not announce itself as well.
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
  expect(articles(container)).toEqual(["Octoboard", "You"]);
});

it("reads a failed request again when a fresh snapshot arrives, but a loaded one is left alone", async () => {
  const container = await mount(new Error("gone"));
  daemon.request.mockResolvedValue({ type: "session_history", session: "s-1", messages: history });
  store.snapshotEpoch = 1;
  const root = roots[0]!;
  await act(async () => root.render(<SessionHistoryView sessionId="s-1" />));
  await act(async () => {});
  expect(articles(container)).toEqual(["Octoboard", "You"]);
  const reads = daemon.request.mock.calls.length;
  store.snapshotEpoch = 2;
  await act(async () => root.render(<SessionHistoryView sessionId="s-1" />));
  expect(daemon.request.mock.calls.length).toBe(reads);
});

it("names each message by its speaker and keeps a message's key when replies are toggled", async () => {
  const container = await mount({ type: "session_history", session: "s-1", messages: history });
  const article = container.querySelector("article")!;
  expect(article.getAttribute("aria-label")).toBeNull();
  expect(document.getElementById(article.getAttribute("aria-labelledby")!)?.textContent).toBe("Octoboard");
  const user = container.querySelectorAll("article")[1]!;
  await act(async () => container.querySelector<HTMLInputElement>("input[role=switch]")!.click());
  expect(container.querySelectorAll("article")[2]).toBe(user);
});

it("speaks the outcome: unavailable and empty through a status, a failure through an alert", async () => {
  vi.useFakeTimers();
  try {
    const container = await mount({ type: "session_history", session: "s-1", messages: [], unavailable: true });
    await act(async () => void vi.advanceTimersByTime(200));
    expect(container.querySelector("[role=status]")?.textContent).toContain("Reopen the session to see its terminal.");
  } finally {
    vi.useRealTimers();
  }
});

it("shows a loading state until the reply arrives", async () => {
  daemon.request.mockImplementation(() => new Promise(() => {}));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  await act(async () => root.render(<SessionHistoryView sessionId="s-1" />));
  await act(async () => void (await new Promise((resolve) => setTimeout(resolve, 200))));
  expect(container.querySelector("[role=status]")?.textContent).toBe("Loading history…");
  expect(container.querySelector("article")).toBeNull();
});

it("keeps focus in the view when Try again goes away under it, on the switch while the messages are not there yet", async () => {
  const container = await mount(new Error("gone"));
  let answer!: (reply: unknown) => void;
  daemon.request.mockImplementation(() => new Promise((resolve) => { answer = resolve; }));
  const retry = [...container.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  retry.focus();
  await act(async () => retry.click());
  expect(document.activeElement).toBe(container.querySelector("input[role=switch]"));
  await act(async () => answer({ type: "session_history", session: "s-1", messages: history }));
  expect(container.querySelector("[role=region][tabindex]")).not.toBeNull();
  expect(document.activeElement).toBe(container.querySelector("input[role=switch]"));
});


it("retains the daemon reason for rich rendering inside the Alert, which announces it once and opens no error alert dialog", async () => {
  const failure = new DaemonRequestError("Session missing-session was not found.", "unknown_session", { session: "missing-session" });
  const container = await mount(failure);
  expect(container.querySelector("code")?.textContent).toBe("missing-session");
  expect(container.querySelectorAll("[role=alert]")).toHaveLength(1);
  expect(container.querySelector("[role=alert]")?.querySelector("button")).toBeNull();
  expect(errorAlerts.report).not.toHaveBeenCalled();
  await act(async () => roots[0]!.render(<SessionHistoryView sessionId="s-1" />));
  expect(container.querySelectorAll("[role=alert]")).toHaveLength(1);
  expect([...container.querySelectorAll("button")].some((button) => button.textContent === "Try again")).toBe(true);
});
