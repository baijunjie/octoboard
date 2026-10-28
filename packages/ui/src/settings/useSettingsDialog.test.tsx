// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { setFocusAfterModalLoss } from "../components/regionFocus";
import { PlatformProvider } from "../platform/react";
import { useSettingsDialog } from "./useSettingsDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(otherModalOpen: boolean, focusTerminal: () => void) {
  let hook!: ReturnType<typeof useSettingsDialog>;
  function Probe() {
    hook = useSettingsDialog({ ready: true, otherModalOpen, focusTerminal });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "browser" }}><Probe /></PlatformProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return () => hook;
}

it("hands focus to the terminal when Settings closes with nothing open under it", () => {
  const focusTerminal = vi.fn();
  const hook = mount(false, focusTerminal);
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  expect(focusTerminal).toHaveBeenCalledOnce();
});

const frame = () => act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });

/** Runs the timers a closing modal's exit transition and React Aria's focus restore are given, after which the window's
 * fallback for focus left on the body runs. Needs `vi.useFakeTimers()`. */
const settle = () => act(() => { vi.advanceTimersByTime(450); });

/** Registers, as the window does, where focus left on the body goes: here a stand-in for the sidebar's selected row. */
function registerFallback() {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  const row = document.body.appendChild(document.createElement("button"));
  const fallback = vi.fn(() => row.focus());
  const unregister = setFocusAfterModalLoss(fallback);
  onTestFinished(() => {
    unregister();
    row.remove();
  });
  return { row, fallback };
}

/** A control whose press opens Settings, pressed the way an assistive technology or a script does: it takes no focus. */
function pressToOpen(hook: () => ReturnType<typeof useSettingsDialog>) {
  const control = document.body.appendChild(document.createElement("button"));
  control.addEventListener("click", () => hook().openSettingsAt("accounts", { addAgent: true }));
  onTestFinished(() => control.remove());
  act(() => control.click());
  expect(document.activeElement).not.toBe(control);
  return control;
}

it("hands focus that neither the terminal nor the modal's restore took to the window's fallback once the restore is over", () => {
  const { row, fallback } = registerFallback();
  // No session selected, and nothing pressed: opened from the native menu.
  const hook = mount(false, () => {});
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  act(() => { vi.advanceTimersByTime(20); });
  expect(fallback).not.toHaveBeenCalled();
  settle();
  expect(fallback).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(row);
});

it("leaves alone the control the modal's restore put focus back on when Settings closed with nothing under it", () => {
  const { fallback } = registerFallback();
  const restored = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => restored.remove());
  const hook = mount(false, () => {});
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  restored.focus();
  settle();
  expect(fallback).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(restored);
});

it("gives focus back to the control whose press opened Settings, though the press gave it none", () => {
  const { fallback } = registerFallback();
  const hook = mount(false, () => {});
  const control = pressToOpen(hook);
  act(() => hook().closeSettings());
  expect(document.activeElement).toBe(control);
  settle();
  expect(fallback).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(control);
});

it("hands focus to the window's fallback when the control whose press opened Settings has gone", () => {
  const { row, fallback } = registerFallback();
  const hook = mount(false, () => {});
  pressToOpen(hook).remove();
  act(() => hook().closeSettings());
  settle();
  expect(fallback).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(row);
});

it("does not take a press that came before Settings was asked for as its opener", () => {
  const { row } = registerFallback();
  const hook = mount(false, () => {});
  const earlier = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => earlier.remove());
  act(() => earlier.click());
  // The press is over: Settings is then asked for with no press, as from the native menu.
  act(() => { vi.advanceTimersByTime(20); });
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  settle();
  expect(document.activeElement).toBe(row);
});

it("leaves focus on the selected session's terminal, whatever control opened Settings", () => {
  registerFallback();
  const terminal = document.body.appendChild(document.createElement("textarea"));
  onTestFinished(() => terminal.remove());
  const hook = mount(false, () => terminal.focus());
  pressToOpen(hook);
  act(() => hook().closeSettings());
  settle();
  expect(document.activeElement).toBe(terminal);
});

it("hands focus back to a dialog open under Settings, a frame after the modal's own restore, when that left it on the body", async () => {
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  onTestFinished(() => dialog.remove());
  const focusTerminal = vi.fn();
  const hook = mount(true, focusTerminal);
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  expect(document.activeElement).toBe(document.body);
  await frame();
  expect(focusTerminal).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(dialog);
});

it("leaves alone a control the modal's restore has put focus on inside the dialog under Settings", async () => {
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  const opener = dialog.appendChild(document.createElement("button"));
  onTestFinished(() => dialog.remove());
  const hook = mount(true, () => {});
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  opener.focus();
  await frame();
  expect(document.activeElement).toBe(opener);
});

it("numbers each request, so an open dialog can follow a later one", () => {
  const hook = mount(false, () => {});
  act(() => hook().openSettingsAt("accounts", { addAgent: true }));
  const first = hook().settingsNavigation;
  act(() => hook().openSettingsAt("accounts"));
  expect(hook().settingsNavigation.nonce).toBe(first.nonce + 1);
  expect(first).toMatchObject({ section: "accounts", addAgent: true });
  expect(hook().settingsNavigation).toMatchObject({ section: "accounts", addAgent: false });
});

function mountWithMenu(options: { otherModalOpen?: boolean; ready?: boolean }) {
  let about: () => void = () => {};
  const menu = { onSettingsRequested: () => () => {}, onAboutRequested: (handler: () => void) => { about = handler; return () => {}; }, setLabels: async () => {} };
  let hook!: ReturnType<typeof useSettingsDialog>;
  function Probe() {
    hook = useSettingsDialog({ ready: options.ready ?? true, otherModalOpen: options.otherModalOpen ?? false, focusTerminal: () => {} });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "tauri", appMenu: menu }}><Probe /></PlatformProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { hook: () => hook, about: () => act(() => about()) };
}

it("opens Settings on About from the native menu's About item, and moves an open Settings there", () => {
  const { hook, about } = mountWithMenu({});
  about();
  expect(hook()).toMatchObject({ settingsOpen: true, settingsNavigation: { section: "about" } });
  act(() => hook().openSettingsAt("git"));
  about();
  expect(hook().settingsNavigation.section).toBe("about");
});

it("ignores the menu's About item before the UI is ready and while another modal is open", () => {
  for (const options of [{ ready: false }, { otherModalOpen: true }]) {
    const { hook, about } = mountWithMenu(options);
    about();
    expect(hook().settingsOpen).toBe(false);
  }
});
