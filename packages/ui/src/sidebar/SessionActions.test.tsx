// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import type { RequestHandler } from "../requests";
import { enter, openTooltip, press } from "../components/hint/tooltipTestSupport";
import { sessionOf } from "../fixtures/builders";
import type { Session } from "../protocol";
import type { State } from "../store";
import { ariaBinding } from "../shortcuts/binding";
import { defaultBinding } from "../shortcuts/commands";
import { restoreAllBindings } from "../shortcuts/store";
import { BROWSER_HOST } from "../test/hosts";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { SessionActions } from "./SessionActions";
import type { SidebarHandlers } from "./types";
import { fakeTimers, runOutTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(session: Session, inspect: RequestHandler, state: Partial<State> = {}, others: Session[] = []) {
  const handlers = { onOpenDialog: vi.fn(), onArchiveSession: vi.fn(), onCopySessionId: vi.fn(), onBindSession: vi.fn() } as unknown as SidebarHandlers;
  const request = vi.fn(inspect);
  const daemon = testDaemon({ state: { sessions: new Map([session, ...others].map((s) => [s.id, s])), ...state }, request });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (next: Session) => act(() => root.render(
    <TestHost daemon={daemon}>
      <SessionActions selected session={next} handlers={handlers} accounts={[]} hasBoundSessions={false} rowRef={{ current: container }} />
    </TestHost>,
  ));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render(session);
  return { container, daemon, request, handlers, render };
}

afterEach(() => {
  restoreAllBindings();
  localStorage.clear();
});

const unused = sessionOf("unused", "console", "project", "Unused", "archived", { has_conversation: false });
const shortcut = (container: HTMLElement, label: string) => container.querySelector<HTMLButtonElement>(`button[aria-label^="${label} "]`);

it.each<[Session["status"], boolean, string | undefined]>([
  ["archived", true, undefined],
  ["archived", false, "Delete"],
  ["idle", false, "Delete"],
  ["working", false, "Delete"],
  ["starting", false, undefined],
  ["idle", true, "Archive"],
])("offers a shortcut for %s, conversation=%s without inspecting native records", (status, has_conversation, label) => {
  const session = { ...unused, status, has_conversation };
  const { container, request, handlers } = mount(session, async () => { throw new Error("No inspection expected"); });
  expect(shortcut(container, "Delete") !== null).toBe(label === "Delete");
  expect(shortcut(container, "Archive") !== null).toBe(label === "Archive");
  expect(request).not.toHaveBeenCalled();
  if (label === "Delete") {
    act(() => shortcut(container, "Delete")!.click());
    expect(handlers.onOpenDialog).toHaveBeenCalledWith({ kind: "delete-session", session });
  }
});

it("offers a bound unused session the user opened the Delete shortcut", () => {
  const { container } = mount({ ...unused, status: "idle", bound_to: "owner", origin: "user" }, async () => ({ type: "ack", id: null }));
  expect(shortcut(container, "Delete")).not.toBeNull();
});

it("removes the delete shortcut as soon as the first conversation is reported", () => {
  const { container, request, render } = mount({ ...unused, status: "idle" }, async () => ({ type: "ack", id: null }));
  expect(shortcut(container, "Delete")).not.toBeNull();
  render({ ...unused, status: "working", has_conversation: true });
  expect(shortcut(container, "Delete")).toBeNull();
  expect(request).not.toHaveBeenCalled();
});

it.each([false, true])("hides removal shortcuts on a locked session, conversation=%s", (has_conversation) => {
  const { container } = mount({ ...unused, status: "idle", has_conversation, locked: true }, async () => ({ type: "ack", id: null }));
  expect(shortcut(container, "Delete")).toBeNull();
  expect(shortcut(container, "Archive")).toBeNull();
});

it.each([true, false])("gives the Copy session ID row's ID its full text as a tooltip only while it is clipped, clipped=%s", async (clipped) => {
  fakeTimers();
  // jsdom lays nothing out, so the overflow every label measures is stubbed.
  for (const [name, value] of [["scrollWidth", clipped ? 300 : 100], ["clientWidth", 100]] as const) {
    const own = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, own));
  }
  const session = { ...unused, id: "3f2c9a1e-7b4d-4e8a-9c61-0d5b2f8e4a17", status: "idle" as const, has_conversation: true };
  const { container, handlers } = mount(session, async () => ({ type: "ack", id: null }));
  press(container.querySelector<HTMLElement>('button[aria-label^="Actions for session"]')!, "mouse");
  await act(async () => {});
  const item = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.getAttribute("aria-label") === "Copy session ID")!;
  const id = [...item.querySelectorAll<HTMLElement>("[dir=ltr]")].find((el) => el.textContent === session.id)!;

  enter(id);
  runOutTimers();
  const tooltip = openTooltip();
  if (clipped) {
    // The bare ID, laid out left to right like the label under a right-to-left language too.
    expect(tooltip?.textContent).toBe(session.id);
    expect(tooltip?.querySelector("[dir=ltr]")?.textContent).toBe(session.id);
  } else {
    expect(tooltip).toBeNull();
  }

  // A press on the ID line is a press of the item, which still copies.
  press(id, "mouse");
  expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(session);
});

it("shows menu-only letters instead of selected-session window shortcuts", async () => {
  const session = { ...unused, status: "idle" as const, has_conversation: true, kind: "console" as const };
  const { container, handlers } = mount(session, async () => ({ type: "ack", id: null }));
  for (const letter of ["L", "P", "A", "C"]) {
    act(() => container.dispatchEvent(new KeyboardEvent("keydown", { key: letter.toLowerCase(), code: `Key${letter}`, bubbles: true })));
  }
  expect(handlers.onArchiveSession).not.toHaveBeenCalled();
  expect(handlers.onCopySessionId).not.toHaveBeenCalled();
  press(container.querySelector<HTMLElement>('button[aria-label^="Actions for session"]')!, "mouse");
  await act(async () => {});
  const shown = (label: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent?.startsWith(label))!;
  for (const [label, letter] of [["Lock", "L"], ["Pin", "P"], ["Rename", "R"], ["Copy session ID", "C"], ["Change model", "M"], ["Archive", "A"]]) {
    expect(shown(label).querySelector("kbd")?.textContent).toBe(letter);
    expect(shown(label).getAttribute("aria-keyshortcuts")).toBe(letter);
  }
  expect(shown("Focus mode").querySelector("kbd")?.textContent?.endsWith("→")).toBe(true);
  expect(shown("Focus mode").getAttribute("aria-keyshortcuts")).toBe(ariaBinding(defaultBinding("enterFocusMode"), BROWSER_HOST));
  expect(shown("Colour").querySelector("kbd")).toBeNull();
  expect(shown("Colour").getAttribute("aria-keyshortcuts")).toBeNull();
  act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "b", bubbles: true, cancelable: true })));
  expect(document.querySelector('[role="listbox"]')).toBeNull();
  act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true, cancelable: true })));
  expect(handlers.onCopySessionId).toHaveBeenCalledExactlyOnceWith(session);
});

// Open in the agent's app is in the menu where the agent has an app and the daemon found it installed; the menu asks the
// daemon about the session whenever it opens.
const CONVERSATION = "0b8f1c52-3d1e-4a7b-9c41-5e6d7f8091a2";
const asleep = (extra: Partial<Session> = {}) => sessionOf("asleep", "console", "project", "Asleep", "interrupted", { agent_session_id: CONVERSATION, ...extra });

/** Answers a probe with `agents` installed and every session asked about seen, and acknowledges anything else. */
const probeAnswering = (agents: string[]): RequestHandler => async (body) =>
  body.type === "probe_native_apps" ? { type: "native_apps", id: null, agents: agents as never, sessions_seen: body.sessions ?? [] } : { type: "ack", id: null };

async function openMenu(container: HTMLElement) {
  press(container.querySelector<HTMLElement>('button[aria-label^="Actions for session"]')!, "mouse");
  await act(async () => {});
  return [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
}
const itemNamed = (items: HTMLElement[], label: string) => items.find((el) => el.textContent?.startsWith(label));

it("lists Open in the app for a session whose agent's app is installed, and opens the session in it when chosen", async () => {
  const { container, request } = mount(asleep(), probeAnswering(["claude"]), { nativeApps: ["claude"] });
  const items = await openMenu(container);

  const open = itemNamed(items, "Open in Claude app");
  expect(open).toBeDefined();
  expect(open!.getAttribute("aria-disabled")).not.toBe("true");
  press(open!, "mouse");
  await act(async () => {});

  expect(request).toHaveBeenCalledWith({ type: "open_session_in_app", session: "asleep" });
});

it("leaves Open in the app out where the app is not installed, or the agent has none", async () => {
  const notInstalled = mount(asleep(), probeAnswering([]), { nativeApps: [] });
  expect(itemNamed(await openMenu(notInstalled.container), "Open in Claude app")).toBeUndefined();
  act(() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));

  const noApp = mount(asleep({ id: "grok-asleep", agent: "grok" }), probeAnswering(["grok"]), { nativeApps: ["grok" as never] });
  const items = await openMenu(noApp.container);
  expect(items.some((el) => /^Open in /.test(el.getAttribute("aria-label") ?? ""))).toBe(false);
});

it("probes the session when its menu opens, not before, so an app installed since is found", async () => {
  const { container, request } = mount(asleep(), probeAnswering(["claude"]), { nativeApps: ["claude"] });
  expect(request).not.toHaveBeenCalled();

  await openMenu(container);

  expect(request).toHaveBeenCalledWith({ type: "probe_native_apps", sessions: ["asleep"] });
});

const NOT_SEEN_SENTENCE = "The agent's app may not see this conversation: its latest record is kept in this account's own directory, not the default one.";
const notSeenAnswer: RequestHandler = async (body) => body.type === "probe_native_apps" ? { type: "native_apps", id: null, agents: ["claude"], sessions_seen: [], sessions_unseen: body.sessions ?? [] } : { type: "ack", id: null };
const openInAppItem = () => itemNamed([...document.querySelectorAll<HTMLElement>("[role=menuitem]")], "Open in Claude app");
const descriptionOf = (item: HTMLElement | undefined) => document.getElementById(item?.getAttribute("aria-describedby") ?? "")?.textContent;

it("keeps Open in the app enabled, noting that the app may not see the conversation, when the probe finds its latest record is not in the default directory", async () => {
  const { container } = mount(asleep(), notSeenAnswer, { nativeApps: ["claude"] });
  await openMenu(container);
  // The probe's answer is recorded after the menu has opened.
  await act(async () => {});

  const item = openInAppItem();
  expect(item?.getAttribute("aria-disabled")).not.toBe("true");
  expect(item?.textContent).toContain("The agent's app may not see this conversation");
});

it("gives the whole sentence as the description of Open in the app, which stays enabled, where the conversation is already known to be unseen", async () => {
  const { container, request } = mount(asleep(), notSeenAnswer, { nativeApps: ["claude"], nativeAppsUnseen: new Set(["asleep"]) });
  await openMenu(container);
  await act(async () => {});

  const item = openInAppItem();
  expect(item?.getAttribute("aria-disabled")).not.toBe("true");
  expect(item?.getAttribute("aria-label")).toBe("Open in Claude app");
  expect(item?.textContent).toContain("The agent's app may not see this conversation");
  expect(descriptionOf(item)).toBe(NOT_SEEN_SENTENCE);

  press(item!, "mouse");
  await act(async () => {});
  expect(request).toHaveBeenCalledWith({ type: "open_session_in_app", session: "asleep" });
});

it("gives the whole sentence as the description of Open in the app also where the probe finds the conversation unseen only after the menu opened", async () => {
  const { container } = mount(asleep(), notSeenAnswer, { nativeApps: ["claude"] });
  await openMenu(container);
  await act(async () => {});

  expect(descriptionOf(openInAppItem())).toBe(NOT_SEEN_SENTENCE);
});

it.each([
  ["the session has a process", { status: "idle" as const }, ["claude"] as string[], "Session running"],
])("disables Open in the app, saying why, when %s", async (_name, extra, seen, reason) => {
  const answer: RequestHandler = async (body) => body.type === "probe_native_apps" ? { type: "native_apps", id: null, agents: ["claude"], sessions_seen: seen.length ? (body.sessions ?? []) : [] } : { type: "ack", id: null };
  const { container } = mount(asleep(extra), answer, { nativeApps: ["claude"] });
  await openMenu(container);
  // The probe's answer is recorded after the menu has opened.
  await act(async () => {});

  const item = itemNamed([...document.querySelectorAll<HTMLElement>("[role=menuitem]")], "Open in Claude app");
  expect(item?.getAttribute("aria-disabled")).toBe("true");
  expect(item?.textContent).toContain(reason);
});

describe("the Bind to entry of a session's menu", () => {
  const hub = sessionOf("hub", "console", undefined, "Hub", "idle");
  const coordinator = sessionOf("coordinator", "console", "project", "Coordinator", "idle");
  const free = sessionOf("free", "console", "project", "Free", "idle");
  const openMenu = async (session: Session, others: Session[]) => {
    const mounted = mount(session, async () => ({ type: "ack", id: null }), {}, others);
    press(mounted.container.querySelector<HTMLElement>('button[aria-label^="Actions for session"]')!, "mouse");
    await act(async () => {});
    return mounted;
  };
  const menuLabels = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].map((el) => el.textContent ?? "");
  const openBind = async () => {
    await act(async () => {
      const item = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent?.startsWith("Bind to"))!;
      item.focus();
      item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    });
  };

  it("is offered to an independent project session, listing the sessions that can own it", async () => {
    const handlers = (await openMenu(free, [hub, coordinator])).handlers;
    expect(menuLabels().some((label) => label.startsWith("Bind to"))).toBe(true);
    await openBind();
    const groups = [...document.querySelectorAll("[role=group]")].map((group) => [...group.querySelectorAll("[role=menuitem]")].map((el) => el.textContent));
    expect(groups).toEqual([["Hub"], ["Coordinator"]]);
    await act(async () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === "Coordinator")!.click());
    expect(handlers.onBindSession).toHaveBeenCalledExactlyOnceWith(free, coordinator);
  });

  it("lists no project session while a session, archived too, is bound to this one", async () => {
    const archivedChild = sessionOf("child", "console", "project", "Child", "archived", { bound_to: "free" });
    await openMenu(free, [hub, coordinator, archivedChild]);
    await openBind();
    const groups = [...document.querySelectorAll("[role=group]")].map((group) => [...group.querySelectorAll("[role=menuitem]")].map((el) => el.textContent));
    expect(groups).toEqual([["Hub"]]);
  });

  it.each<[string, Session, string[]]>([
    ["a console session", hub, []],
    ["an archived session", { ...free, status: "archived" }, []],
    ["a bound session, which offers Unbind", { ...free, bound_to: "hub" }, ["Unbind"]],
  ])("is not offered to %s", async (_name, session, expected) => {
    await openMenu(session, [hub, coordinator]);
    const labels = menuLabels();
    expect(labels.some((label) => label.startsWith("Bind to"))).toBe(false);
    for (const label of expected) expect(labels.some((entry) => entry.startsWith(label))).toBe(true);
  });
});
