// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers, settleFrames, withModality } from "../components/dialog/dialogTestSupport";
import { MODAL_CLOSE_SETTLE_MS, setFocusAfterModalLoss } from "../components/focus/modalFocus";
import { browserHost } from "../platform/host";
import { PlatformProvider } from "../platform/react";
import type { PlatformAdapter, UpdaterCapability } from "../platform";
import { UpdatesProvider } from "../updates/UpdatesProvider";
import { useSettingsDialog } from "./useSettingsDialog";
import { bindingFor, restoreAllBindings, setBinding } from "../shortcuts/store";
import { eventKey, type Binding } from "../shortcuts/binding";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(otherModalOpen: boolean, focusTerminal: () => void, platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST }) {
  let hook!: ReturnType<typeof useSettingsDialog>;
  function Probe() {
    hook = useSettingsDialog({ ready: true, otherModalOpen, focusTerminal });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={platform}><Probe /></PlatformProvider>));
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

/** Runs the timers a closing modal's exit transition and React Aria's focus restore are given, after which the window's
 * fallback for focus left on the body runs. Needs `vi.useFakeTimers()`. */
const settle = () => act(() => { vi.runOnlyPendingTimers(); });

/** Registers, as the window does, where focus left on the body goes: here a stand-in for the sidebar's selected row. */
function registerFallback() {
  fakeTimers();
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
function pressToOpen(hook: () => ReturnType<typeof useSettingsDialog>, parent: HTMLElement = document.body) {
  const control = parent.appendChild(document.createElement("button"));
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
  act(() => { vi.advanceTimersByTime(MODAL_CLOSE_SETTLE_MS - 1); });
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
  act(() => { vi.runOnlyPendingTimers(); });
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
  framesAsTimers("pointer");
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  onTestFinished(() => dialog.remove());
  const focusTerminal = vi.fn();
  const hook = mount(true, focusTerminal);
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  expect(document.activeElement).toBe(document.body);
  await settleFrames();
  expect(focusTerminal).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(dialog);
});

it("hands focus to the control a dialog under Settings opens on, rather than the dialog, when the restore left it on the body", async () => {
  framesAsTimers("pointer");
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  const field = dialog.appendChild(document.createElement("input"));
  field.setAttribute("data-opening-focus", "");
  onTestFinished(() => dialog.remove());
  const hook = mount(true, () => {});
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  await settleFrames();
  expect(document.activeElement).toBe(field);
});

it("hands focus back to the control in the dialog under Settings whose press opened Settings, though the press gave it none", async () => {
  framesAsTimers("pointer");
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  const field = dialog.appendChild(document.createElement("input"));
  field.setAttribute("data-opening-focus", "");
  onTestFinished(() => dialog.remove());
  const hook = mount(true, () => {});
  const control = pressToOpen(hook, dialog);
  act(() => hook().closeSettings());
  await settleFrames();
  expect(document.activeElement).toBe(control);
});

it("leaves alone a control the modal's restore has put focus on inside the dialog under Settings", async () => {
  framesAsTimers("pointer");
  const dialog = document.body.appendChild(document.createElement("div"));
  dialog.setAttribute("role", "dialog");
  dialog.tabIndex = -1;
  const opener = dialog.appendChild(document.createElement("button"));
  onTestFinished(() => dialog.remove());
  const hook = mount(true, () => {});
  act(() => hook().openSettingsAt("accounts"));
  act(() => hook().closeSettings());
  opener.focus();
  await settleFrames();
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

function mountWithMenu(options: { otherModalOpen?: boolean; ready?: boolean; updater?: UpdaterCapability }) {
  let requestUpdates: () => void = () => {};
  const statusItem = { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {}, onUpdatesRequested: (handler: () => void) => { requestUpdates = handler; return () => {}; } };
  let about: () => void = () => {};
  const menu = { onSettingsRequested: () => () => {}, onAboutRequested: (handler: () => void) => { about = handler; return () => {}; }, setLabels: async () => {} };
  let hook!: ReturnType<typeof useSettingsDialog>;
  function Probe() {
    hook = useSettingsDialog({ ready: options.ready ?? true, otherModalOpen: options.otherModalOpen ?? false, focusTerminal: () => {} });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "tauri", host: BROWSER_HOST, appMenu: menu, statusItem, updater: options.updater }}><UpdatesProvider firstCheckDelayMs={null}><Probe /></UpdatesProvider></PlatformProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { hook: () => hook, about: () => act(() => about()), updates: () => act(async () => requestUpdates()) };
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


it("opens update settings and checks manually from the tray, keeping an already offered update", async () => {
  const check = vi.fn(async () => ({ version: "99.0.0" }));
  const { hook, about, updates } = mountWithMenu({ updater: { currentVersion: async () => "1.0.0", check, install: async () => {} } });
  about();
  expect(check).not.toHaveBeenCalled();
  act(() => hook().openSettingsAt("git"));
  await updates();
  expect(hook()).toMatchObject({ settingsOpen: true, settingsNavigation: { section: "updates" } });
  expect(check).toHaveBeenCalledOnce();
  act(() => hook().closeSettings());
  await updates();
  expect(hook().settingsOpen).toBe(true);
  expect(check).toHaveBeenCalledOnce();
});

it("ignores the tray update request before readiness or behind another modal", async () => {
  for (const options of [{ ready: false }, { otherModalOpen: true }]) {
    const check = vi.fn(async () => undefined);
    const { hook, updates } = mountWithMenu({ ...options, updater: { currentVersion: async () => "1.0.0", check, install: async () => {} } });
    await updates();
    expect(hook().settingsOpen).toBe(false);
    expect(check).not.toHaveBeenCalled();
  }
});

describe("the Settings shortcut where there is no native menu", () => {
  const WINDOWS: PlatformAdapter = { kind: "tauri", host: WINDOWS_HOST };
  const MAC: PlatformAdapter = { kind: "tauri", host: MAC_HOST, appMenu: { onSettingsRequested: () => () => {}, setLabels: async () => {} } };
  const comma: Binding = { code: "Comma", primary: true };

  afterEach(() => {
    restoreAllBindings();
    localStorage.clear();
  });

  function press(binding: Binding, host: PlatformAdapter["host"], target: EventTarget = document.body) {
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...eventKey(binding, host) });
    act(() => { target.dispatchEvent(event); });
    return event;
  }

  it("opens Settings on the Windows app, and on a browser, with the user's current binding", () => {
    // A browser on a Mac takes ⌘, and has no native menu to leave it to.
    const macBrowser: PlatformAdapter = { kind: "browser", host: browserHost("MacIntel") };
    for (const platform of [WINDOWS, { kind: "browser", host: BROWSER_HOST } as PlatformAdapter, macBrowser]) {
      const hook = mount(false, () => {}, platform);
      const event = press(comma, platform.host);
      expect(event.defaultPrevented, platform.host.os).toBe(true);
      expect(hook().settingsOpen, platform.host.os).toBe(true);
      act(() => hook().closeSettings());
    }
    expect(eventKey(comma, macBrowser.host)).toMatchObject({ metaKey: true, ctrlKey: false });
    const hook = mount(false, () => {}, WINDOWS);
    expect(setBinding("openSettings", { code: "KeyK", primary: true, shift: true }, WINDOWS_HOST)).toBeUndefined();
    press(comma, WINDOWS_HOST);
    expect(hook().settingsOpen).toBe(false);
    press(bindingFor("openSettings"), WINDOWS_HOST);
    expect(hook().settingsOpen).toBe(true);
  });

  it("shows the focus ring on what Settings focuses once the key opens it, and not when it does nothing", () => {
    withModality("pointer");
    const blocked = mount(true, () => {}, WINDOWS);
    press(comma, WINDOWS_HOST);
    expect(blocked().settingsOpen).toBe(false);
    expect(getInteractionModality()).toBe("pointer");
    const hook = mount(false, () => {}, WINDOWS);
    press(comma, WINDOWS_HOST);
    expect(hook().settingsOpen).toBe(true);
    expect(getInteractionModality()).toBe("keyboard");
  });

  it("leaves the key to the native menu on the macOS app, so nothing fires twice", () => {
    const hook = mount(false, () => {}, MAC);
    const event = press(comma, MAC_HOST);
    expect(event.defaultPrevented).toBe(false);
    expect(hook().settingsOpen).toBe(false);
  });

  it("takes the key from the terminal and leaves its other keys to it", () => {
    const hook = mount(false, () => {}, WINDOWS);
    const terminal = document.body.appendChild(document.createElement("div"));
    terminal.className = "xterm";
    const input = terminal.appendChild(document.createElement("textarea"));
    onTestFinished(() => terminal.remove());
    input.focus();
    const reached = vi.fn();
    terminal.addEventListener("keydown", reached);
    expect(press({ code: "KeyC", primary: true }, WINDOWS_HOST, input).defaultPrevented).toBe(false);
    expect(reached).toHaveBeenCalledOnce();
    expect(press(comma, WINDOWS_HOST, input).defaultPrevented).toBe(true);
    expect(reached).toHaveBeenCalledOnce();
    expect(hook().settingsOpen).toBe(true);
  });

  it("is ignored while another modal holds the user's attention, and still consumes the key", () => {
    const hook = mount(true, () => {}, WINDOWS);
    const event = press(comma, WINDOWS_HOST);
    expect(event.defaultPrevented).toBe(true);
    expect(hook().settingsOpen).toBe(false);
  });
});
