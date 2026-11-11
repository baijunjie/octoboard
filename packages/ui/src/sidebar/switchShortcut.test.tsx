// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../fixtures/builders";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { Session } from "../protocol";
import type { HostFacts } from "../platform/host";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import type { FocusTarget } from "./types";
import { useSwitchShortcut } from "./switchShortcut";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-1", "Main");
const hubs = ["a", "b", "c"].map((id) => sessionOf(`s-${id}`, main.id, undefined, id, "idle", { colour: "teal" }));
const desktop: PlatformAdapter = { kind: "tauri", host: BROWSER_HOST };

let root: Root | undefined;
let container: HTMLElement | undefined;

afterEach(() => {
  act(() => root?.unmount());
  container?.remove();
  document.body.replaceChildren();
  root = container = undefined;
});

function mount({
  platform = desktop,
  focus = { consoleSession: hubs[0] },
  strip = hubs,
  shown = true,
}: { platform?: PlatformAdapter; focus?: FocusTarget; strip?: Session[]; shown?: boolean } = {}) {
  const onSwitch = vi.fn<(session: Session) => void>();
  function Host() {
    useSwitchShortcut({ focus, strip, shown, onSwitch });
    return null;
  }
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(<PlatformProvider value={platform}><Host /></PlatformProvider>));
  return { onSwitch };
}

// The key the shortcut takes: Control, on every keyboard.
const control = { ctrlKey: true };

const press = (init: KeyboardEventInit, target: EventTarget = document.body) => {
  const event = new KeyboardEvent("keydown", { key: "Tab", code: "Tab", bubbles: true, cancelable: true, ...init });
  act(() => void target.dispatchEvent(event));
  return event;
};

describe.each<[string, HostFacts]>([["macOS", MAC_HOST], ["Windows", WINDOWS_HOST]])("useSwitchShortcut in the desktop app on %s", (_name, host) => {
  // The Windows app has no window chrome and no native menu: the switch exists in it all the same.
  const app: PlatformAdapter = { kind: "tauri", host };

  it("moves on Ctrl+Tab and Ctrl+Shift+Tab", () => {
    const { onSwitch } = mount({ platform: app });
    expect(press(control).defaultPrevented).toBe(true);
    expect(onSwitch).toHaveBeenLastCalledWith(hubs[1]);
    press({ ...control, shiftKey: true });
    expect(onSwitch).toHaveBeenLastCalledWith(hubs[2]);
  });

  it("leaves Tab with ⌘ or the Windows key alone", () => {
    const { onSwitch } = mount({ platform: app });
    expect(press({ metaKey: true }).defaultPrevented).toBe(false);
    expect(press({ metaKey: true, shiftKey: true }).defaultPrevented).toBe(false);
    expect(onSwitch).not.toHaveBeenCalled();
  });

  it("takes the key before a focused terminal sees it", () => {
    const { onSwitch } = mount({ platform: app });
    // xterm.js listens on its helper textarea, which holds focus while the terminal does.
    const terminal = document.body.appendChild(document.createElement("textarea"));
    const reached = vi.fn();
    terminal.addEventListener("keydown", reached);
    terminal.focus();
    expect(press(control, terminal).defaultPrevented).toBe(true);
    expect(reached).not.toHaveBeenCalled();
    expect(onSwitch).toHaveBeenCalledWith(hubs[1]);
  });

  it("leaves the key to the page in a browser on the same system, which keeps it for its own tabs", () => {
    const { onSwitch } = mount({ platform: { kind: "browser", host } });
    expect(press(control).defaultPrevented).toBe(false);
    expect(onSwitch).not.toHaveBeenCalled();
  });
});

describe("useSwitchShortcut", () => {
  it("moves to the next console session on Ctrl+Tab and the previous on Ctrl+Shift+Tab, going round", () => {
    const { onSwitch } = mount();
    expect(press({ ...control }).defaultPrevented).toBe(true);
    expect(onSwitch).toHaveBeenLastCalledWith(hubs[1]);
    press({ ...control, shiftKey: true });
    expect(onSwitch).toHaveBeenLastCalledWith(hubs[2]);
  });

  it("leaves the key alone where it does not exist", () => {
    for (const options of [
      { platform: { kind: "browser", host: BROWSER_HOST } as PlatformAdapter },
      { focus: { project: {} } as FocusTarget },
      { strip: hubs.slice(0, 1) },
      { shown: false },
    ]) {
      const { onSwitch } = mount(options);
      expect(press({ ...control }).defaultPrevented).toBe(false);
      expect(onSwitch).not.toHaveBeenCalled();
      act(() => root?.unmount());
      container?.remove();
    }
  });

  it("ignores Tab without Ctrl or with another modifier, and during an IME composition", () => {
    const { onSwitch } = mount();
    for (const init of [
      {},
      { shiftKey: true },
      { ...control, altKey: true },
      { ctrlKey: true, metaKey: true },
      { ...control, isComposing: true },
    ]) {
      expect(press(init).defaultPrevented).toBe(false);
    }
    expect(onSwitch).not.toHaveBeenCalled();
  });

  it("keeps a held key from the terminal but moves only on the first press", () => {
    const { onSwitch } = mount();
    press({ ...control });
    expect(press({ ...control, repeat: true }).defaultPrevented).toBe(true);
    press({ ...control, repeat: true });
    expect(onSwitch).toHaveBeenCalledTimes(1);
  });

  it("does nothing while a dialog is open, though it still takes the key", () => {
    const { onSwitch } = mount();
    const dialog = document.body.appendChild(document.createElement("div"));
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    expect(press({ ...control }).defaultPrevented).toBe(true);
    expect(onSwitch).not.toHaveBeenCalled();
  });
});
