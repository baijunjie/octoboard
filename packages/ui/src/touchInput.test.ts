// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, expect, it, vi } from "vitest";

import { interactionIsTouch, ringForAutomaticFocus } from "./touchInput";

afterEach(() => vi.unstubAllGlobals());

const press = (pointerType: string) => document.dispatchEvent(new PointerEvent("pointerdown", { pointerType, bubbles: true }));

// Runs first: the module has seen no interaction yet only until a test dispatches one.
it("takes the device's lack of hover before any interaction, then each interaction's own input", () => {
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: query === "(hover: none)" }));
  expect(interactionIsTouch()).toBe(true);
  press("mouse");
  expect(interactionIsTouch()).toBe(false);
  press("touch");
  expect(interactionIsTouch()).toBe(true);
  // A hardware keyboard on a touch device keeps the desktop's behaviour.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(interactionIsTouch()).toBe(false);
});

it.each([
  ["mouse", "keyboard"],
  ["touch", "pointer"],
])("turns the focus ring on for a focus move the user did not ask for, except after a tap (%s)", (pointerType, modality) => {
  const before = getInteractionModality();
  press(pointerType);
  act(() => setInteractionModality("pointer"));
  ringForAutomaticFocus();
  expect(getInteractionModality()).toBe(modality);
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
