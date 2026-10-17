// @vitest-environment jsdom
import { ListBox } from "@heroui/react";
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { useListMotion } from "./useListMotion";
import { MotionListBox } from "./MotionListBox";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let points: Record<string, [number, number]>;
let width: number;
let reduced: boolean;
let resize: () => void;
let preferenceChanged: () => void;
let progress: number;
const disconnectResize = vi.fn();
const cancel = vi.fn();
const animate = vi.fn((_frames: Keyframe[], _options: KeyframeAnimationOptions) => ({ cancel, effect: { getComputedTiming: () => ({ progress }) }, onfinish: null }));

beforeEach(() => {
  points = {};
  width = 200;
  reduced = false;
  progress = 0.5;
  vi.clearAllMocks();
  vi.stubGlobal("matchMedia", () => ({ matches: reduced, addEventListener: (_: string, fn: () => void) => { preferenceChanged = fn; }, removeEventListener: vi.fn() }));
  vi.stubGlobal("ResizeObserver", class {
    constructor(fn: () => void) { resize = fn; }
    observe() {}
    unobserve() {}
    disconnect = disconnectResize;
  });
  Object.defineProperties(HTMLElement.prototype, {
    offsetLeft: { configurable: true, get(this: HTMLElement) { return points[this.dataset.listMotion ?? ""]?.[0] ?? 0; } },
    offsetTop: { configurable: true, get(this: HTMLElement) { return points[this.dataset.listMotion ?? ""]?.[1] ?? 0; } },
    clientWidth: { configurable: true, get() { return width; } },
  });
  HTMLElement.prototype.animate = animate as unknown as typeof HTMLElement.prototype.animate;
});

afterEach(() => vi.unstubAllGlobals());

function List({ keys, descendants, virtualized }: { keys: string[]; descendants?: boolean; virtualized?: boolean }) {
  const ref = useListMotion<HTMLDivElement>({ descendants, virtualized });
  const rows = keys.map((key) => <button key={key} data-list-motion={key}>{key}</button>);
  return <div ref={ref} data-root="">{descendants ? <div style={{ display: "contents" }}>{rows}</div> : rows}</div>;
}

function mount(initial: string[], options: { descendants?: boolean; virtualized?: boolean } = {}) {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const render = (keys: string[]) => act(() => root.render(<List keys={keys} {...options} />));
  const unmount = () => { act(() => root.unmount()); host.remove(); };
  onTestFinished(unmount);
  render(initial);
  return { render, host, unmount, list: host.firstElementChild as HTMLElement };
}

it("moves wrapped rows in both axes, and retargets a running slide from its visible position", () => {
  points = { a: [0, 0], b: [80, 0] };
  const list = mount(["a", "b"], { descendants: true });
  points = { a: [80, 0], b: [0, 40] };
  list.render(["b", "a"]);
  expect(animate.mock.calls[0][0]).toEqual([{ translate: "80px -40px" }, { translate: "0px 0px" }]);
  points = { a: [0, 0], b: [80, 0] };
  list.render(["a", "b"]);
  expect(animate.mock.calls[2][0]).toEqual([{ translate: "40px 0px" }, { translate: "0px 0px" }]);
  expect(cancel).toHaveBeenCalledTimes(2);
});

it("observes independently committed descendant rows without reading layout on hover or text changes", async () => {
  points = { a: [0, 0], b: [0, 40] };
  const { list } = mount(["a", "b"], { descendants: true });
  const measure = vi.spyOn(list, "clientWidth", "get");
  list.querySelector("button")!.className = "hovered";
  list.querySelector("button")!.textContent = "new name";
  await act(async () => {});
  expect(measure).not.toHaveBeenCalled();
  points.b = [0, 80];
  const row = document.createElement("button");
  row.dataset.listMotion = "new";
  list.firstElementChild!.prepend(row);
  await act(async () => {});
  expect(animate).toHaveBeenCalledTimes(2);
});

it("resets width changes, cancels live slides when reduced motion changes, and disconnects on unmount", () => {
  points = { a: [0, 0], b: [0, 40] };
  const list = mount(["a", "b"]);
  width = 300;
  points = { a: [0, 40], b: [0, 0] };
  list.render(["b", "a"]);
  expect(animate).not.toHaveBeenCalled();
  points = { a: [0, 0], b: [0, 40] };
  list.render(["a", "b"]);
  expect(animate).toHaveBeenCalledTimes(2);
  reduced = true;
  act(() => preferenceChanged());
  expect(cancel).toHaveBeenCalledTimes(2);
  list.unmount();
  expect(disconnectResize).toHaveBeenCalledOnce();
  act(() => resize());
  expect(animate).toHaveBeenCalledTimes(2);
});

it("keeps focus on a surviving moved control, and does not reclaim deliberately left focus", async () => {
  const list = mount(["a", "b"]);
  const button = list.host.querySelector("button")!;
  act(() => button.focus());
  list.render(["b", "a"]);
  expect(document.activeElement).toBe(button);
  act(() => button.blur());
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  list.render(["a", "b"]);
  expect(document.activeElement).toBe(document.body);
});

it("moves virtual rows by real identity while ignoring scrolling, recycling, and viewport resize", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const [x, y] = points[this.dataset.listMotion ?? ""] ?? [0, 0];
    const scroll = this.closest<HTMLElement>("[data-root]")?.scrollTop ?? 0;
    return { x, y: y - (this.dataset.listMotion ? scroll : 0), top: y, left: x, right: x + 200, bottom: y + 40, width: 200, height: 40, toJSON: () => ({}) };
  });
  onTestFinished(() => { vi.restoreAllMocks(); });
  points = { a: [0, 0], b: [0, 40] };
  const list = mount(["a", "b"], { virtualized: true });
  list.list.scrollTop = 30;
  act(() => list.list.dispatchEvent(new Event("scroll")));
  list.render(["b", "c"]);
  expect(animate).not.toHaveBeenCalled();
  // A recycled DOM node takes a different identity: it is an entering row, never the old row.
  list.list.querySelector("button")!.dataset.listMotion = "recycled";
  act(() => resize());
  expect(animate).not.toHaveBeenCalled();
  points.c = [0, 80];
  act(() => resize());
  expect(animate).toHaveBeenCalledOnce();
  // Recycling while that row is still moving must not carry its old translation to a new item.
  list.list.querySelector('[data-list-motion="c"]')!.setAttribute("data-list-motion", "replacement");
  act(() => resize());
  expect(cancel).toHaveBeenCalledOnce();
  expect(animate).toHaveBeenCalledOnce();
  width = 240;
  act(() => resize());
  expect(cancel).toHaveBeenCalledOnce();
});

it("animates the first update of a list mounted by a child's popup state", async () => {
  function Popup({ children }: { children: React.ReactNode }) {
    const [visible, setVisible] = useState(false);
    return <><button data-open onClick={() => setVisible(true)}>Open</button>{visible && children}</>;
  }
  function View() {
    const [keys, setKeys] = useState(["a", "b"]);
    return <><button data-remove onClick={() => setKeys(["b"])}>Remove</button><Popup><MotionListBox aria-label="Choices">{keys.map((key) => <ListBox.Item key={key} id={key} textValue={key} data-list-motion={key}>{key}</ListBox.Item>)}</MotionListBox></Popup></>;
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  points = { a: [0, 0], b: [0, 40] };
  act(() => root.render(<View />));
  await act(async () => host.querySelector<HTMLButtonElement>("[data-open]")!.click());
  expect(animate).not.toHaveBeenCalled();
  points.b = [0, 0];
  await act(async () => host.querySelector<HTMLButtonElement>("[data-remove]")!.click());
  expect(animate).toHaveBeenCalledTimes(2);
  expect(animate.mock.calls[1][0]).toEqual([{ translate: "0px 40px" }, { translate: "0px 0px" }]);
});

it("enters and exits alongside layout movement without retaining removed controls", () => {
  points = { a: [0, 0], b: [0, 40] };
  const list = mount(["a", "b"]);
  expect(animate).not.toHaveBeenCalled();
  const removed = list.list.querySelector<HTMLElement>('[data-list-motion="a"]')!;
  removed.id = "original";
  removed.innerHTML = '<input id="field" name="field" autofocus><span>Original content</span>';
  act(() => removed.focus());
  points = { b: [0, 0], c: [0, 40] };
  list.render(["b", "c"]);
  const ghost = list.list.querySelector<HTMLElement>("[data-list-motion-ghost]")!;
  expect(removed.isConnected).toBe(false);
  expect(document.activeElement).toBe(document.body);
  expect(ghost.textContent).toBe("Original content");
  expect(ghost.hasAttribute("inert")).toBe(true);
  expect(ghost.getAttribute("aria-hidden")).toBe("true");
  expect(ghost.querySelector("[id], [name], [autofocus], [data-list-motion]")).toBeNull();
  expect(ghost.id).toBe("");
  expect(ghost.style.position).toBe("absolute");
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([
    [{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateX(25%)" }],
    [{ translate: "0px 40px" }, { translate: "0px 0px" }],
    [{ opacity: 0, transform: "translateX(-25%)" }, { opacity: 1, transform: "none" }],
  ]);
  expect(animate.mock.calls.map(([, options]) => options.duration)).toEqual([300, 300, 300]);
  const animation = animate.mock.results[0].value;
  (animation.onfinish as (() => void) | null)?.();
  expect(list.list.querySelector("[data-list-motion-ghost]")).toBeNull();
});

it("cleans up exits on rapid readdition, scrolling, reduced motion and unmount", () => {
  const list = mount(["a", "b"]);
  list.render(["b"]);
  expect(list.list.querySelectorAll("[data-list-motion-ghost]")).toHaveLength(1);
  list.render(["a", "b"]);
  expect(list.list.querySelector("[data-list-motion-ghost]")).toBeNull();
  list.render(["b"]);
  act(() => list.list.dispatchEvent(new Event("scroll")));
  expect(list.list.querySelector("[data-list-motion-ghost]")).toBeNull();
  list.render(["a", "b"]);
  list.render(["b"]);
  reduced = true;
  act(() => preferenceChanged());
  expect(list.list.querySelector("[data-list-motion-ghost]")).toBeNull();
  const calls = animate.mock.calls.length;
  list.render(["a", "b"]);
  list.render(["b"]);
  expect(animate).toHaveBeenCalledTimes(calls);
  reduced = false;
  list.render(["a", "b"]);
  list.render(["b"]);
  const detachedList = list.list;
  list.unmount();
  expect(detachedList.querySelector("[data-list-motion-ghost]")).toBeNull();
});

it("animates a live first addition to an initially empty collection after mounting settles", async () => {
  const list = mount([]);
  await act(async () => {});
  list.render(["a"]);
  expect(animate).toHaveBeenCalledOnce();
  expect(animate.mock.calls[0][0]).toEqual([{ opacity: 0, transform: "translateX(-25%)" }, { opacity: 1, transform: "none" }]);
});

it("preserves resolved full-span subgrid tracks in a departing visual", () => {
  const list = mount(["a"]);
  list.list.style.gridTemplateColumns = "32px 168px";
  const row = list.list.querySelector<HTMLElement>("button")!;
  Object.assign(row.style, { gridTemplateColumns: "subgrid", gridColumn: "1 / -1" });
  act(() => resize());
  list.render([]);
  const ghost = list.list.querySelector<HTMLElement>("[data-list-motion-ghost]")!;
  expect(ghost.style.gridTemplateColumns).toBe("32px 168px");
});

it("lifts departing visuals outside shrinking scrollers without losing their scroll position", () => {
  points = { a: [0, 40] };
  const list = mount(["a"], { descendants: true });
  const scroller = list.list.firstElementChild as HTMLElement;
  scroller.style.overflowY = "auto";
  scroller.scrollTop = 20;
  list.render([]);
  const ghost = list.list.querySelector<HTMLElement>("[data-list-motion-ghost]")!;
  expect(ghost.parentElement!.parentElement).toBe(list.list);
  expect(scroller.children).toHaveLength(0);
  expect(ghost.parentElement!.style.top).toBe("20px");
  act(() => scroller.dispatchEvent(new Event("scroll")));
  expect(list.list.querySelector("[data-list-motion-ghost]")).toBeNull();
});

it("preserves existing transforms and composes horizontal presence with layout translation", async () => {
  const list = mount(["a"]);
  const row = document.createElement("button");
  row.dataset.listMotion = "b";
  row.style.transform = "matrix(1, 0, 0, 1, 5, 0)";
  list.list.appendChild(row);
  await act(async () => {});
  expect(animate.mock.calls[0][0]).toEqual([
    { opacity: 0, transform: "translateX(-25%) matrix(1, 0, 0, 1, 5, 0)" },
    { opacity: 1, transform: "matrix(1, 0, 0, 1, 5, 0)" },
  ]);
  points.b = [0, 40];
  act(() => resize());
  expect(animate.mock.calls[1][0]).toEqual([{ translate: "0px -40px" }, { translate: "0px 0px" }]);
  row.remove();
  await act(async () => {});
  expect(animate.mock.calls[2][0]).toEqual([
    { opacity: 1, transform: "matrix(1, 0, 0, 1, 5, 0)" },
    { opacity: 0, transform: "translateX(25%) matrix(1, 0, 0, 1, 5, 0)" },
  ]);
});

it("bounds exit visuals to their previous box without clipping real controls or bottom exits", () => {
  const list = mount(["a", "b"]);
  const removed = list.list.querySelector<HTMLElement>('[data-list-motion="a"]')!;
  Object.defineProperties(removed, { offsetWidth: { value: 180 }, offsetHeight: { value: 40 } });
  act(() => resize());
  list.render(["b"]);
  const visual = list.list.querySelector<HTMLElement>("[data-list-motion-exit]")!;
  expect(visual.style.width).toBe("180px");
  expect(visual.style.height).toBe("40px");
  expect(visual.style.overflow).toBe("clip");
  expect(visual.hasAttribute("inert")).toBe(true);
  expect(visual.querySelector("[data-list-motion-ghost]")).not.toBeNull();
  const survivor = list.list.querySelector<HTMLElement>('[data-list-motion="b"]')!;
  expect(survivor.parentElement).toBe(list.list);
  expect(list.list.style.overflowX).toBe("");
  list.render(["a", "b"]);
  expect(list.list.querySelector("[data-list-motion-exit]")).toBeNull();
});
