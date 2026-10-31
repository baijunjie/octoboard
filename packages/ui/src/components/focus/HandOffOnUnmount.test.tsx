// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished } from "vitest";

import { HandOffOnUnmount } from "./HandOffOnUnmount";

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
