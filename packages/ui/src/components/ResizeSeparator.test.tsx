// @vitest-environment jsdom
import type React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { ResizeSeparator } from "./ResizeSeparator";
import { enter, leave, openTooltip } from "./tooltipTestSupport";

/** The elements a tooltip has been placed against. */
const anchored: Element[] = [];
const placements: (string | undefined)[] = [];
vi.mock("./AnchoredTooltip", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./AnchoredTooltip")>();
  return {
    AnchoredTooltip: (props: React.ComponentProps<typeof actual.AnchoredTooltip>) => {
      if (props.hovered) {
        anchored.push(props.anchor.current!);
        placements.push(props.placement);
      }
      return actual.AnchoredTooltip(props);
    },
  };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(orientation: "vertical" | "horizontal", grow: 1 | -1 = 1, direction: "ltr" | "rtl" = "ltr", extra: { appearance?: "grip"; tooltip?: string } = {}) {
  const onChange = vi.fn();
  const onCommit = vi.fn();
  const onReset = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  container.dir = direction;
  const root = createRoot(container);
  act(() =>
    root.render(
      <ResizeSeparator orientation={orientation} value={200} min={100} max={400} label="Resize" grow={grow} {...extra} onChange={onChange} onCommit={onCommit} onReset={onReset} />,
    ),
  );
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    document.documentElement.className = "";
    document.documentElement.removeAttribute("style");
  });
  const separator = container.querySelector<HTMLElement>("[role=separator]")!;
  const key = (key: string, shiftKey = false) => act(() => void separator.dispatchEvent(new KeyboardEvent("keydown", { key, shiftKey, bubbles: true })));
  const pointer = (type: string, init: PointerEventInit) => act(() => void separator.dispatchEvent(new PointerEvent(type, { bubbles: true, ...init })));
  separator.setPointerCapture = vi.fn();
  return { separator, onChange, onCommit, onReset, key, pointer };
}

it.each([
  ["vertical", "ArrowRight", "ArrowLeft"],
  ["horizontal", "ArrowDown", "ArrowUp"],
] as const)("%s: grows with %s and shrinks with %s, in larger steps with Shift, to the bounds with End and Home", (orientation, grows, shrinks) => {
  const { separator, onChange, key } = mount(orientation);
  expect(separator.getAttribute("aria-orientation")).toBe(orientation);
  key(grows);
  key(shrinks, true);
  key("End");
  key("Home");
  expect(onChange.mock.calls.map(([value]) => value)).toEqual([216, 136, 400, 100]);
});

it("follows the reading direction on a vertical line but not on a horizontal one", () => {
  const vertical = mount("vertical", 1, "rtl");
  vertical.key("ArrowLeft");
  expect(vertical.onChange).toHaveBeenLastCalledWith(216);
  const horizontal = mount("horizontal", 1, "rtl");
  horizontal.key("ArrowDown");
  expect(horizontal.onChange).toHaveBeenLastCalledWith(216);
});

it.each([
  ["vertical", { clientX: 10, clientY: 0 }, { clientX: 50, clientY: 90 }],
  ["horizontal", { clientX: 0, clientY: 10 }, { clientX: 90, clientY: 50 }],
] as const)("%s: a drag along its axis changes the value without committing, and commits once at the end", (orientation, from, to) => {
  const { separator, onChange, onCommit, pointer } = mount(orientation);
  pointer("pointerdown", { button: 0, pointerId: 1, ...from });
  expect(separator.setPointerCapture).toHaveBeenCalledWith(1);
  expect(document.documentElement.classList.contains("pane-resizing")).toBe(true);
  expect(document.documentElement.style.getPropertyValue("--pane-resize-cursor")).toBe(orientation === "vertical" ? "col-resize" : "row-resize");
  pointer("pointermove", { pointerId: 1, ...to });
  expect(onChange).toHaveBeenLastCalledWith(240, { commit: false });
  expect(onCommit).not.toHaveBeenCalled();
  pointer("lostpointercapture", { pointerId: 1 });
  expect(onCommit).toHaveBeenCalledOnce();
});

it("resets on a double click", () => {
  const { separator, onReset } = mount("horizontal");
  act(() => void separator.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
  expect(onReset).toHaveBeenCalledOnce();
});

it.each([
  ["vertical", "h-10 w-1"],
  ["horizontal", "h-1 w-10"],
] as const)("%s: draws a short rounded grip that shows on hover, focus and drag only", (orientation, size) => {
  const { separator } = mount(orientation, 1, "ltr", { appearance: "grip" });
  const grip = separator.firstElementChild!;
  expect(grip.className).toContain(size);
  expect(grip.className).toContain("rounded-full");
  expect(grip.className).toContain("opacity-0 ");
  expect(grip.className).toContain("group-hover:opacity-100");
  expect(grip.className).toContain("group-data-focus-visible:opacity-100");
  // Light grey on hover and focus; the strong colour only once pressed.
  expect(grip.className).toContain("bg-(--grip-hover)");
  expect(grip.className).not.toContain("--grip-active");
  act(() => void separator.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerId: 1 })));
  expect(grip.className).toContain("bg-(--grip-active)");
  expect(grip.className).not.toContain("bg-(--grip-hover)");
});

it("shows its tooltip once the mouse has rested on the handle, without naming the separator a second time", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { separator } = mount("vertical", -1, "ltr", { appearance: "grip", tooltip: "Resize panel" });
  expect(separator.getAttribute("aria-label")).toBe("Resize");
  expect(separator.hasAttribute("aria-describedby")).toBe(false);
  enter(separator);
  expect(openTooltip()).toBeNull();
  act(() => void vi.runOnlyPendingTimers());
  expect(openTooltip()?.textContent).toBe("Resize panel");
  expect(separator.hasAttribute("aria-describedby")).toBe(false);
  // Placed against the grip, not the handle's full-height box.
  expect(anchored.at(-1)).toBe(separator.firstElementChild);
  // Beside a vertical grip, toward the end (mirrored under right-to-left).
  expect(placements.at(-1)).toBe("end");
  leave(separator);
  expect(openTooltip()).toBeNull();
});

it("has no tooltip unless it is given one", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { separator } = mount("vertical");
  enter(separator);
  act(() => void vi.runOnlyPendingTimers());
  expect(openTooltip()).toBeNull();
});

it("opens a horizontal grip's tooltip above it", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const { separator } = mount("horizontal", 1, "ltr", { appearance: "grip", tooltip: "Resize panel" });
  enter(separator);
  act(() => void vi.runOnlyPendingTimers());
  expect(placements.at(-1)).toBeUndefined();
  expect(openTooltip()?.textContent).toBe("Resize panel");
});
