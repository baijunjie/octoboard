// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../input/focusGuard";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { beginRowDrag, movedRow, ROW_DRAG_THRESHOLD } from "./rowDrag";

describe("movedRow", () => {
  it.each([
    ["a", 2, ["b", "c", "a"]],
    ["c", 0, ["c", "a", "b"]],
    ["b", 1, ["a", "b", "c"]],
    ["a", 1, ["b", "a", "c"]],
  ])("puts %s at index %i", (id, to, expected) => {
    expect(movedRow(["a", "b", "c"], id, to)).toEqual(expected);
  });
});

const IDS = ["a", "b", "c"];
const HEIGHT = 40;

function setUp(onDrop: (order: string[], to: number, moved: string) => void = () => {}) {
  vi.stubGlobal("matchMedia", () => Object.assign(new EventTarget(), { matches: true }));
  const elements = new Map<string, HTMLElement>();
  IDS.forEach((id, index) => {
    const row = document.body.appendChild(document.createElement("li"));
    const top = index * HEIGHT;
    row.getBoundingClientRect = () => ({ left: 0, right: 300, width: 300, top, bottom: top + HEIGHT, height: HEIGHT, x: 0, y: top, toJSON() {} });
    elements.set(id, row);
  });
  onTestFinished(() => {
    elements.forEach((row) => row.remove());
    vi.unstubAllGlobals();
  });
  const press = (id: string, init: PointerEventInit = {}) => {
    const row = elements.get(id)!;
    const middle = IDS.indexOf(id) * HEIGHT + HEIGHT / 2;
    beginRowDrag({ id, ids: IDS, elements, event: new PointerEvent("pointerdown", { clientY: middle, button: 0, pointerType: "mouse", ...init }), onDrop });
    return { row, middle };
  };
  const move = (clientY: number) => document.dispatchEvent(new PointerEvent("pointermove", { clientY, bubbles: true }));
  const release = () => document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  const transforms = () => IDS.map((id) => elements.get(id)!.style.transform);
  return { press, move, release, transforms, elements };
}

describe("beginRowDrag", () => {
  it("drops a row dragged down past the next row's middle at its new place, the rows standing as the drag left them, then clears them", () => {
    const seen: string[][] = [];
    const drops: Array<[string[], number, string]> = [];
    const { press, move, release, transforms } = setUp((order, to, moved) => {
      drops.push([order, to, moved]);
      seen.push(transforms());
    });
    const { middle } = press("a");
    move(middle + 50);
    release();
    expect(drops).toEqual([[["b", "a", "c"], 1, "a"]]);
    expect(seen[0]).toEqual(["translateY(50px)", `translateY(-${HEIGHT}px)`, ""]);
    expect(transforms()).toEqual(["", "", ""]);
  });

  it("drops a row dragged up past the middle of the one above it", () => {
    const drops: Array<[string[], number, string]> = [];
    const { press, move, release } = setUp((order, to, moved) => drops.push([order, to, moved]));
    const { middle } = press("c");
    move(middle - 50);
    move(middle - 90);
    release();
    expect(drops).toEqual([[["c", "a", "b"], 0, "c"]]);
  });

  it("does not move a row that has not passed its neighbour's middle, and drops nothing", () => {
    const onDrop = vi.fn();
    const { press, move, release } = setUp(onDrop);
    const { middle } = press("a");
    move(middle + 10);
    release();
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("is a press, not a drag, until the pointer has moved the threshold", () => {
    const onDrop = vi.fn();
    const { press, move, release, transforms } = setUp(onDrop);
    const { middle } = press("a");
    move(middle + ROW_DRAG_THRESHOLD - 1);
    expect(transforms()).toEqual(["", "", ""]);
    release();
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("is ended by Escape with nothing dropped, the rows put back, and nothing more followed", () => {
    const onDrop = vi.fn();
    const { press, move, release, transforms } = setUp(onDrop);
    const { middle } = press("a");
    move(middle + 70);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(transforms()).toEqual(["", "", ""]);
    move(middle + 70);
    release();
    expect(transforms()).toEqual(["", "", ""]);
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("is ended by a cancelled pointer and by the window losing focus", () => {
    const onDrop = vi.fn();
    const { press, move, release } = setUp(onDrop);
    let { middle } = press("a");
    move(middle + 70);
    document.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true }));
    release();
    ({ middle } = press("a"));
    move(middle + 70);
    window.dispatchEvent(new Event("blur"));
    release();
    expect(onDrop).not.toHaveBeenCalled();
  });

  it.each([
    ["a touch", { pointerType: "touch" }],
    ["a press of another button", { button: 2 }],
  ])("is not begun by %s", (_name, init) => {
    const onDrop = vi.fn();
    const { press, move, release, transforms } = setUp(onDrop);
    const { middle } = press("a", init);
    move(middle + 70);
    expect(transforms()).toEqual(["", "", ""]);
    release();
    expect(onDrop).not.toHaveBeenCalled();
  });
});
