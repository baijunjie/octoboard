// @vitest-environment jsdom
import { ListBox } from "@heroui/react";
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { BrowserListMotion, useBrowserListMotion } from "../browser/useBrowserListMotion";
import { releaseListMotion, useListMotion } from "./useListMotion";
import { MotionListBox } from "./MotionListBox";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let points: Record<string, [number, number]>;
let width: number;
let reduced: boolean;
/** The latest observer's report of `targets`, every element it observes unless given. */
let resize: (targets?: Element[]) => void;
/** Like a real observer, the latest one first reports each element it starts observing, at that element's size. */
let firstReports: () => void;
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
  vi.stubGlobal("matchMedia", () => ({ get matches() { return reduced; }, addEventListener: (_: string, fn: () => void) => { preferenceChanged = fn; }, removeEventListener: vi.fn() }));
  vi.stubGlobal("ResizeObserver", class {
    observed = new Set<Element>();
    pending: Element[] = [];
    constructor(report: (entries: ResizeObserverEntry[]) => void) {
      const entries = (targets: Element[]) => targets.map((target) => ({ target })) as unknown as ResizeObserverEntry[];
      resize = (targets) => report(entries(targets ?? [...this.observed]));
      firstReports = () => {
        const targets = this.pending.splice(0);
        if (targets.length > 0) report(entries(targets));
      };
    }
    observe(element: Element) { this.observed.add(element); this.pending.push(element); }
    unobserve(element: Element) { this.observed.delete(element); this.pending = this.pending.filter((target) => target !== element); }
    disconnect() { disconnectResize(); this.observed.clear(); this.pending = []; }
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

it("takes rows a collection commits after its list, while the mount settles, as part of the mount", async () => {
  // In a browser the observer reports those rows only after the mount has settled.
  const Observer = MutationObserver;
  vi.stubGlobal("MutationObserver", class extends Observer {
    constructor(report: MutationCallback) { super((records, observer) => setTimeout(() => report(records, observer), 0)); }
  });
  const list = mount([]);
  // A react-aria collection renders its rows in a commit of its own, within the same task.
  const row = document.createElement("button");
  row.dataset.listMotion = "late";
  list.list.appendChild(row);
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(animate).not.toHaveBeenCalled();
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

function Strip({ keys, rtl }: { keys: string[]; rtl?: boolean }) {
  const ref = useListMotion<HTMLDivElement>({ contentWidth: true });
  return <div ref={ref} style={{ direction: rtl ? "rtl" : "ltr" }}>{keys.map((key) => <button key={key} data-list-motion={key}>{key}</button>)}</div>;
}

function mountStrip(initial: string[], rtl = false, tabWidths: Record<string, number> = {}) {
  // The strip is `width` wide, each tab 80 pixels unless `tabWidths` says otherwise.
  const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth")!;
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get(this: HTMLElement) { const key = this.dataset.listMotion; return key === undefined ? width : tabWidths[key] ?? 80; } });
  onTestFinished(() => { Object.defineProperty(HTMLElement.prototype, "offsetWidth", offsetWidth); });
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const render = (keys: string[]) => act(() => root.render(<Strip keys={keys} rtl={rtl} />));
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  render(initial);
  return { render, strip: host.firstElementChild as HTMLElement };
}

it("takes a content-width strip's change of width with its tabs as that change, and a change of width alone as a resize", () => {
  width = 160;
  points = { a: [0, 0], b: [80, 0] };
  const strip = mountStrip(["a", "b"]);
  width = 240;
  points.c = [160, 0];
  strip.render(["a", "b", "c"]);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([[{ opacity: 0, transform: "translateX(-25%)" }, { opacity: 1, transform: "none" }]]);
  animate.mockClear();
  width = 160;
  points = { b: [0, 0], c: [80, 0] };
  strip.render(["b", "c"]);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([
    [{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateX(25%)" }],
    [{ translate: "80px 0px" }, { translate: "0px 0px" }],
    [{ translate: "80px 0px" }, { translate: "0px 0px" }],
  ]);
  const ghost = strip.strip.querySelector<HTMLElement>("[data-list-motion-ghost]")!;
  expect(ghost.closest("[inert][aria-hidden=true]")).not.toBeNull();
  animate.mockClear();
  width = 200;
  points = { b: [0, 0], c: [100, 0] };
  act(() => resize());
  expect(animate).not.toHaveBeenCalled();
});

it("slides the tabs after one that grows or shrinks on its own, but not when every tab changes width at once", () => {
  const tabWidths: Record<string, number> = {};
  width = 240;
  points = { a: [0, 0], b: [80, 0], c: [160, 0] };
  const strip = mountStrip(["a", "b", "c"], false, tabWidths);
  // `b` gains a mark and grows by 20: `c` slides along from where it stood.
  tabWidths.b = 100;
  width = 260;
  points.c = [180, 0];
  act(() => resize());
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([[{ translate: "-20px 0px" }, { translate: "0px 0px" }]]);
  // Once that slide is over the mark goes again, and the strip keeps its width until `c` has slid back.
  animate.mockClear();
  progress = 1;
  delete tabWidths.b;
  width = 240;
  points.c = [160, 0];
  act(() => resize());
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([[{ translate: "20px 0px" }, { translate: "0px 0px" }]]);
  expect(strip.strip.style.minWidth).toBe("260px");
  // Every tab wider at once (the text zoomed) is a resize: nothing moves.
  animate.mockClear();
  Object.assign(tabWidths, { a: 90, b: 90, c: 90 });
  width = 270;
  points = { a: [0, 0], b: [90, 0], c: [180, 0] };
  act(() => resize());
  expect(animate).not.toHaveBeenCalled();
});

it("puts the tabs after one that changed width straight in place while reduced motion is asked for", () => {
  const tabWidths: Record<string, number> = {};
  reduced = true;
  width = 160;
  points = { a: [0, 0], b: [80, 0] };
  mountStrip(["a", "b"], false, tabWidths);
  tabWidths.a = 100;
  width = 180;
  points.b = [100, 0];
  act(() => resize());
  expect(animate).not.toHaveBeenCalled();
});

it("measures a right-to-left strip from its right edge, so tabs that stay put while it grows at its end do not slide", () => {
  // The strip ends at 300 and grows leftward; its first tab sits at its right end. The strip's own offset is keyed
  // by no tab.
  width = 160;
  points = { "": [140, 0], a: [220, 0], b: [140, 0] };
  const strip = mountStrip(["a", "b"], true);
  width = 240;
  points = { "": [60, 0], a: [220, 0], b: [140, 0], c: [60, 0] };
  strip.render(["a", "b", "c"]);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([[{ opacity: 0, transform: "translateX(-25%)" }, { opacity: 1, transform: "none" }]]);
  animate.mockClear();
  width = 160;
  points = { "": [140, 0], b: [220, 0], c: [140, 0] };
  strip.render(["b", "c"]);
  expect(animate.mock.calls.slice(1).map(([frames]) => frames)).toEqual([
    [{ translate: "-80px 0px" }, { translate: "0px 0px" }],
    [{ translate: "-80px 0px" }, { translate: "0px 0px" }],
  ]);
  // The departing tab's visual stands where the tab stood (jsdom gives it no offset parent to measure from).
  expect(strip.strip.querySelector<HTMLElement>("[data-list-motion-exit]")!.style.left).toBe("220px");
});

it("starts a released drag's move from where its transforms show the tabs, for the change that follows only", async () => {
  vi.stubGlobal("DOMMatrixReadOnly", class {
    m41: number;
    m42 = 0;
    constructor(transform: string) { this.m41 = Number(/translateX\((-?\d+)px\)/.exec(transform)?.[1] ?? 0); }
  });
  points = { a: [0, 0], b: [80, 0] };
  const strip = mountStrip(["a", "b"]);
  const [a, b] = Array.from(strip.strip.querySelectorAll<HTMLElement>("[data-list-motion]"));
  // `a` was dragged past `b`, which made room for it.
  a.style.transform = "translateX(100px)";
  b.style.transform = "translateX(-80px)";
  releaseListMotion(a);
  a.style.transform = "";
  b.style.transform = "";
  points = { b: [0, 0], a: [80, 0] };
  strip.render(["b", "a"]);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([[{ translate: "20px 0px" }, { translate: "0px 0px" }]]);
  // A reorder brings nothing in or out, so there is no travel to clip: the dropped tab's slide shows whole.
  expect([strip.strip.style.minWidth, strip.strip.style.overflow]).toEqual(["", ""]);
  animate.mockClear();
  progress = 1;
  a.style.transform = "translateX(-80px)";
  releaseListMotion(a);
  a.style.transform = "";
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  points = { a: [0, 0], b: [80, 0] };
  strip.render(["a", "b"]);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([
    [{ translate: "80px 0px" }, { translate: "0px 0px" }],
    [{ translate: "-80px 0px" }, { translate: "0px 0px" }],
  ]);
});

it("keeps a strip that loses a tab at its width, clipping, until the motion has played out, and not under reduced motion", () => {
  width = 160;
  points = { a: [0, 0], b: [80, 0] };
  const strip = mountStrip(["a", "b"]);
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  onTestFinished(() => { vi.useRealTimers(); });
  width = 80;
  points = { b: [0, 0] };
  strip.render(["b"]);
  expect([strip.strip.style.minWidth, strip.strip.style.overflow]).toEqual(["160px", "clip"]);
  act(() => { vi.advanceTimersByTime(300); });
  expect([strip.strip.style.minWidth, strip.strip.style.overflow]).toEqual(["", ""]);
  reduced = true;
  width = 160;
  strip.render(["a", "b"]);
  width = 80;
  strip.render(["b"]);
  expect([strip.strip.style.minWidth, strip.strip.style.overflow]).toEqual(["", ""]);
});

function FitList({ keys, options, style }: { keys: string[]; options: Parameters<typeof useListMotion>[0]; style?: React.CSSProperties }) {
  const ref = useListMotion<HTMLDivElement>(options);
  return <div ref={ref} data-root="" style={style}>{keys.map((key) => <button key={key} data-list-motion={key}>{key}</button>)}</div>;
}

function mountFit(initial: string[], options: Parameters<typeof useListMotion>[0] = { fitHeight: true }, style?: React.CSSProperties) {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const render = (keys: string[]) => act(() => root.render(<FitList keys={keys} options={options} style={style} />));
  const unmount = () => { act(() => root.unmount()); host.remove(); };
  onTestFinished(unmount);
  render(initial);
  return { render, unmount, list: host.firstElementChild as HTMLElement };
}

/** The list's laid-out block size, and its rows' by key, which jsdom would otherwise leave at nothing. */
function stubListHeight(initial: number) {
  const state = { height: initial, rows: {} as Record<string, number> };
  const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) { return this.hasAttribute("data-root") ? state.height : state.rows[this.dataset.listMotion ?? ""] ?? 0; },
  });
  onTestFinished(() => {
    if (original) Object.defineProperty(HTMLElement.prototype, "offsetHeight", original);
    else Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
  });
  return state;
}

const heightFrames = () => animate.mock.calls.map(([frames]) => frames).filter((frames) => frames[0]?.height !== undefined);
const heightRuns = () => animate.mock.results.filter((_, index) => animate.mock.calls[index]![0][0]?.height !== undefined).map((result) => result.value as { onfinish: () => void });

it("animates a fitHeight collection's block size over the change of its rows, clipped meanwhile, and lets go of it after", () => {
  const list = stubListHeight(100);
  points = { a: [0, 0], b: [0, 50] };
  const strip = mountFit(["a", "b"]);
  list.height = 50;
  points = { a: [0, 0] };
  strip.render(["a"]);
  expect(heightFrames()).toEqual([[{ height: "100px" }, { height: "50px" }]]);
  expect(animate.mock.calls.find(([frames]) => frames[0]?.height !== undefined)![1]).toMatchObject({ duration: 300, easing: "cubic-bezier(0.2, 0, 0, 1)" });
  expect(strip.list.style.overflowY).toBe("clip");
  act(() => heightRuns()[0]!.onfinish());
  expect(strip.list.style.overflowY).toBe("");
});

it("starts an interrupted fitHeight change from the size shown, and leaves nothing behind once the last has played", () => {
  const list = stubListHeight(100);
  points = { a: [0, 0], b: [0, 50] };
  const strip = mountFit(["a", "b"]);
  // Mid-way the list shows 75px; the rows come back, so once the running change is cancelled it lays out at 100px.
  const plain = animate.getMockImplementation()!;
  onTestFinished(() => { animate.mockImplementation(plain); });
  animate.mockImplementation((frames) => ({ cancel: (frames[0]?.height === undefined ? cancel : () => { list.height = 100; }) as typeof cancel, effect: { getComputedTiming: () => ({ progress }) }, onfinish: null }));
  list.height = 50;
  points = { a: [0, 0] };
  strip.render(["a"]);
  list.height = 75;
  points = { a: [0, 0], b: [0, 50] };
  strip.render(["a", "b"]);
  expect(heightFrames().at(-1)).toEqual([{ height: "75px" }, { height: "100px" }]);
  expect(strip.list.style.overflowY).toBe("clip");
  const run = animate.mock.results.at(-1)!.value as { onfinish: () => void };
  act(() => run.onfinish());
  expect(strip.list.style.overflowY).toBe("");
});

it("leaves a fitHeight collection's block size alone under reduced motion", () => {
  const list = stubListHeight(100);
  points = { a: [0, 0], b: [0, 50] };
  const strip = mountFit(["a", "b"]);
  reduced = true;
  list.height = 50;
  points = { a: [0, 0] };
  strip.render(["a"]);
  expect(heightFrames()).toEqual([]);
  expect(strip.list.style.overflowY).toBe("");
});

it("does not fit the block size of a collection that did not ask for it", () => {
  const list = stubListHeight(100);
  points = { a: [0, 0], b: [0, 50] };
  const plain = mount(["a", "b"]);
  list.height = 50;
  points = { a: [0, 0] };
  plain.render(["a"]);
  expect(heightFrames()).toEqual([]);
});

it("sets an inline overflow-y back as it was, and does so when the collection unmounts mid-change", () => {
  const list = stubListHeight(100);
  points = { a: [0, 0], b: [0, 50] };
  const strip = mountFit(["a", "b"], { fitHeight: true }, { overflowY: "auto" });
  list.height = 50;
  points = { a: [0, 0] };
  strip.render(["a"]);
  expect(strip.list.style.overflowY).toBe("clip");
  act(() => heightRuns()[0]!.onfinish());
  expect(strip.list.style.overflowY).toBe("auto");
  // The change of size gets a cancel of its own, apart from the one the rows' movement shares.
  const plain = animate.getMockImplementation()!;
  onTestFinished(() => { animate.mockImplementation(plain); });
  const fitCancel = vi.fn();
  animate.mockImplementation((frames) => ({ cancel: frames[0]?.height === undefined ? cancel : fitCancel, effect: { getComputedTiming: () => ({ progress }) }, onfinish: null }));
  list.height = 100;
  points = { a: [0, 0], b: [0, 50] };
  strip.render(["a", "b"]);
  expect(strip.list.style.overflowY).toBe("clip");
  expect(fitCancel).not.toHaveBeenCalled();
  strip.unmount();
  expect(fitCancel).toHaveBeenCalledOnce();
  expect(strip.list.style.overflowY).toBe("auto");
});

it("takes what the observer reports beyond the list's own animated size during a change, and not that", () => {
  const list = stubListHeight(150);
  list.rows = { a: 50, b: 50, c: 50 };
  points = { a: [0, 0], b: [0, 50], c: [0, 100] };
  const strip = mountFit(["a", "b", "c"]);
  act(() => firstReports());
  // While a change of size runs, the list shows its animated size; cancelled, it lays out at `laid`.
  let laid = 50;
  const plain = animate.getMockImplementation()!;
  onTestFinished(() => { animate.mockImplementation(plain); });
  animate.mockImplementation((frames) => ({ cancel: (frames[0]?.height === undefined ? cancel : () => { list.height = laid; }) as typeof cancel, effect: { getComputedTiming: () => ({ progress }) }, onfinish: null }));
  list.height = 50;
  points = { a: [0, 0] };
  strip.render(["a"]);
  expect(heightFrames()).toEqual([[{ height: "150px" }, { height: "50px" }]]);
  animate.mockClear();
  // Mid-way it shows 100px. The list's own size, and a row at the size it was measured at, are no change.
  list.height = 100;
  const row = strip.list.querySelector("[data-list-motion=a]")!;
  act(() => resize([strip.list, row]));
  expect(animate).not.toHaveBeenCalled();
  expect(strip.list.style.overflowY).toBe("clip");
  // A row that grows during the change is one: the change goes on from the size shown to the new one.
  laid = 70;
  list.rows.a = 70;
  act(() => resize([row]));
  expect(heightFrames()).toEqual([[{ height: "100px" }, { height: "70px" }]]);
  expect(strip.list.style.overflowY).toBe("clip");
  // A width change comes with the report: that is a resize, which ends the change and plays none.
  animate.mockClear();
  width = 120;
  act(() => resize([strip.list]));
  expect(animate).not.toHaveBeenCalled();
  expect(strip.list.style.overflowY).toBe("");
});

it("leaves a running change alone when the observer first reports the rows that change brought in", () => {
  const list = stubListHeight(50);
  list.rows = { a: 50 };
  points = { a: [0, 0] };
  const strip = mountFit(["a"]);
  act(() => firstReports());
  list.height = 150;
  list.rows = { a: 50, b: 50, c: 50 };
  points = { a: [0, 0], b: [0, 50], c: [0, 100] };
  strip.render(["a", "b", "c"]);
  expect(heightFrames()).toEqual([[{ height: "50px" }, { height: "150px" }]]);
  const calls = animate.mock.calls.length;
  const cancels = cancel.mock.calls.length;
  act(() => firstReports());
  expect(animate).toHaveBeenCalledTimes(calls);
  expect(cancel).toHaveBeenCalledTimes(cancels);
  expect(strip.list.style.overflowY).toBe("clip");
});

it("leaves the block size of a virtualized or content-width collection alone, and of a width resize", () => {
  const list = stubListHeight(100);
  points = { a: [0, 0], b: [0, 50] };
  const virtual = mountFit(["a", "b"], { fitHeight: true, virtualized: true });
  const strip = mountFit(["a", "b"], { fitHeight: true, contentWidth: true });
  list.height = 50;
  points = { a: [0, 0] };
  virtual.render(["a"]);
  strip.render(["a"]);
  expect(heightFrames()).toEqual([]);
  const plain = mountFit(["a", "b"]);
  width = 300;
  list.height = 30;
  plain.render(["a"]);
  expect(heightFrames()).toEqual([]);
});

it("starts a growing collection from the size shown, one of its rows having grown, so what follows glides the whole way", () => {
  const list = stubListHeight(100);
  list.rows = { a: 100 };
  points = { a: [0, 0], b: [0, 100] };
  const strip = mountFit(["a", "b"]);
  strip.render(["a"]);
  animate.mockClear();
  // `a` gains a line and `c` comes in: the grown part of `a` shows as the clip reaches it.
  list.height = 220;
  list.rows = { a: 120, c: 100 };
  points = { a: [0, 0], c: [0, 120] };
  strip.render(["a", "c"]);
  expect(heightFrames().at(-1)).toEqual([{ height: "100px" }, { height: "220px" }]);
});

it("grows from the size shown as rows come in above the rows that stay", () => {
  const list = stubListHeight(50);
  list.rows = { b: 50 };
  points = { b: [0, 0] };
  const strip = mountFit(["b"]);
  list.height = 100;
  list.rows = { a: 50, b: 50 };
  points = { a: [0, 0], b: [0, 50] };
  strip.render(["a", "b"]);
  expect(heightFrames()).toEqual([[{ height: "50px" }, { height: "100px" }]]);
});

it("follows a row that grows or shrinks on its own, wherever it stands", () => {
  const list = stubListHeight(133);
  list.rows = { a: 93, b: 40 };
  points = { a: [0, 0], b: [0, 93] };
  mountFit(["a", "b"]);
  act(() => firstReports());
  // `a` gains a line: `b` slides down from where it stood and the list grows with it.
  list.height = 153;
  list.rows = { a: 113, b: 40 };
  points = { a: [0, 0], b: [0, 113] };
  act(() => resize());
  expect(heightFrames()).toEqual([[{ height: "133px" }, { height: "153px" }]]);
  act(() => heightRuns()[0]!.onfinish());
  // The last row growing and shrinking again: the list follows it both ways.
  list.height = 173;
  list.rows = { a: 113, b: 60 };
  act(() => resize());
  act(() => heightRuns()[1]!.onfinish());
  list.height = 153;
  list.rows = { a: 113, b: 40 };
  act(() => resize());
  expect(heightFrames().slice(1)).toEqual([[{ height: "153px" }, { height: "173px" }], [{ height: "173px" }, { height: "153px" }]]);
});

it("starts the change of a row its owner renders larger as the owner renders, not from the observer's report", () => {
  const list = stubListHeight(133);
  list.rows = { a: 93, b: 40 };
  points = { a: [0, 0], b: [0, 93] };
  const strip = mountFit(["a", "b"]);
  list.height = 153;
  list.rows = { a: 113, b: 40 };
  points = { a: [0, 0], b: [0, 113] };
  strip.render(["a", "b"]);
  expect(heightFrames()).toEqual([[{ height: "133px" }, { height: "153px" }]]);
  // The observer then reports the row at the size it was measured at: the change goes on.
  const calls = animate.mock.calls.length;
  act(() => resize([strip.list.querySelector("[data-list-motion=a]")!]));
  expect(animate).toHaveBeenCalledTimes(calls);
});

/** A fitHeight list rendered into `parent`, which the test owns. */
function mountFitInto(parent: HTMLElement, outer: HTMLElement) {
  const root = createRoot(parent);
  onTestFinished(() => { act(() => root.unmount()); outer.remove(); });
  return (keys: string[]) => act(() => root.render(<FitList keys={keys} options={{ fitHeight: true }} />));
}

/** A scroller whose position the test sets, as a browser clamps it when what it holds gets shorter. */
function stubScrollTop(element: HTMLElement, initial: number) {
  const state = { top: initial };
  Object.defineProperty(element, "scrollTop", { configurable: true, get: () => state.top, set: (value: number) => { state.top = value; } });
  return state;
}

it("puts a scroller back where it stood as the list's shrinking starts, so it clamps as the size plays out", () => {
  const list = stubListHeight(300);
  const host = document.body.appendChild(document.createElement("div"));
  host.style.overflowY = "auto";
  points = { a: [0, 0], b: [0, 150] };
  const render = mountFitInto(host, host);
  render(["a", "b"]);
  const scroll = stubScrollTop(host, 0);
  scroll.top = 200;
  act(() => host.dispatchEvent(new Event("scroll")));
  list.height = 150;
  // The browser clamps the position as it lays out the smaller list.
  scroll.top = 100;
  points = { a: [0, 0] };
  render(["a"]);
  expect(scroll.top).toBe(200);
});

it("leaves a running shrinking change and its scroller alone when its owner renders again with nothing changed", () => {
  const list = stubListHeight(300);
  const host = document.body.appendChild(document.createElement("div"));
  host.style.overflowY = "auto";
  points = { a: [0, 0], b: [0, 150] };
  const render = mountFitInto(host, host);
  render(["a", "b"]);
  const scroll = stubScrollTop(host, 0);
  scroll.top = 200;
  act(() => host.dispatchEvent(new Event("scroll")));
  // Cancelled, the change would lay the list out at its new size, which clamps the scroller.
  const plain = animate.getMockImplementation()!;
  onTestFinished(() => { animate.mockImplementation(plain); });
  const fitCancel = vi.fn(() => { scroll.top = 100; });
  animate.mockImplementation((frames) => ({ cancel: frames[0]?.height === undefined ? cancel : fitCancel, effect: { getComputedTiming: () => ({ progress }) }, onfinish: null }));
  list.height = 150;
  scroll.top = 100;
  points = { a: [0, 0] };
  render(["a"]);
  expect(scroll.top).toBe(200);
  render(["a"]);
  expect(fitCancel).not.toHaveBeenCalled();
  expect(heightFrames()).toEqual([[{ height: "300px" }, { height: "150px" }]]);
  expect(scroll.top).toBe(200);
  expect(host.querySelector("[data-root]")!.getAttribute("style")).toBe("overflow-y: clip;");
});

/** A fitHeight list 300px tall in a 50px scroller scrolled to 200, which shrinks to 150px: the scroller's end
 * moves from 250 to 100. `top` is where the scroller stands once the smaller list is laid out. */
function shrinkInScroller(top: number) {
  const list = stubListHeight(300);
  const host = document.body.appendChild(document.createElement("div"));
  host.style.overflowY = "auto";
  points = { a: [0, 0], b: [0, 150] };
  const render = mountFitInto(host, host);
  Object.defineProperties(host, { scrollHeight: { configurable: true, value: 300 }, clientHeight: { configurable: true, value: 50 } });
  render(["a", "b"]);
  const scroll = stubScrollTop(host, 0);
  scroll.top = 200;
  act(() => host.dispatchEvent(new Event("scroll")));
  Object.defineProperty(host, "scrollHeight", { configurable: true, value: 150 });
  scroll.top = top;
  list.height = 150;
  points = { a: [0, 0] };
  render(["a"]);
  expect(heightFrames()).toEqual([[{ height: "300px" }, { height: "150px" }]]);
  return scroll.top;
}

it("puts back a scroller the shrinking list clamped at its new end", () => {
  expect(shrinkInScroller(100)).toBe(200);
});

it("does not put back a scroller that was scrolled up short of its new end, rather than clamped, as the list shrinks", () => {
  // Scrolled up to 50 in the same task as the change, before any scroll event.
  expect(shrinkInScroller(50)).toBe(50);
});

it("keeps a released drag for the change it announces across a render of a fitted list's owner that changes nothing", () => {
  vi.stubGlobal("DOMMatrixReadOnly", class {
    m41 = 0;
    m42: number;
    constructor(transform: string) { this.m42 = Number(/translateY\((-?\d+)px\)/.exec(transform)?.[1] ?? 0); }
  });
  points = { a: [0, 0], b: [0, 50] };
  const strip = mountFit(["a", "b"]);
  const a = strip.list.querySelector<HTMLElement>("[data-list-motion=a]")!;
  // `a` was dragged 60px down, past `b`.
  a.style.transform = "translateY(60px)";
  releaseListMotion(a);
  a.style.transform = "";
  strip.render(["a", "b"]);
  points = { b: [0, 0], a: [0, 50] };
  strip.render(["b", "a"]);
  expect(animate.mock.calls.map(([frames]) => frames)).toEqual([
    [{ translate: "0px 50px" }, { translate: "0px 0px" }],
    [{ translate: "0px 10px" }, { translate: "0px 0px" }],
  ]);
});

it("puts back the ancestor that scrolls, not the nearest one that may, and only as the list shrinks", () => {
  const list = stubListHeight(150);
  const outer = document.body.appendChild(document.createElement("div"));
  outer.style.overflowY = "auto";
  const inner = outer.appendChild(document.createElement("div"));
  // The nearest ancestor whose overflow lets it scroll, though nothing overflows it vertically.
  inner.style.overflowY = "auto";
  const render = mountFitInto(inner, outer);
  points = { a: [0, 0], b: [0, 150] };
  render(["a"]);
  const scroll = stubScrollTop(outer, 0);
  scroll.top = 120;
  act(() => outer.dispatchEvent(new Event("scroll")));
  // Growing: something else scrolled the page up meanwhile, which stays.
  list.height = 300;
  scroll.top = 40;
  render(["a", "b"]);
  expect(heightFrames().at(-1)).toEqual([{ height: "150px" }, { height: "300px" }]);
  expect(scroll.top).toBe(40);
  act(() => heightRuns().at(-1)!.onfinish());
  scroll.top = 200;
  act(() => outer.dispatchEvent(new Event("scroll")));
  // Shrinking: the clamp is undone, on the ancestor that scrolls.
  list.height = 150;
  scroll.top = 50;
  points = { a: [0, 0] };
  render(["a"]);
  expect(scroll.top).toBe(200);
});

it("stops following its ancestors' scrolling once it is gone", () => {
  stubListHeight(100);
  const add = vi.spyOn(document, "addEventListener");
  const remove = vi.spyOn(document, "removeEventListener");
  onTestFinished(() => { add.mockRestore(); remove.mockRestore(); });
  points = { a: [0, 0] };
  const strip = mountFit(["a"]);
  const follow = add.mock.calls.find(([type, , options]) => type === "scroll" && typeof options === "object" && options.capture)![1];
  strip.unmount();
  expect(remove).toHaveBeenCalledWith("scroll", follow, true);
});
