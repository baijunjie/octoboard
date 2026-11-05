// @vitest-environment jsdom
import { act, useState } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { Focus } from "lucide-react";

import { ActionMenu } from "../components/menus/ActionMenu";
import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { isolate } from "../i18n/isolate";
import { menuOfTrigger, partSelector } from "../primitives";
import type { Session } from "../protocol";
import { ariaBinding, eventKey } from "../shortcuts/binding";
import { defaultBinding } from "../shortcuts/commands";
import { bindingFor, restoreAllBindings, setBinding } from "../shortcuts/store";
import { SessionRow } from "../sidebar/SessionRow";
import { FocusModeContext } from "../sidebar/focusModeContext";
import type { FocusTarget, SidebarHandlers } from "../sidebar/types";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useFocusMode } from "./useFocusMode";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
afterEach(() => restoreAllBindings());

const console_ = consoleOf("c", "Main");
const project = projectOf("p", console_.id, "Website");
const hub = sessionOf("hub", console_.id, undefined, "Hub", "idle");
const coordinator = sessionOf("coordinator", console_.id, project.id, "Coordinator", "idle", { bound_to: hub.id });
const independent = sessionOf("independent", console_.id, project.id, "Independent", "idle");
const direct = sessionOf("direct", console_.id, project.id, "Direct", "idle", { bound_to: hub.id });
const nested = sessionOf("nested", console_.id, project.id, "Nested", "idle", { bound_to: coordinator.id });

function mount(selected: Session = nested) {
  const daemon = testDaemon({ state: { consoles: new Map([[console_.id, console_]]), projects: new Map([[project.id, project]]), sessions: new Map([hub, coordinator, independent, direct, nested].map((session) => [session.id, session])) } });
  const focused = vi.fn();
  const selectConsole = vi.fn();
  const handlers = { onFocus: focused, onOpenDialog: vi.fn(), onSetLocked: vi.fn(), onSetPinned: vi.fn() } as unknown as SidebarHandlers;
  function View() {
    const [focus, setFocus] = useState<FocusTarget>();
    const applyFocus = (next: FocusTarget | undefined) => { focused(next); setFocus(next); };
    useFocusMode({ focus, setFocus: applyFocus, selectConsole, currentConsoleId: console_.id, selectedSession: selected, sidebarShown: true, selectSession: vi.fn(), focusTerminal: vi.fn() });
    return <FocusModeContext value={focus !== undefined}>
      <input aria-label="Terminal" />
      {[selected, ...(selected.id === independent.id ? [] : [independent])].map((session) => <SessionRow key={session.id} session={session} selected={session.id === selected.id} handlers={{ ...handlers, onFocus: applyFocus }} />)}
      <ActionMenu label="Other menu" items={[{ label: "Rename", icon: Focus, onClick: vi.fn() }]} />
    </FocusModeContext>;
  }
  const host = mountHost(<View />, { daemon });
  const terminal = host.querySelector<HTMLInputElement>("input")!;
  const open = async (title = selected.title) => {
    const trigger = host.querySelector<HTMLButtonElement>(`button[aria-label="Actions for session ${isolate(title)}"]`)!;
    await act(async () => trigger.click());
    return menuOfTrigger(trigger) as HTMLElement;
  };
  const press = (target: EventTarget = terminal, init: KeyboardEventInit = {}) => {
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...eventKey(bindingFor("focusMode"), BROWSER_HOST), ...init });
    act(() => { target.dispatchEvent(event); });
    return event;
  };
  return { host, daemon, focused, selectConsole, open, press, terminal };
}

const focusItem = (menu: Element) => [...menu.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.querySelector(partSelector.itemLabel)?.textContent === "Focus mode");

it.each([[independent, { project }], [hub, { consoleSession: hub }], [direct, { consoleSession: hub }], [nested, { consoleSession: hub }]] as const)("enters the selected session's context from the terminal: $0.title", (selected, target) => {
  const { focused, selectConsole, press, terminal } = mount(selected);
  const leaked = vi.fn();
  terminal.addEventListener("keydown", leaked);
  expect(press().defaultPrevented).toBe(true);
  expect(focused).toHaveBeenCalledExactlyOnceWith(target);
  expect(selectConsole).toHaveBeenCalledExactlyOnceWith(console_.id);
  expect(leaked).not.toHaveBeenCalled();
  press();
  expect(focused).toHaveBeenLastCalledWith(undefined);
});

it("reacts to owner-chain changes without a change to the selected session", () => {
  const { daemon, focused, press } = mount();
  const nextHub = { ...hub, id: "new-hub", title: "New hub" };
  act(() => daemon.store.setState((state) => ({ sessions: new Map(state.sessions).set(nextHub.id, nextHub).set(coordinator.id, { ...coordinator, bound_to: nextHub.id }) })));
  press();
  expect(focused).toHaveBeenCalledExactlyOnceWith({ consoleSession: nextHub });
  press();
  act(() => daemon.store.setState((state) => ({ sessions: new Map(state.sessions).set(nextHub.id, { ...nextHub, status: "archived" }) })));
  press();
  expect(focused).toHaveBeenCalledTimes(2);
});

it.each([independent, direct, nested, hub])("offers Focus mode only on the selected $title menu and runs its advertised chord", async (selected) => {
  const { open, press, focused } = mount(selected);
  const menu = await open();
  const item = focusItem(menu)!;
  expect(item).toBeDefined();
  expect(item.getAttribute("aria-keyshortcuts")).toBe(ariaBinding(defaultBinding("focusMode"), BROWSER_HOST));
  expect(item.querySelector("kbd")?.textContent?.endsWith("F")).toBe(true);
  expect(item.querySelector("kbd")?.textContent).not.toBe("F");
  act(() => menu.dispatchEvent(new KeyboardEvent("keydown", { key: "f", bubbles: true, cancelable: true })));
  expect(focused).not.toHaveBeenCalled();
  expect(press(menu).defaultPrevented).toBe(true);
  expect(focused).toHaveBeenCalledOnce();
  expect(document.querySelector("[role=menu]")).toBeNull();
  press(undefined, { repeat: true });
  expect(focused).toHaveBeenCalledTimes(1);
  const reopened = await open();
  expect(focusItem(reopened)).toBeUndefined();
});

it("updates an open selected menu when an intermediate owner's binding changes", async () => {
  const { daemon, focused, open, press } = mount();
  const menu = await open();
  const next = { ...hub, id: "new-hub", title: "Other hub" };
  act(() => daemon.store.setState((state) => ({ sessions: new Map(state.sessions).set(next.id, next).set(coordinator.id, { ...coordinator, bound_to: next.id }) })));
  press(menu);
  expect(focused).toHaveBeenCalledExactlyOnceWith({ consoleSession: next });
});

it("leaves an unselected session unchanged when its menu is opened", async () => {
  const { open, press, focused } = mount();
  const menu = await open(independent.title);
  expect(focusItem(menu)).toBeUndefined();
  expect(press(menu).defaultPrevented).toBe(true);
  expect(focused).not.toHaveBeenCalled();
});

it("updates the open item's hint and activation when the global binding changes", async () => {
  const { open, press, focused } = mount();
  const menu = await open();
  const old = defaultBinding("focusMode");
  const binding = { code: "KeyG", primary: true, alt: true };
  act(() => { expect(setBinding("focusMode", binding, BROWSER_HOST)).toBeUndefined(); });
  expect(focusItem(menu)?.getAttribute("aria-keyshortcuts")).toBe(ariaBinding(binding, BROWSER_HOST));
  expect(focusItem(menu)?.querySelector("kbd")?.textContent?.endsWith("G")).toBe(true);
  press(menu, eventKey(old, BROWSER_HOST));
  expect(focused).not.toHaveBeenCalled();
  press(menu);
  expect(focused).toHaveBeenCalledExactlyOnceWith({ consoleSession: hub });
});

it("suppresses the command in other menus, dialogs, composition and repeated presses", async () => {
  const { host, open, press, focused } = mount();
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="Other menu"]')!.click());
  press(document.activeElement!);
  expect(focused).not.toHaveBeenCalled();
  act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  const menu = await open();
  press(menu, { repeat: true });
  press(menu, { isComposing: true });
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  press(menu);
  dialog.remove();
  expect(focused).not.toHaveBeenCalled();
});
