// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { PlatformProvider } from "../platform/react";
import { findInFilesLauncher } from "../project-browser/files/contentSearch/findInFilesLauncher";
import { defaultBinding } from "../shortcuts/commands";
import { eventKey } from "../shortcuts/binding";
import { restoreAllBindings, setBinding } from "../shortcuts/store";
import { BROWSER_HOST } from "../test/hosts";
import { useFindInFilesShortcut } from "./useFindInFilesShortcut";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let container: HTMLElement;

function Shortcut() {
  useFindInFilesShortcut();
  return null;
}

beforeEach(() => {
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}><Shortcut /></PlatformProvider>));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  restoreAllBindings();
});

/** Registers a browser on screen, which the shortcut opens Find in files on. */
function browserOnScreen() {
  const open = vi.fn();
  const unregister = findInFilesLauncher.register(open);
  return { open, unregister };
}

const press = (init: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...eventKey(defaultBinding("findInFiles"), BROWSER_HOST), ...init });
  act(() => void window.dispatchEvent(event));
  return event;
};

it("opens Find in files on the project browser on screen from its keys, and takes the keys from the page", () => {
  const { open, unregister } = browserOnScreen();
  expect(press().defaultPrevented).toBe(true);
  expect(open).toHaveBeenCalledExactlyOnceWith();
  unregister();
});

it("opens nothing again for a held key's repeats, though it still takes them", () => {
  const { open, unregister } = browserOnScreen();
  press();
  const repeat = press({ repeat: true });
  expect(repeat.defaultPrevented).toBe(true);
  expect(open).toHaveBeenCalledTimes(1);
  unregister();
});

it("leaves other keys alone, and does nothing while no project browser is on screen", () => {
  const { open, unregister } = browserOnScreen();
  const other = press({ ...eventKey({ code: "KeyG", primary: true, shift: true }, BROWSER_HOST) });
  expect(other.defaultPrevented).toBe(false);
  expect(open).not.toHaveBeenCalled();
  unregister();
  expect(() => press()).not.toThrow();
  expect(open).not.toHaveBeenCalled();
});

it("follows the shortcut the user rebound it to", () => {
  const { open, unregister } = browserOnScreen();
  act(() => void setBinding("findInFiles", { code: "KeyK", primary: true, alt: true }, BROWSER_HOST));
  press();
  expect(open).not.toHaveBeenCalled();
  press({ ...eventKey({ code: "KeyK", primary: true, alt: true }, BROWSER_HOST) });
  expect(open).toHaveBeenCalledTimes(1);
  unregister();
});
