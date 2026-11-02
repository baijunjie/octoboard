// @vitest-environment jsdom
import { act, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { Button } from "react-aria-components";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished } from "vitest";

import { HandOffOnUnmount, useHandOffAfterRemoval } from "./HandOffOnUnmount";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A pane holding focus that goes away, handing focus to a control outside it. */
function handOff(touch: boolean) {
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: touch ? "touch" : "mouse", bubbles: true }));
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
          <HandOffOnUnmount from="#pane" to={() => document.getElementById("target")!.focus()} />
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

it("hands focus on with the keyboard's focus ring", () => {
  expect(handOff(false)).toBe("target");
  expect(getInteractionModality()).toBe("keyboard");
});

it("hands focus on without a focus ring after a tap", () => {
  expect(handOff(true)).toBe("target");
  expect(getInteractionModality()).toBe("pointer");
});

/** A component rendering a pane that holds focus, which hands it to a react-aria button beside it as it goes. */
function HandOffHost({ shown, disabled, restore }: { shown: boolean; disabled: boolean; restore: boolean }) {
  const handOffFrom = useHandOffAfterRemoval(() => document.getElementById("target"));
  return (
    <>
      <Button id="target" isDisabled={disabled}>target</Button>
      <button type="button" id="restored">restored</button>
      {shown ? (
        <>
          {handOffFrom("#pane")}
          <div id="pane"><button type="button">inside</button></div>
        </>
      ) : restore && <Restore />}
    </>
  );
}

/** Focus placed elsewhere in the same commit, by a layout effect of a descendant of the hand-off's host. */
function Restore() {
  useLayoutEffect(() => document.getElementById("restored")!.focus(), []);
  return null;
}

it.each([
  ["moves focus, with the ring, when focus fell to <body>", { mounted: true }, "target", "keyboard"],
  ["moves nothing, and leaves the modality, when its host goes with the source", { mounted: false }, undefined, "pointer"],
  ["leaves focus placed meanwhile, and the modality, alone", { mounted: true, restore: true }, "restored", "pointer"],
  ["leaves a target disabled by the same render alone", { mounted: true, disabled: true }, undefined, "pointer"],
] as const)("the hand-off after the removal %s", (_, after: { mounted: boolean; restore?: boolean; disabled?: boolean }, focused, modality) => {
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  const before = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [before]));
  });
  act(() => root.render(<HandOffHost shown disabled={false} restore={false} />));
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  act(() => root.render(after.mounted ? <HandOffHost shown={false} disabled={after.disabled ?? false} restore={after.restore ?? false} /> : null));
  expect(document.activeElement).toBe(focused ? document.getElementById(focused) : document.body);
  expect(getInteractionModality()).toBe(modality);
  if (focused === "target") expect(document.getElementById("target")!.hasAttribute("data-focus-visible")).toBe(true);
});

it("lets a react-aria control taking focus in the commit show its focus ring", async () => {
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
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
          <HandOffOnUnmount from="#pane" to={() => document.getElementById("target")!.focus()} />
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
