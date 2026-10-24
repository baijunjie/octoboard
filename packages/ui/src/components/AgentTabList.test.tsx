// @vitest-environment jsdom
import { Tabs } from "@heroui/react";
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { Agent } from "../protocol";
import { AgentTabList } from "./AgentTabList";
import { presence, stubListMotion } from "./listMotionTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Lays the strip out under jsdom: each tab 100 pixels wide with 10 between, at the place its agent holds in
 * `order`, and shown where its `transform` puts it. */
function layOut(order: () => Agent[]) {
  const at = (tab: HTMLElement) => order().indexOf(tab.dataset.listMotion as Agent) * 110;
  const offsetLeft = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetLeft")!;
  Object.defineProperty(HTMLElement.prototype, "offsetLeft", { configurable: true, get(this: HTMLElement) { return this.dataset.listMotion ? at(this) : 0; } });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const x = this.dataset.listMotion ? at(this) : 0;
    return { x, y: 0, left: x, right: x + 100, top: 0, bottom: 30, width: 100, height: 30, toJSON: () => ({}) };
  });
  vi.stubGlobal("DOMMatrixReadOnly", class {
    m41: number;
    m42 = 0;
    constructor(transform: string) { this.m41 = Number(/translateX\((-?\d+)px\)/.exec(transform)?.[1] ?? 0); }
  });
  onTestFinished(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetLeft", offsetLeft);
    vi.restoreAllMocks();
  });
}

it("moves a dropped tab into its new place from where the drag left it, and the tabs that made room not at all", async () => {
  const motion = stubListMotion();
  let order: Agent[] = ["claude", "codex"];
  layOut(() => order);
  function Strip() {
    const [agents, setAgents] = useState(order);
    order = agents;
    return (
      <Tabs selectedKey={agents[0]}>
        <AgentTabList agents={agents} label="Agents" onReorder={setAgents} />
      </Tabs>
    );
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  act(() => root.render(<Strip />));
  // The tabs are rendered from the collection in commits of their own, which the motion observes.
  await act(async () => {});
  expect(motion.played()).toEqual([]);
  const claude = host.querySelector<HTMLElement>('[data-list-motion="claude"]')!;
  act(() => {
    claude.dispatchEvent(new PointerEvent("pointerdown", { clientX: 50, button: 0, pointerType: "mouse", bubbles: true }));
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 180, bubbles: true }));
    document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  });
  await act(async () => {});
  expect(order).toEqual(["codex", "claude"]);
  // Dragged 130 pixels on, Claude Code is let go 20 beyond its new place at 110; Codex had already made room and
  // stays put.
  expect(motion.played().map(({ element, frames }) => [element, presence(frames) ?? frames[0]])).toEqual([[claude, { translate: "20px 0px" }]]);
  expect(claude.style.transform).toBe("");
});

it("gives focus back to an agent's actions button when reordering the tabs dropped it", async () => {
  stubListMotion();
  let order: Agent[] = ["claude", "codex"];
  layOut(() => order);
  let reorder!: (order: Agent[]) => void;
  function Strip() {
    const [agents, setAgents] = useState<Agent[]>(order);
    reorder = setAgents;
    order = agents;
    return (
      <Tabs selectedKey="claude">
        <AgentTabList agents={agents} label="Agents" tabActions={(agent) => <button type="button" aria-label={`Actions for ${agent}`} />} />
      </Tabs>
    );
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  act(() => root.render(<Strip />));
  await act(async () => {});
  // The controls are placed once list motion has ended.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 400)); });
  const actions = () => host.querySelector<HTMLElement>('[data-tab-actions="claude"] button')!;
  act(() => actions().focus());
  expect(document.activeElement).toBe(actions());
  // Moving a focused element in the document drops its focus, as the reorder does.
  act(() => actions().blur());
  expect(document.activeElement).toBe(document.body);
  act(() => reorder(["codex", "claude"]));
  expect(document.activeElement).toBe(actions());
});
