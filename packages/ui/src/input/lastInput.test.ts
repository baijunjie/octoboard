// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, expect, it, vi } from "vitest";

import { interactionIsTouch, ringForAutomaticFocus } from "./lastInput";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const press = (pointerType: string) => document.dispatchEvent(new PointerEvent("pointerdown", { pointerType, bubbles: true }));

// The module has seen no interaction yet only until one is dispatched, and the other tests dispatch them, so this test
// loads a fresh instance of the module with the react-aria state it writes to (one graph, so the modality it reads is
// the one the module sets).
it("takes the device's lack of hover before any interaction for a tap, then each interaction's own input", async () => {
  vi.resetModules();
  const fresh = await import("./lastInput");
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: query === "(hover: none)" }));
  expect(fresh.interactionIsTouch()).toBe(true);
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  expect(fresh.interactionIsTouch()).toBe(false);
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: query === "(hover: none)" }));
  press("mouse");
  expect(fresh.interactionIsTouch()).toBe(false);
  press("touch");
  expect(fresh.interactionIsTouch()).toBe(true);
  // A hardware keyboard on a touch device keeps the desktop's behaviour.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(fresh.interactionIsTouch()).toBe(false);
});

// A ring is shown only once a keyboard is known to be in use: before any input there is none, on a device that can hover
// and on one that cannot alike. The module is loaded fresh, so no input has been seen.
it.each([
  ["a device that can hover", false],
  ["a device that cannot hover", true],
])("settles the modality at load to the pointer's, and shows no ring for a focus move before any input, on %s", async (_, cannotHover) => {
  vi.resetModules();
  const aria = await import("react-aria");
  const state = await import("react-aria/private/interactions/useFocusVisible");
  const before = state.getInteractionModality();
  // Nothing has been settled in this graph yet: its react-aria has had no input.
  act(() => Reflect.apply(aria.setInteractionModality, undefined, [null]));
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: cannotHover && query === "(hover: none)" }));
  const fresh = await import("./lastInput");
  expect(state.getInteractionModality()).toBe("pointer");
  fresh.ringForAutomaticFocus();
  expect(state.getInteractionModality()).toBe("pointer");
  act(() => Reflect.apply(aria.setInteractionModality, undefined, [before]));
});

it.each([
  ["a hardware key", () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })), "keyboard"],
  ["a mouse press", () => press("mouse"), "pointer"],
  ["a touch press", () => press("touch"), "pointer"],
  ["a pen press", () => press("pen"), "pointer"],
])("follows the last input for the focus ring of a focus move the user did not ask for: %s", (_, input, modality) => {
  const before = getInteractionModality();
  input();
  act(() => setInteractionModality("pointer"));
  ringForAutomaticFocus();
  expect(getInteractionModality()).toBe(modality);
  act(() => Reflect.apply(setInteractionModality, undefined, [before]));
});

it("keeps the previous input through a virtual press, which a screen reader or a script sends with no pointer behind it", () => {
  press("mouse");
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch", width: 0, height: 0, bubbles: true }));
  expect(interactionIsTouch()).toBe(false);
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "", bubbles: true }));
  expect(interactionIsTouch()).toBe(false);
});

it("counts a zero-sized press as virtual only off Android, and reads TalkBack's size-1 press as virtual on it", () => {
  const userAgent = vi.spyOn(navigator, "userAgent", "get");
  const sized = (init: PointerEventInit) => document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, ...init }));
  userAgent.mockReturnValue("Mozilla/5.0 (Macintosh)");
  press("mouse");
  // Off Android a size of 1 is a real pointer's, whatever its pressure.
  sized({ pointerType: "touch", width: 1, height: 1, pressure: 0 });
  expect(interactionIsTouch()).toBe(true);
  userAgent.mockReturnValue("Mozilla/5.0 (Linux; Android 14)");
  press("mouse");
  // On Android a zero size is no screen reader's mark, and only TalkBack's exact signature is virtual.
  sized({ pointerType: "touch", width: 0, height: 0 });
  expect(interactionIsTouch()).toBe(true);
  press("mouse");
  sized({ pointerType: "touch", width: 1, height: 1, pressure: 0 });
  expect(interactionIsTouch()).toBe(true);
  sized({ pointerType: "mouse", width: 1, height: 1, pressure: 0, detail: 1 });
  expect(interactionIsTouch()).toBe(false);
  sized({ pointerType: "touch", width: 4, height: 4 });
  expect(interactionIsTouch()).toBe(true);
  sized({ pointerType: "mouse", width: 1, height: 1, pressure: 0, detail: 0 });
  expect(interactionIsTouch()).toBe(true);
  sized({ pointerType: "", width: 0, height: 0 });
  expect(interactionIsTouch()).toBe(true);
});

it("sees a key a handler on window's capture phase stops, so a keyboard close still shows the ring", () => {
  const stop = (event: Event) => event.stopPropagation();
  const before = getInteractionModality();
  press("mouse");
  window.addEventListener("keydown", stop, true);
  try {
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  } finally {
    window.removeEventListener("keydown", stop, true);
  }
  act(() => setInteractionModality("pointer"));
  ringForAutomaticFocus();
  expect(getInteractionModality()).toBe("keyboard");
  act(() => Reflect.apply(setInteractionModality, undefined, [before]));
});

it("keeps a hardware key's ring and a mouse's no-ring through a virtual press", () => {
  const before = getInteractionModality();
  const ringed = () => {
    act(() => setInteractionModality("pointer"));
    ringForAutomaticFocus();
    return getInteractionModality() === "keyboard";
  };
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", width: 0, height: 0, bubbles: true }));
  expect(ringed()).toBe(true);
  press("mouse");
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "", bubbles: true }));
  expect(ringed()).toBe(false);
  act(() => Reflect.apply(setInteractionModality, undefined, [before]));
});

it("counts a pen's press as a tap", () => {
  press("mouse");
  press("pen");
  expect(interactionIsTouch()).toBe(true);
});

it("keeps a tap's behaviour while an on-screen keyboard types into a field, and takes a hardware keyboard's navigation or shortcut", () => {
  const field = document.body.appendChild(document.createElement("textarea"));
  const button = document.body.appendChild(document.createElement("button"));
  const key = (target: Element, init: KeyboardEventInit) => target.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, ...init }));
  try {
    press("touch");
    key(field, { key: "a" });
    key(field, { key: "Enter" });
    key(field, { key: "Unidentified" });
    key(field, { key: "k", isComposing: true });
    key(button, { key: "Unidentified" });
    expect(interactionIsTouch()).toBe(true);
    key(field, { key: "Tab" });
    expect(interactionIsTouch()).toBe(false);
    press("touch");
    key(field, { key: "v", metaKey: true });
    expect(interactionIsTouch()).toBe(false);
    press("touch");
    key(button, { key: "Enter" });
    expect(interactionIsTouch()).toBe(false);
  } finally {
    field.remove();
    button.remove();
  }
});
