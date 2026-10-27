// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import type { SwipeToward } from "../layout/paneSwipe";
import { useSwipe } from "../layout/useSwipe";
import { createUi } from "./interact";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The window's swipe gesture over a page jsdom gives no layout: the terminal is whatever sits under the finger. */
function mount(rtl: boolean) {
  document.documentElement.dir = rtl ? "rtl" : "ltr";
  const swipes: SwipeToward[] = [];
  function Probe() {
    useSwipe(true, rtl, (toward) => swipes.push(toward));
    return null;
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  act(() => root.render(<Probe />));
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => host });
  onTestFinished(() => {
    act(() => root.unmount());
    host.remove();
    Reflect.deleteProperty(document, "elementFromPoint");
    document.documentElement.removeAttribute("dir");
  });
  return swipes;
}

it("swipe sends the gesture towards the edge it names", async () => {
  const swipes = mount(false);
  const ui = createUi(document, "en");
  await ui.swipe("end");
  await ui.swipe("start");
  expect(swipes).toEqual(["end", "start"]);
});

it("swipe names the edge in the reading direction, so the finger goes the other way in right-to-left text", async () => {
  const swipes = mount(true);
  const moves: number[] = [];
  const record = (event: Event) => moves.push((event as unknown as { changedTouches: { clientX: number }[] }).changedTouches[0]!.clientX);
  window.addEventListener("touchmove", record, true);
  onTestFinished(() => window.removeEventListener("touchmove", record, true));
  await createUi(document, "ar").swipe("end");
  expect(swipes).toEqual(["end"]);
  expect(moves[moves.length - 1]!).toBeLessThan(moves[0]!);
});
