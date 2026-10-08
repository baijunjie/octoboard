// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../gallery/fixtures/builders";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { Session } from "../protocol";
import type { FocusTarget } from "./types";
import { useSwitchShortcut } from "./switchShortcut";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-1", "Main");
const hubs = ["a", "b", "c"].map((id) => sessionOf(`s-${id}`, main.id, undefined, id, "idle", { colour: "teal" }));
const desktop: PlatformAdapter = { kind: "tauri", windowChrome: { leftInset: () => 0, subscribe: () => () => {} } };

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
  let move!: (backward: boolean) => void;
  function Host() {
    move = useSwitchShortcut({ focus, strip, shown, onSwitch });
    return null;
  }
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(<PlatformProvider value={platform}><Host /></PlatformProvider>));
  return { onSwitch, move: (backward: boolean) => move(backward) };
}

const press = (init: KeyboardEventInit) => {
  const event = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true, ...init });
  act(() => void document.body.dispatchEvent(event));
  return event;
};

describe("useSwitchShortcut", () => {
  it("moves to the next console session on Ctrl+Tab and the previous on Ctrl+Shift+Tab, going round", () => {
    const { onSwitch } = mount();
    expect(press({ ctrlKey: true }).defaultPrevented).toBe(true);
    expect(onSwitch).toHaveBeenLastCalledWith(hubs[1]);
    press({ ctrlKey: true, shiftKey: true });
    expect(onSwitch).toHaveBeenLastCalledWith(hubs[2]);
  });

  it("leaves the key alone where it does not exist", () => {
    for (const options of [
      { platform: { kind: "browser" } as PlatformAdapter },
      { focus: { project: {} } as FocusTarget },
      { strip: hubs.slice(0, 1) },
      { shown: false },
    ]) {
      const { onSwitch } = mount(options);
      expect(press({ ctrlKey: true }).defaultPrevented).toBe(false);
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
      { ctrlKey: true, altKey: true },
      { ctrlKey: true, metaKey: true },
      { ctrlKey: true, isComposing: true },
    ]) {
      expect(press(init).defaultPrevented).toBe(false);
    }
    expect(onSwitch).not.toHaveBeenCalled();
  });

  it("keeps a held key from the terminal but moves only on the first press", () => {
    const { onSwitch } = mount();
    press({ ctrlKey: true });
    expect(press({ ctrlKey: true, repeat: true }).defaultPrevented).toBe(true);
    press({ ctrlKey: true, repeat: true });
    expect(onSwitch).toHaveBeenCalledTimes(1);
  });

  it("does nothing while a dialog is open, though it still takes the key", () => {
    const { onSwitch, move } = mount();
    const dialog = document.body.appendChild(document.createElement("div"));
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    expect(press({ ctrlKey: true }).defaultPrevented).toBe(true);
    move(false);
    expect(onSwitch).not.toHaveBeenCalled();
  });

  it("returns a move that does nothing where the shortcut does not exist", () => {
    for (const options of [
      { platform: { kind: "browser" } as PlatformAdapter },
      { focus: { project: {} } as FocusTarget },
      { strip: hubs.slice(0, 1) },
      { shown: false },
    ]) {
      const { onSwitch, move } = mount(options);
      move(false);
      move(true);
      expect(onSwitch).not.toHaveBeenCalled();
      act(() => root?.unmount());
      container?.remove();
    }
  });

  it("returns the move for a key press that arrives another way", () => {
    const { onSwitch, move } = mount();
    move(true);
    expect(onSwitch).toHaveBeenCalledWith(hubs[2]);
  });
});
