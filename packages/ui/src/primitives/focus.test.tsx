// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { setPointerModality, settleUnknownModality, useArrivalFocusProps } from "./focus";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount() {
  function Probe() {
    return <div data-probe {...useArrivalFocusProps()}><textarea /><button>inside</button></div>;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const outside = document.body.appendChild(document.createElement("button"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    outside.remove();
    setInteractionModality("pointer");
  });
  act(() => root.render(<Probe />));
  const surface = container.querySelector<HTMLElement>("[data-probe]")!;
  return { surface, input: container.querySelector("textarea")!, inner: container.querySelector("button")!, outside, ring: () => surface.hasAttribute("data-focus-visible") };
}

function key(target: Element, name: string) {
  act(() => {
    target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
    target.dispatchEvent(new KeyboardEvent("keyup", { key: name, bubbles: true }));
  });
}

it("shows focus that arrives by keyboard, keeps it while focus moves inside, and drops it when focus leaves", () => {
  const { input, inner, outside, ring } = mount();
  act(() => { setInteractionModality("keyboard"); input.focus(); });
  expect(ring()).toBe(true);
  act(() => inner.focus());
  expect(ring()).toBe(true);
  act(() => outside.focus());
  expect(ring()).toBe(false);
});

it("does not show focus that arrived by pointer when Escape, Tab or a letter is typed inside", () => {
  const { input, ring } = mount();
  act(() => { setInteractionModality("pointer"); input.focus(); });
  expect(ring()).toBe(false);
  for (const name of ["Escape", "Tab", "a"]) {
    key(input, name);
    expect(ring()).toBe(false);
  }
});

it("drops a keyboard arrival's ring at a pointer press inside, and does not bring it back with a key", () => {
  const { input, ring } = mount();
  act(() => { setInteractionModality("keyboard"); input.focus(); });
  expect(ring()).toBe(true);
  act(() => setInteractionModality("pointer"));
  expect(ring()).toBe(false);
  key(input, "Escape");
  expect(ring()).toBe(false);
});

it("keeps the decision made on arrival when focus leaves and comes back with the window", () => {
  const { input, ring } = mount();
  act(() => { setInteractionModality("pointer"); input.focus(); });
  key(input, "Escape");
  // The window deactivates and comes back: focus events with no related target, the element still active throughout.
  act(() => {
    input.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: null }));
    input.dispatchEvent(new FocusEvent("focusin", { bubbles: true, relatedTarget: null }));
  });
  expect(ring()).toBe(false);
});

it("decides again when focus is handed to another element inside, as a cover hands it back to the input", () => {
  const { input, inner, ring } = mount();
  act(() => { setInteractionModality("pointer"); inner.focus(); });
  expect(ring()).toBe(false);
  // An automatic handoff inside the container: the keyboard modality set, then focus moved (the cover removed first).
  act(() => { setInteractionModality("keyboard"); input.focus(); });
  expect(ring()).toBe(true);
});

it("takes the input modality back to the pointer's from the keyboard's", () => {
  onTestFinished(() => void act(() => setInteractionModality("pointer")));
  act(() => setInteractionModality("keyboard"));
  expect(getInteractionModality()).toBe("keyboard");
  act(() => setPointerModality());
  expect(getInteractionModality()).toBe("pointer");
});

type Modality = "keyboard" | "pointer" | "virtual" | null;
const setModality = (modality: Modality) =>
  act(() => void Reflect.apply(setInteractionModality, undefined, [modality]));

it.each([
  { from: null, settled: "pointer" },
  { from: "virtual", settled: "virtual" },
  { from: "keyboard", settled: "keyboard" },
  { from: "pointer", settled: "pointer" },
] as const)("settles $from to $settled, leaving every modality but the unknown one as it is", ({ from, settled }) => {
  onTestFinished(() => setModality("pointer"));
  setModality(from);
  act(() => settleUnknownModality());
  expect(getInteractionModality()).toBe(settled);
});

it("registers no listeners, so a modality react-aria sets later is never touched", () => {
  const onWindow = vi.spyOn(window, "addEventListener");
  const onDocument = vi.spyOn(document, "addEventListener");
  onTestFinished(() => {
    onWindow.mockRestore();
    onDocument.mockRestore();
    setModality("pointer");
  });
  setModality(null);
  act(() => settleUnknownModality());
  expect(onWindow).not.toHaveBeenCalled();
  expect(onDocument).not.toHaveBeenCalled();
  // Focus with no input before it, and a click with no pointer, leave react-aria's virtual modality as it is.
  const button = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => button.remove());
  setModality("virtual");
  act(() => button.focus());
  act(() => button.click());
  expect(getInteractionModality()).toBe("virtual");
});
