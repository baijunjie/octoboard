// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import type { Agent } from "../protocol";
import { beginTabDrag } from "./tabDrag";

const agents = ["claude", "codex"] as unknown as Agent[];

function setup(reduced: boolean, onDrop: (order: Agent[], moved: Agent) => void = () => {}) {
  const preference = new EventTarget() as MediaQueryList;
  Object.defineProperty(preference, "matches", { value: reduced, writable: true });
  vi.stubGlobal("matchMedia", () => preference);
  const elements = new Map<Agent, HTMLElement>();
  agents.forEach((agent, index) => {
    const tab = document.body.appendChild(document.createElement("button"));
    tab.getBoundingClientRect = () => ({ left: index * 110, right: index * 110 + 100, width: 100, top: 0, bottom: 30, height: 30, x: index * 110, y: 0, toJSON() {} });
    elements.set(agent, tab);
  });
  onTestFinished(() => {
    elements.forEach((tab) => tab.remove());
    vi.unstubAllGlobals();
  });
  const press = (clientX: number) => beginTabDrag({
    agent: agents[0]!,
    agents,
    elements,
    event: new PointerEvent("pointerdown", { clientX, button: 0, pointerType: "mouse" }),
    onDrop,
  });
  const move = (clientX: number) => document.dispatchEvent(new PointerEvent("pointermove", { clientX, bubbles: true }));
  const release = () => document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  const other = () => elements.get(agents[1]!)!;
  return { preference, press, move, release, other };
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
