// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import type { Agent } from "../protocol";
import { beginTabDrag } from "./tabDrag";

const agents = ["claude", "codex"] as unknown as Agent[];

function setup(reduced: boolean, onDrop: (order: Agent[], moved: Agent) => void = () => {}, { dragged = 0, rtl = false } = {}) {
  const preference = new EventTarget() as MediaQueryList;
  Object.defineProperty(preference, "matches", { value: reduced, writable: true });
  vi.stubGlobal("matchMedia", () => preference);
  const elements = new Map<Agent, HTMLElement>();
  agents.forEach((agent, index) => {
    const tab = document.body.appendChild(document.createElement("button"));
    // Right to left, the first tab is at the right.
    const left = rtl ? (agents.length - 1 - index) * 110 : index * 110;
    if (rtl) tab.style.direction = "rtl";
    tab.getBoundingClientRect = () => ({ left, right: left + 100, width: 100, top: 0, bottom: 30, height: 30, x: left, y: 0, toJSON() {} });
    elements.set(agent, tab);
  });
  onTestFinished(() => {
    elements.forEach((tab) => tab.remove());
    vi.unstubAllGlobals();
  });
  const press = (clientX: number) => beginTabDrag({
    agent: agents[dragged]!,
    agents,
    elements,
    event: new PointerEvent("pointerdown", { clientX, button: 0, pointerType: "mouse" }),
    onDrop,
  });
  const move = (clientX: number) => document.dispatchEvent(new PointerEvent("pointermove", { clientX, bubbles: true }));
  const release = () => document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  const source = () => elements.get(agents[dragged]!)!;
  const other = () => elements.get(agents[1 - dragged]!)!;
  return { preference, press, move, release, source, other };
}

it("eases the tabs a drag passes, and stops easing them when reduced motion is requested mid-drag", () => {
  const { preference, press, move, release, other } = setup(false);
  press(10);
  move(80);
  expect(other().style.transition).toBe("transform 150ms");
  Object.defineProperty(preference, "matches", { value: true });
  preference.dispatchEvent(new Event("change"));
  expect(other().style.transition).toBe("");
  release();
});

it("reads the preference when the drag starts rather than when the tab is pressed", () => {
  const { preference, press, move, release, other } = setup(true);
  press(10);
  Object.defineProperty(preference, "matches", { value: false });
  preference.dispatchEvent(new Event("change"));
  move(80);
  expect(other().style.transition).toBe("transform 150ms");
  release();
});

it("reports a drop while the tabs still stand where the drag left them, and clears them after", () => {
  const shown: string[] = [];
  const { press, move, release, other } = setup(false, (order, moved) => {
    expect(order).toEqual(["codex", "claude"]);
    expect(moved).toBe("claude");
    shown.push(other().style.transform);
  });
  press(10);
  move(130);
  release();
  expect(shown).toEqual(["translateX(-110px)"]);
  expect(other().style.transform).toBe("");
});

// The tabs are 100px wide, 110px apart, so the other tab's middle is 60px past the dragged tab's leading edge.
it.each([
  { name: "a later tab, dragged forward", dragged: 0, rtl: false, before: 50, after: 70, shift: "translateX(-110px)" },
  { name: "an earlier tab, dragged back", dragged: 1, rtl: false, before: -50, after: -70, shift: "translateX(110px)" },
  { name: "a later tab right to left", dragged: 0, rtl: true, before: -50, after: -70, shift: "translateX(110px)" },
  { name: "an earlier tab right to left, dragged back", dragged: 1, rtl: true, before: 50, after: 70, shift: "translateX(-110px)" },
])("makes room for $name once the dragged tab's leading edge passes its middle, drawing the dragged tab on top", ({ dragged, rtl, before, after, shift }) => {
  const { press, move, release, source, other } = setup(true, undefined, { dragged, rtl });
  press(200);
  move(200 + before);
  expect(other().style.transform).toBe("");
  move(200 + after);
  expect(other().style.transform).toBe(shift);
  expect(source().style.zIndex).toBe("2");
  release();
});

it("keeps text from being selected from the press until it ends", () => {
  const { press, release } = setup(true);
  press(10);
  const during = new Event("selectstart", { cancelable: true });
  document.dispatchEvent(during);
  expect(during.defaultPrevented).toBe(true);
  release();
  const after = new Event("selectstart", { cancelable: true });
  document.dispatchEvent(after);
  expect(after.defaultPrevented).toBe(false);
});

it.each([
  { name: "ends when its mouse loses the capture", lose: () => document.dispatchEvent(new PointerEvent("lostpointercapture", { pointerType: "mouse", bubbles: true })), ends: true },
  { name: "ends when the window loses focus", lose: () => window.dispatchEvent(new Event("blur")), ends: true },
  { name: "goes on when a pen's capture is released", lose: () => document.dispatchEvent(new PointerEvent("lostpointercapture", { pointerType: "pen", bubbles: true })), ends: false },
])("a press $name", ({ lose, ends }) => {
  const { press, move, release, other } = setup(true);
  press(10);
  move(80);
  lose();
  expect(other().style.transform).toBe(ends ? "" : "translateX(-110px)");
  const after = new Event("selectstart", { cancelable: true });
  document.dispatchEvent(after);
  expect(after.defaultPrevented).toBe(!ends);
  release();
});
