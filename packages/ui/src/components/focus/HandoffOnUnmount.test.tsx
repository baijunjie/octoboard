// @vitest-environment jsdom
import { act, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { Button } from "react-aria-components";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished } from "vitest";

import { HandoffOnUnmount, useHandoffAfterRemoval } from "./HandoffOnUnmount";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Sends the last input before a handoff: a press of a pointer, or a hardware keyboard's navigation key. */
function lastInput(input: "key" | "mouse" | "touch") {
  if (input === "key") document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  else document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: input, bubbles: true }));
}

/** A pane holding focus that goes away, handing focus to a control outside it. */
function handoff(input: "key" | "mouse" | "touch") {
  lastInput(input);
  const modality = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
  });
  const render = (shown: boolean) => act(() => root.render(
    <>
      <button type="button" id="target">target</button>
      {shown && (
        <>
          <HandoffOnUnmount from="#pane" to={() => document.getElementById("target")!.focus()} />
          <div id="pane"><button type="button">inside</button></div>
        </>
      )}
    </>,
  ));
  render(true);
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  render(false);
  return document.activeElement?.id;
}

it("hands focus on with the keyboard's focus ring after a key", () => {
  expect(handoff("key")).toBe("target");
  expect(getInteractionModality()).toBe("keyboard");
});

it.each(["mouse", "touch"] as const)("hands focus on without a focus ring after a %s press", (input) => {
  expect(handoff(input)).toBe("target");
  expect(getInteractionModality()).toBe("pointer");
});

/** A component rendering a pane that holds focus, which hands it to a react-aria button beside it as it goes. */
function HandoffHost({ shown, disabled, restore }: { shown: boolean; disabled: boolean; restore: boolean }) {
  const handoffFrom = useHandoffAfterRemoval(() => document.getElementById("target"));
  return (
    <>
      <Button id="target" isDisabled={disabled}>target</Button>
      <button type="button" id="restored">restored</button>
      {shown ? (
        <>
          {handoffFrom("#pane")}
          <div id="pane"><button type="button">inside</button></div>
        </>
      ) : restore && <Restore />}
    </>
  );
}

/** Focus placed elsewhere in the same commit, by a layout effect of a descendant of the handoff's host. */
function Restore() {
  useLayoutEffect(() => document.getElementById("restored")!.focus(), []);
  return null;
}

it.each([
  ["moves focus, with the ring, when focus fell to <body> after a key", { mounted: true }, "target", "keyboard", "key"],
  ["moves focus, without a ring, when focus fell to <body> after a mouse press", { mounted: true }, "target", "pointer", "mouse"],
  ["moves nothing, and leaves the modality, when its host goes with the source", { mounted: false }, undefined, "pointer", "mouse"],
  ["leaves focus placed meanwhile, and the modality, alone", { mounted: true, restore: true }, "restored", "pointer", "mouse"],
  ["leaves a target disabled by the same render alone", { mounted: true, disabled: true }, undefined, "pointer", "mouse"],
] as const)("the handoff after the removal %s", (_, after: { mounted: boolean; restore?: boolean; disabled?: boolean }, focused, modality, input) => {
  lastInput(input);
  const before = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [before]));
  });
  act(() => root.render(<HandoffHost shown disabled={false} restore={false} />));
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  act(() => root.render(after.mounted ? <HandoffHost shown={false} disabled={after.disabled ?? false} restore={after.restore ?? false} /> : null));
  expect(document.activeElement).toBe(focused ? document.getElementById(focused) : document.body);
  expect(getInteractionModality()).toBe(modality);
  if (focused === "target") expect(document.getElementById("target")!.hasAttribute("data-focus-visible")).toBe(modality === "keyboard");
});

it("lets a react-aria control taking focus in the commit show its focus ring after a key", async () => {
  lastInput("key");
  const before = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [before]));
  });
  const render = (shown: boolean) => act(() => root.render(
    <>
      <Button id="target">target</Button>
      {shown && (
        <>
          <HandoffOnUnmount from="#pane" to={() => document.getElementById("target")!.focus()} />
          <div id="pane"><button type="button">inside</button></div>
        </>
      )}
    </>,
  ));
  render(true);
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  render(false);
  const target = document.getElementById("target")!;
  expect(document.activeElement).toBe(target);
  await act(async () => {});
  expect(target.hasAttribute("data-focused")).toBe(true);
  expect(target.hasAttribute("data-focus-visible")).toBe(true);
});
