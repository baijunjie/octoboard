// @vitest-environment jsdom
import { ListBox } from "@heroui/react";
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { BrowserListMotion, useBrowserListMotion } from "../browser/useBrowserListMotion";
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

it("follows a list its owner holds but another component shows and replaces, as a collection renders its panels", async () => {
  function Shown({ panels }: { panels: React.ReactNode[] }) {
    const [shown, setShown] = useState(0);
    return <>{panels.map((_, index) => <button key={index} data-show={index} onClick={() => setShown(index)} />)}{panels[shown]}</>;
  }
  function Owner({ keys }: { keys: string[] }) {
    const ref = useListMotion<HTMLDivElement>();
    return <Shown panels={[null, <div key="first" ref={ref}>{keys.map((key) => <button key={key} data-list-motion={key}>{key}</button>)}</div>, <div key="second" />]} />;
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  act(() => root.render(<Owner keys={["a", "b"]} />));
  const show = (index: number) => act(() => host.querySelector<HTMLButtonElement>(`[data-show="${index}"]`)!.click());
  show(1);
  await act(async () => {});
  act(() => root.render(<Owner keys={["b"]} />));
  expect(host.querySelectorAll("[data-list-motion-ghost]")).toHaveLength(1);
  // The list that goes away takes its departing copies with it; its replacement starts with none.
  show(2);
  show(1);
  expect(host.querySelector("[data-list-motion-ghost]")).toBeNull();
  const calls = animate.mock.calls.length;
  await act(async () => {});
  act(() => root.render(<Owner keys={["a", "b"]} />));
  expect(animate).toHaveBeenCalledTimes(calls + 1);
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


it("captures logical virtual exits before recycled DOM changes and ignores viewport-only identities", async () => {
  let presenceShift = 0;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const [layoutX, y] = points[this.dataset.listMotion ?? ""] ?? [0, 0];
    const x = layoutX + (this.dataset.listMotion ? presenceShift : 0);
    return { x, y, top: y, left: x, right: x + 180, bottom: y + 28, width: 180, height: 28, toJSON: () => ({}) };
  });
  onTestFinished(() => { vi.restoreAllMocks(); });
  function Virtual({ logical, window }: { logical: string[]; window: string[] }) {
    const keys = new Set(logical);
    const { ref, prepare } = useBrowserListMotion(null, keys);
    return <BrowserListMotion keys={keys} prepare={prepare}><div ref={ref} data-root="">{window.map((key, index) => <div key={index} style={{ transform: `translateY(${index * 32}px)` }}><button data-list-motion={key}>{key}</button></div>)}</div></BrowserListMotion>;
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  const render = (logical: string[], window: string[]) => act(() => root.render(<Virtual logical={logical} window={window} />));
  points = { a: [12, 34], b: [12, 66], c: [12, 98] };
  render(["a", "b", "c"], ["a", "b"]);
  const original = host.querySelector('[data-list-motion="a"]');
  const clone = vi.spyOn(original!, "cloneNode");
  render(["a", "b", "c"], ["b", "c"]);
  expect(animate).not.toHaveBeenCalled();
  expect(clone).not.toHaveBeenCalled();
  render(["a", "c", "d"], ["c", "d"]);
  const ghost = host.querySelector<HTMLElement>("[data-list-motion-ghost]")!;
  expect(ghost.textContent).toBe("b");
  expect(ghost.parentElement!.style.left).toBe("12px");
  expect(ghost.parentElement!.style.top).toBe("66px");
  expect(ghost.style.width).toBe("180px");
  expect(ghost.style.height).toBe("28px");
  expect(ghost.style.transform).toBe("none");
  expect(ghost.hasAttribute("inert")).toBe(true);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([
    [{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateX(25%)" }],
    [{ opacity: 0, transform: "translateX(-25%)" }, { opacity: 1, transform: "none" }],
  ]);
  expect(animate.mock.calls.map(([, options]) => options.easing)).toEqual(Array(2).fill("cubic-bezier(0.4, 0, 0.2, 1)"));
  presenceShift = -45;
  act(() => resize());
  expect(animate).toHaveBeenCalledTimes(2);
  reduced = true;
  act(() => preferenceChanged());
  expect(host.querySelector("[data-list-motion-ghost]")).toBeNull();
  render(["e"], ["e"]);
  expect(animate).toHaveBeenCalledTimes(2);
  reduced = false;
  render(["e", "offscreen"], ["e"]);
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  render(["e", "offscreen"], ["offscreen"]);
  expect(animate).toHaveBeenCalledTimes(2);
});

it("folds virtual rows in and out with their parent, nested rows included, and enters rows added under an open parent", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const [x, y] = points[this.dataset.listMotion ?? ""] ?? [0, 0];
    return { x, y, top: y, left: x, right: x + 180, bottom: y + 28, width: 180, height: 28, toJSON: () => ({}) };
  });
  onTestFinished(() => { vi.restoreAllMocks(); });
  // Rows are `parent/child` paths; a parent's rows are shown while it is open.
  function Virtual({ rows, open }: { rows: string[]; open: string[] }) {
    const keys = new Set(rows);
    const parents = new Map(rows.filter((key) => key.includes("/")).map((key) => [key, key.slice(0, key.lastIndexOf("/"))]));
    const folds = { parents, open: new Set(open) };
    const { ref, prepare } = useBrowserListMotion(null, keys, folds);
    return <BrowserListMotion keys={keys} folds={folds} prepare={prepare}><div ref={ref} data-root="">{rows.map((key, index) => <div key={key} style={{ transform: `translateY(${index * 32}px)` }}><button data-list-motion={key}>{key}</button></div>)}</div></BrowserListMotion>;
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  const render = (rows: string[], open: string[]) => act(() => root.render(<Virtual rows={rows} open={open} />));
  const presences = () => animate.mock.calls.filter(([frames]) => frames[0].opacity !== undefined).map(([frames]) => frames[0].opacity);
  render(["a"], []);
  render(["a", "a/b", "a/b/c"], ["a", "a/b"]);
  expect(presences()).toEqual([]);
  render(["a", "a/b", "a/b/c", "a/d", "a/d/e"], ["a", "a/b", "a/d"]);
  expect(presences()).toEqual([0, 0]);
  animate.mockClear();
  render(["a"], []);
  expect(presences()).toEqual([]);
  expect(host.querySelector("[data-list-motion-ghost]")).toBeNull();
  render(["a", "f"], []);
  expect(presences()).toEqual([0]);
});
