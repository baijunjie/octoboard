// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS, READ_TIME_MS } from "../../components/announce/StatusAnnouncer";
import { CONTROL_TOOLTIP_DELAY_MS, activate, enter, openTooltip, tooltipsCooledDown } from "../../components/hint/tooltipTestSupport";
import { isolate } from "../../i18n/isolate";
import { setLanguage } from "../../i18n/language";
import type { HostFacts } from "../../platform";
import { fakeTimers } from "../../test/clock";
import { MAC_HOST, WINDOWS_HOST } from "../../test/hosts";
import { mountHost } from "../../test/mountHost";
import { ZoomableImage } from "./ZoomableImage";
import { ZoomGroup, ZoomGroupContext } from "./zoomGroup";
import { layout, load, resizeScrollers, stubScrollerLayout } from "./zoomTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  stubScrollerLayout();
  onTestFinished(() => setLanguage("en"));
});

interface Mounted {
  dialog: HTMLElement;
  onError: ReturnType<typeof vi.fn>;
}

/** Mounts an image inside a `role=dialog`; none has loaded. With `sides`, the image is the sides of an image change instead:
 * the Before image (priority 0) and the After image (priority 1) of one zoom group, or the Before side alone, as for a
 * deleted file. The fake clock is on from before the mount and the status region's first-write delay has passed. */
function mountZoomable({ host = WINDOWS_HOST, sides, name = "photo.png" }: { host?: HostFacts; sides?: "pair" | "before"; name?: string } = {}): Mounted {
  fakeTimers();
  const onError = vi.fn();
  const group = new ZoomGroup();
  const image = (index: number) => <ZoomableImage key={index} url={`data:image/png;base64,${index}`} name={name} onError={onError} />;
  const container = mountHost(
    <div role="dialog" aria-label="Viewer">
      {sides === undefined ? image(0) : (
        <>
          <section><ZoomGroupContext value={{ group, priority: 0 }}>{image(0)}</ZoomGroupContext></section>
          {sides === "pair" && <section><ZoomGroupContext value={{ group, priority: 1 }}>{image(1)}</ZoomGroupContext></section>}
        </>
      )}
    </div>,
    { platform: { kind: "browser", host } },
  );
  act(() => void vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  return { dialog: container.querySelector<HTMLElement>("[role=dialog]")!, onError };
}

/** Mounts one image and has it decoded at `width` x `height`. */
function mountLoaded(width: number, height: number, options: Parameters<typeof mountZoomable>[0] = {}): Mounted & { image: HTMLImageElement; scroller: HTMLElement } {
  const mounted = mountZoomable(options);
  const image = mounted.dialog.querySelector("img")!;
  load(image, width, height);
  return { ...mounted, image, scroller: image.parentElement! };
}

const bar = (dialog: HTMLElement, index = 0) => dialog.querySelectorAll<HTMLElement>('[role=group][aria-label="Zoom"]')[index] ?? null;
const readout = (dialog: HTMLElement, index = 0) => bar(dialog, index).querySelector('span[aria-hidden="true"]')!.textContent;
const control = (dialog: HTMLElement, name: string, index = 0) => bar(dialog, index).querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)!;
const drawnWidth = (image: HTMLImageElement) => parseFloat(image.style.width);
/** What a button or key zoom says: the percentage is isolated, so it keeps its own order in a right-to-left sentence. */
const spoken = (percent: string) => `Zoomed to ${isolate(percent)}`;
const statuses = (dialog: HTMLElement) => [...dialog.querySelectorAll("[role=status]")].map((region) => region.textContent).filter(Boolean);

/** Zoom in until the button is disabled, and report the readouts passed on the way. */
function zoomInToTheEnd(dialog: HTMLElement): string[] {
  const seen = [readout(dialog)];
  for (let guard = 0; guard < 40 && !control(dialog, "Zoom in").disabled; guard += 1) {
    activate(control(dialog, "Zoom in"));
    seen.push(readout(dialog));
  }
  return seen;
}

function keydown(target: Element, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  act(() => void target.dispatchEvent(event));
  return event;
}

function wheel(target: Element, init: WheelEventInit): WheelEvent {
  const event = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  act(() => void target.dispatchEvent(event));
  return event;
}

function pointer(target: Element, type: string, init: { pointerId?: number; clientX: number; clientY: number; button?: number }): void {
  act(() => void target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerId: 1, ...init })));
}

// ---------------------------------------------------------------------------------------------------------------------
// Opening, the bar and the bounds

it("shows no zoom bar until the image has loaded with a size of its own", () => {
  const { dialog } = mountZoomable();
  expect(bar(dialog)).toBeNull();
  // An SVG with only a `viewBox` has no size of its own: it stays as the browser fits it, unzoomable.
  load(dialog.querySelector("img")!, 0, 0);
  expect(bar(dialog)).toBeNull();
  load(dialog.querySelector("img")!, 300, 200);
  expect(bar(dialog)).not.toBeNull();
});

it("calls onError when the image cannot be decoded", () => {
  const { dialog, onError } = mountZoomable();
  act(() => void dialog.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(onError).toHaveBeenCalledTimes(1);
});

it("opens a large image fitted to the view, less its padding", () => {
  const { dialog, image } = mountLoaded(2000, 1000);
  expect(drawnWidth(image)).toBeCloseTo(1000);
  expect(parseFloat(image.style.height)).toBeCloseTo(500);
  expect(readout(dialog)).toBe("50%");
});

it("opens a small image at its actual size, never scaled up", () => {
  const { dialog, image } = mountLoaded(200, 100);
  expect(drawnWidth(image)).toBe(200);
  expect(parseFloat(image.style.height)).toBe(100);
  expect(readout(dialog)).toBe("100%");
});

it("names the bar's group Zoom and each control for what it does, with a tooltip", async () => {
  await tooltipsCooledDown();
  const { dialog } = mountLoaded(2000, 1000);
  expect(bar(dialog).getAttribute("role")).toBe("group");
  expect(control(dialog, "Zoom out")).not.toBeNull();
  expect(control(dialog, "Zoom in")).not.toBeNull();
  expect(control(dialog, "Actual size")).not.toBeNull();
  expect([...bar(dialog).querySelectorAll("button")]).toHaveLength(3);
  enter(control(dialog, "Zoom in"));
  act(() => void vi.advanceTimersByTime(CONTROL_TOOLTIP_DELAY_MS));
  expect(openTooltip()?.textContent).toBe("Zoom in");
});

it("keeps fitting while fitted as the view resizes, and keeps a zoom the view's growth does not overtake", () => {
  const { dialog, image } = mountLoaded(2000, 1000);
  layout.width = 516;
  layout.height = 516;
  resizeScrollers();
  // The box is now 500 x 500: the width is the tight side.
  expect(drawnWidth(image)).toBeCloseTo(500);
  expect(readout(dialog)).toBe("25%");

  // Zoomed to actual size, well above the fitted scale of either size of view.
  for (const percent of ["33%", "50%", "67%", "75%", "100%"]) {
    activate(control(dialog, "Zoom in"));
    expect(readout(dialog)).toBe(percent);
  }
  layout.width = 1016;
  resizeScrollers();
  expect(readout(dialog)).toBe("100%");
  expect(drawnWidth(image)).toBeCloseTo(2000);
});

it("fits again when the view grows until the fitted scale passes a chosen zoom", () => {
  const { dialog, image } = mountLoaded(2000, 1000);
  layout.width = 516;
  layout.height = 516;
  resizeScrollers();
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("33%");
  expect(control(dialog, "Zoom to fit")).not.toBeNull();
  // The fitted scale is 50% in the larger view, above the 33% chosen: nothing can be smaller than fitted.
  layout.width = 1016;
  resizeScrollers();
  expect(readout(dialog)).toBe("50%");
  expect(drawnWidth(image)).toBeCloseTo(1000);
  expect(control(dialog, "Actual size")).not.toBeNull();
  // And it follows the view from there.
  layout.width = 516;
  resizeScrollers();
  expect(drawnWidth(image)).toBeCloseTo(500);
});

it("fits the image to a view that has not been laid out yet at its actual size", () => {
  layout.width = 0;
  layout.height = 0;
  const { image } = mountLoaded(2000, 1000);
  expect(drawnWidth(image)).toBe(2000);
});

// ---------------------------------------------------------------------------------------------------------------------
// Buttons

it("steps Zoom in through the fixed levels up to 3200% and disables it there", () => {
  const { dialog } = mountLoaded(2000, 1000);
  expect(control(dialog, "Zoom out").disabled).toBe(true);
  expect(control(dialog, "Zoom in").disabled).toBe(false);
  expect(zoomInToTheEnd(dialog)).toEqual(["50%", "67%", "75%", "100%", "125%", "150%", "200%", "300%", "400%", "600%", "800%", "1,200%", "1,600%", "2,400%", "3,200%"]);
  expect(control(dialog, "Zoom in").disabled).toBe(true);
  expect(control(dialog, "Zoom out").disabled).toBe(false);
});

it("steps Zoom out back down to the fitted scale, where it is disabled again and the image fits once more", () => {
  const { dialog, image } = mountLoaded(2000, 1000);
  activate(control(dialog, "Zoom in"));
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("75%");
  activate(control(dialog, "Zoom out"));
  expect(readout(dialog)).toBe("67%");
  activate(control(dialog, "Zoom out"));
  expect(readout(dialog)).toBe("50%");
  expect(control(dialog, "Zoom out").disabled).toBe(true);
  // Back to fitted mode: the fit button offers actual size again, and the image follows the view.
  expect(control(dialog, "Actual size")).not.toBeNull();
  layout.width = 516;
  resizeScrollers();
  expect(drawnWidth(image)).toBeCloseTo(500);
});

it("makes the fitted scale a stop that Zoom in passes over when the next level is too near", () => {
  // Fits at 31%: the 33% level beside it is less than a tenth more, so Zoom in goes on to 50%.
  layout.width = 266;
  layout.height = 266;
  const { dialog } = mountLoaded(800, 800);
  expect(readout(dialog)).toBe("31%");
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("50%");
  // Back down it stops at the 33% level, which is more than a tenth under 50%, and then at the fitted scale.
  activate(control(dialog, "Zoom out"));
  expect(readout(dialog)).toBe("33%");
  activate(control(dialog, "Zoom out"));
  expect(readout(dialog)).toBe("31%");
  expect(control(dialog, "Zoom out").disabled).toBe(true);
});

it("caps the largest scale so the longest drawn side stays within 65536 px, with the last step landing on it", () => {
  const { dialog, image } = mountLoaded(10000, 5000);
  const seen = zoomInToTheEnd(dialog);
  expect(seen.at(-1)).toBe("655%");
  expect(drawnWidth(image)).toBeCloseTo(65536);
  expect(control(dialog, "Zoom in").disabled).toBe(true);
});

it("always reaches actual size, however large the image", () => {
  const { dialog, image } = mountLoaded(200000, 1000);
  const seen = zoomInToTheEnd(dialog);
  expect(seen.at(-1)).toBe("100%");
  expect(drawnWidth(image)).toBeCloseTo(200000);
});

it("goes from fitted to actual size with the fit button, and back to fitted with it", () => {
  const { dialog, image } = mountLoaded(2000, 1000);
  expect(control(dialog, "Actual size").disabled).toBe(false);
  activate(control(dialog, "Actual size"));
  expect(readout(dialog)).toBe("100%");
  expect(drawnWidth(image)).toBeCloseTo(2000);
  expect(control(dialog, "Actual size")).toBeNull();
  activate(control(dialog, "Zoom to fit"));
  expect(readout(dialog)).toBe("50%");
  expect(drawnWidth(image)).toBeCloseTo(1000);
  expect(control(dialog, "Zoom to fit")).toBeNull();
});

it("disables the fit button while fitted when the image already fits at actual size, and offers fit once zoomed in", () => {
  const { dialog } = mountLoaded(200, 100);
  expect(control(dialog, "Actual size").disabled).toBe(true);
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("125%");
  expect(control(dialog, "Zoom to fit").disabled).toBe(false);
  activate(control(dialog, "Zoom to fit"));
  expect(readout(dialog)).toBe("100%");
  expect(control(dialog, "Actual size").disabled).toBe(true);
});

it("formats the percentage for the language", () => {
  setLanguage("de");
  const { dialog } = mountLoaded(2000, 1000);
  // The group and its controls are named in the language too: the buttons are, in order, Zoom out, Zoom in and the fit one.
  const group = dialog.querySelector("[role=group]")!;
  const zoomIn = group.querySelectorAll("button")[1];
  for (let press = 0; press < 4; press += 1) activate(zoomIn);
  const shown = group.querySelector('span[aria-hidden="true"]')!.textContent;
  expect(shown).toBe(new Intl.NumberFormat("de", { style: "percent" }).format(1.25));
  expect(shown).not.toBe("125%");
});

// ---------------------------------------------------------------------------------------------------------------------
// Announcements

it("announces a change made by a button or key, and empties the region once it has been read", () => {
  const { dialog } = mountLoaded(2000, 1000);
  activate(control(dialog, "Zoom in"));
  expect(statuses(dialog)).toEqual([spoken("67%")]);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  expect(statuses(dialog)).toEqual([]);

  keydown(dialog, { key: "+", code: "Equal", ctrlKey: true });
  expect(statuses(dialog)).toEqual([spoken("75%")]);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  activate(control(dialog, "Zoom to fit"));
  expect(statuses(dialog)).toEqual([spoken("50%")]);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  activate(control(dialog, "Actual size"));
  expect(statuses(dialog)).toEqual([spoken("100%")]);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  keydown(dialog, { key: "0", code: "Digit0", ctrlKey: true });
  expect(statuses(dialog)).toEqual([spoken("50%")]);
});

it("does not announce a change made by the wheel or a pinch", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  wheel(scroller, { deltaY: -10, ctrlKey: true, clientX: 100, clientY: 100 });
  expect(readout(dialog)).not.toBe("50%");
  act(() => {
    scroller.dispatchEvent(new Event("gesturestart", { cancelable: true }));
    scroller.dispatchEvent(Object.assign(new Event("gesturechange", { cancelable: true }), { scale: 1.5, clientX: 100, clientY: 100 }));
  });
  expect(statuses(dialog)).toEqual([]);
});

// ---------------------------------------------------------------------------------------------------------------------
// Keyboard

it.each([
  ["Ctrl+= zooms in", WINDOWS_HOST, { key: "=", code: "Equal", ctrlKey: true }, "67%"],
  ["Ctrl++ (Shift) zooms in", WINDOWS_HOST, { key: "+", code: "Equal", ctrlKey: true, shiftKey: true }, "67%"],
  ["Ctrl+numpad plus zooms in", WINDOWS_HOST, { key: "+", code: "NumpadAdd", ctrlKey: true }, "67%"],
  ["Cmd+= zooms in on a Mac", MAC_HOST, { key: "=", code: "Equal", metaKey: true }, "67%"],
])("%s", (_, host, init, expected) => {
  const { dialog } = mountLoaded(2000, 1000, { host });
  const event = keydown(dialog, init);
  expect(readout(dialog)).toBe(expected);
  expect(event.defaultPrevented).toBe(true);
});

it.each([
  ["Ctrl+- zooms out", WINDOWS_HOST, { key: "-", code: "Minus", ctrlKey: true }],
  ["Ctrl+numpad minus zooms out", WINDOWS_HOST, { key: "-", code: "NumpadSubtract", ctrlKey: true }],
  ["Cmd+- zooms out on a Mac", MAC_HOST, { key: "-", code: "Minus", metaKey: true }],
])("%s", (_, host, init) => {
  const { dialog } = mountLoaded(2000, 1000, { host });
  activate(control(dialog, "Actual size"));
  expect(readout(dialog)).toBe("100%");
  const event = keydown(dialog, init);
  expect(readout(dialog)).toBe("75%");
  expect(event.defaultPrevented).toBe(true);
});

it.each([
  ["Ctrl+0", WINDOWS_HOST, { key: "0", code: "Digit0", ctrlKey: true }],
  ["Ctrl+numpad 0", WINDOWS_HOST, { key: "0", code: "Numpad0", ctrlKey: true }],
  ["Cmd+0", MAC_HOST, { key: "0", code: "Digit0", metaKey: true }],
])("returns to fitted on %s", (_, host, init) => {
  const { dialog, image } = mountLoaded(2000, 1000, { host });
  activate(control(dialog, "Actual size"));
  expect(readout(dialog)).toBe("100%");
  const event = keydown(dialog, init);
  expect(readout(dialog)).toBe("50%");
  expect(drawnWidth(image)).toBeCloseTo(1000);
  expect(event.defaultPrevented).toBe(true);
});

it.each([
  ["Alt held", WINDOWS_HOST, { key: "=", code: "Equal", ctrlKey: true, altKey: true }],
  ["no modifier", WINDOWS_HOST, { key: "=", code: "Equal" }],
  ["Cmd on a platform whose modifier is Ctrl", WINDOWS_HOST, { key: "=", code: "Equal", metaKey: true }],
  ["Ctrl on a Mac", MAC_HOST, { key: "=", code: "Equal", ctrlKey: true }],
  ["both modifiers on a Mac", MAC_HOST, { key: "=", code: "Equal", metaKey: true, ctrlKey: true }],
  ["both modifiers elsewhere", WINDOWS_HOST, { key: "=", code: "Equal", metaKey: true, ctrlKey: true }],
  ["an unrelated key", WINDOWS_HOST, { key: "a", code: "KeyA", ctrlKey: true }],
])("ignores a zoom key with %s and leaves it to the page", (_, host, init) => {
  const { dialog } = mountLoaded(2000, 1000, { host });
  const event = keydown(dialog, init);
  expect(readout(dialog)).toBe("50%");
  expect(event.defaultPrevented).toBe(false);
});

it("zooms from a key pressed anywhere in the viewer dialog", () => {
  const { dialog } = mountLoaded(2000, 1000);
  const elsewhere = dialog.appendChild(document.createElement("button"));
  keydown(elsewhere, { key: "=", code: "Equal", ctrlKey: true });
  expect(readout(dialog)).toBe("67%");
});

// ---------------------------------------------------------------------------------------------------------------------
// Wheel and pinch

it("zooms continuously around the pointer on Ctrl+wheel, so the image point under it stays put", () => {
  const { dialog, image, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => void (scroller.scrollLeft = 0));
  act(() => void (scroller.scrollTop = 0));
  const event = wheel(scroller, { deltaY: -10, deltaMode: 0, ctrlKey: true, clientX: 300, clientY: 200 });
  const factor = Math.exp(0.1);
  expect(event.defaultPrevented).toBe(true);
  expect(drawnWidth(image)).toBeCloseTo(2000 * factor);
  expect(readout(dialog)).toBe("111%");
  // The image point that was at 300 (at scale 1) is at 300 * factor now, and is brought back under the pointer.
  expect(scroller.scrollLeft).toBeCloseTo(300 * factor - 300);
  expect(scroller.scrollTop).toBeCloseTo(200 * factor - 200);
});

it("treats a pinch (Ctrl+wheel) and Cmd+wheel alike", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  expect(wheel(scroller, { deltaY: -10, metaKey: true, clientX: 0, clientY: 0 }).defaultPrevented).toBe(true);
  expect(readout(dialog)).toBe("111%");
});

it("leaves the plain wheel to native scrolling", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  const event = wheel(scroller, { deltaY: -100, clientX: 10, clientY: 10 });
  expect(event.defaultPrevented).toBe(false);
  expect(readout(dialog)).toBe("100%");
});

it.each([
  ["a pixel delta", { deltaY: -10, deltaMode: 0 }, Math.exp(0.1)],
  ["a pixel delta capped at 20", { deltaY: -300, deltaMode: 0 }, Math.exp(0.2)],
  ["a line delta of one line (16 px)", { deltaY: -1, deltaMode: 1 }, Math.exp(0.16)],
  ["a page delta, capped", { deltaY: -1, deltaMode: 2 }, Math.exp(0.2)],
  ["a downward delta", { deltaY: 15, deltaMode: 0 }, Math.exp(-0.15)],
])("scales by the capped delta of %s", (_, init, factor) => {
  const { image, dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  wheel(scroller, { ...init, ctrlKey: true, clientX: 0, clientY: 0 });
  expect(drawnWidth(image)).toBeCloseTo(2000 * factor);
});

it("does not zoom out past the fitted scale, nor in past the largest", () => {
  const { dialog, image, scroller } = mountLoaded(2000, 1000);
  wheel(scroller, { deltaY: 20, ctrlKey: true, clientX: 0, clientY: 0 });
  expect(readout(dialog)).toBe("50%");
  expect(control(dialog, "Actual size")).not.toBeNull();
  for (let index = 0; index < 100; index += 1) wheel(scroller, { deltaY: -20, ctrlKey: true, clientX: 0, clientY: 0 });
  expect(readout(dialog)).toBe("3,200%");
  expect(drawnWidth(image)).toBeCloseTo(64000);
});

it("zooms relative to the scale at the start of a Safari gesture", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  const start = new Event("gesturestart", { cancelable: true });
  act(() => void scroller.dispatchEvent(start));
  expect(start.defaultPrevented).toBe(true);
  const change = (scale: number) => {
    const event = Object.assign(new Event("gesturechange", { cancelable: true }), { scale, clientX: 10, clientY: 10 });
    act(() => void scroller.dispatchEvent(event));
    return event;
  };
  expect(change(1.5).defaultPrevented).toBe(true);
  expect(readout(dialog)).toBe("150%");
  change(2);
  expect(readout(dialog)).toBe("200%");
  change(1);
  expect(readout(dialog)).toBe("100%");
});

// ---------------------------------------------------------------------------------------------------------------------
// Panning and pinching on touch

it("pans an image larger than the view by dragging with the primary button, the scroll moving against the drag", () => {
  const { scroller, dialog } = mountLoaded(2000, 1000);
  // Fitted at 50% the image does not overflow: zoom in first.
  activate(control(dialog, "Actual size"));
  act(() => void (scroller.scrollLeft = 200));
  act(() => void (scroller.scrollTop = 100));
  pointer(scroller, "pointerdown", { clientX: 500, clientY: 300 });
  pointer(scroller, "pointermove", { clientX: 450, clientY: 320 });
  expect(scroller.scrollLeft).toBe(250);
  expect(scroller.scrollTop).toBe(80);
  pointer(scroller, "pointerup", { clientX: 450, clientY: 320 });
  pointer(scroller, "pointermove", { clientX: 400, clientY: 400 });
  expect(scroller.scrollLeft).toBe(250);
});

it("does not pan with a button other than the primary one", () => {
  const { scroller, dialog } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => void (scroller.scrollLeft = 200));
  pointer(scroller, "pointerdown", { clientX: 500, clientY: 300, button: 2 });
  pointer(scroller, "pointermove", { clientX: 450, clientY: 300 });
  expect(scroller.scrollLeft).toBe(200);
});

it("does not take a press on the scrollbar for a drag", () => {
  const { scroller, dialog } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  // The scroller's client area is narrower than its box when a scrollbar takes the room.
  const client = vi.spyOn(Element.prototype, "clientWidth", "get").mockImplementation(function (this: Element) { return this === scroller ? layout.width - 15 : 0; });
  onTestFinished(() => client.mockRestore());
  act(() => void (scroller.scrollLeft = 200));
  pointer(scroller, "pointerdown", { clientX: layout.width - 5, clientY: 300 });
  pointer(scroller, "pointermove", { clientX: layout.width - 105, clientY: 300 });
  expect(scroller.scrollLeft).toBe(200);
});

it("does not pan an image that fits", () => {
  const { scroller } = mountLoaded(200, 100);
  act(() => void (scroller.scrollLeft = 0));
  pointer(scroller, "pointerdown", { clientX: 500, clientY: 300 });
  pointer(scroller, "pointermove", { clientX: 400, clientY: 300 });
  expect(scroller.scrollLeft).toBe(0);
});

it("pinch-zooms with two touch pointers by the change in the distance between them", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  pointer(scroller, "pointerdown", { pointerId: 1, clientX: 400, clientY: 300 });
  pointer(scroller, "pointerdown", { pointerId: 2, clientX: 600, clientY: 300 });
  // The distance between the fingers doubles: 200 to 400.
  pointer(scroller, "pointermove", { pointerId: 2, clientX: 800, clientY: 300 });
  expect(readout(dialog)).toBe("200%");
  pointer(scroller, "pointerup", { pointerId: 2, clientX: 800, clientY: 300 });
  pointer(scroller, "pointerup", { pointerId: 1, clientX: 400, clientY: 300 });
});

it("does not pinch-zoom with a single pointer", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  pointer(scroller, "pointerdown", { pointerId: 1, clientX: 400, clientY: 300 });
  pointer(scroller, "pointermove", { pointerId: 1, clientX: 800, clientY: 300 });
  expect(readout(dialog)).toBe("100%");
});

// ---------------------------------------------------------------------------------------------------------------------
// Double-click

it("goes from fitted to actual size on a double-click, and from zoomed to fitted on the next", () => {
  const { dialog, image, scroller } = mountLoaded(2000, 1000);
  const doubleClick = () => act(() => void scroller.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 100, clientY: 100 })));
  doubleClick();
  expect(readout(dialog)).toBe("100%");
  expect(drawnWidth(image)).toBeCloseTo(2000);
  doubleClick();
  expect(readout(dialog)).toBe("50%");
  expect(drawnWidth(image)).toBeCloseTo(1000);
});

it("returns a zoomed-in image that is smaller than the view to fitted on a double-click", () => {
  const { dialog, scroller } = mountLoaded(200, 100);
  const doubleClick = () => act(() => void scroller.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 100, clientY: 100 })));
  // Fitted at actual size already: there is nowhere to go.
  doubleClick();
  expect(readout(dialog)).toBe("100%");
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("125%");
  doubleClick();
  expect(readout(dialog)).toBe("100%");
});

// ---------------------------------------------------------------------------------------------------------------------
// The scroller

it("is a focusable region named by the file only while the image overflows the view", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000, { name: "diagram.png" });
  expect(scroller.getAttribute("role")).toBeNull();
  expect(scroller.getAttribute("aria-label")).toBeNull();
  expect(scroller.hasAttribute("tabindex")).toBe(false);

  activate(control(dialog, "Actual size"));
  expect(scroller.getAttribute("role")).toBe("region");
  expect(scroller.getAttribute("aria-label")).toBe("diagram.png");
  expect(scroller.tabIndex).toBe(0);

  activate(control(dialog, "Zoom to fit"));
  expect(scroller.getAttribute("role")).toBeNull();
  expect(scroller.hasAttribute("tabindex")).toBe(false);
});

it("is no region or tab stop for an image that fits", () => {
  const { scroller } = mountLoaded(200, 100);
  expect(scroller.getAttribute("role")).toBeNull();
  expect(scroller.hasAttribute("tabindex")).toBe(false);
});

it("is laid out left to right whatever the direction of the interface", () => {
  document.documentElement.dir = "rtl";
  onTestFinished(() => void document.documentElement.removeAttribute("dir"));
  const { scroller } = mountLoaded(2000, 1000);
  expect(scroller.getAttribute("dir")).toBe("ltr");
});

it("draws the image pixelated above actual size and smoothed at or below it", () => {
  const { dialog, image } = mountLoaded(2000, 1000);
  const pixelated = () => image.className.includes("image-rendering:pixelated");
  expect(readout(dialog)).toBe("50%");
  expect(pixelated()).toBe(false);
  activate(control(dialog, "Actual size"));
  expect(readout(dialog)).toBe("100%");
  expect(pixelated()).toBe(false);
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("125%");
  expect(pixelated()).toBe(true);
  activate(control(dialog, "Zoom out"));
  expect(pixelated()).toBe(false);
});

// ---------------------------------------------------------------------------------------------------------------------
// The two sides of an image change: one group, led by the After side

interface Pair {
  dialog: HTMLElement;
  before: HTMLImageElement;
  after: HTMLImageElement;
  /** The scrollers, Before then After. */
  scrollers: [HTMLElement, HTMLElement];
}

/** Mounts a Before and an After image in one group, decoded at the given natural sizes. */
function mountPair(beforeSize: [number, number], afterSize: [number, number], options: Parameters<typeof mountZoomable>[0] = {}): Pair & { onError: Mounted["onError"] } {
  const { dialog, onError } = mountZoomable({ ...options, sides: "pair" });
  const [before, after] = dialog.querySelectorAll("img");
  load(before, ...beforeSize);
  load(after, ...afterSize);
  return { dialog, onError, before, after, scrollers: [before.parentElement!, after.parentElement!] };
}

/** The scale an image is drawn at, from its drawn width and natural width. */
const scaleDrawn = (image: HTMLImageElement, naturalWidth: number) => drawnWidth(image) / naturalWidth;

it("shows one bar for a change, over the After side, which tells the After side's scale", () => {
  const { dialog, before, after } = mountPair([2000, 1000], [4000, 2000]);
  expect(dialog.querySelectorAll("[role=group]")).toHaveLength(1);
  const sections = dialog.querySelectorAll("section");
  expect(sections[1].contains(bar(dialog))).toBe(true);
  expect(sections[0].querySelector("[role=group]")).toBeNull();
  // Each side fits its own view: the After side at 25%, the Before side at 50%.
  expect(drawnWidth(before)).toBeCloseTo(1000);
  expect(drawnWidth(after)).toBeCloseTo(1000);
  expect(readout(dialog)).toBe("25%");
});

it("leads with the Before side when there is no After image, as for a deleted file", () => {
  const { dialog } = mountZoomable({ sides: "before" });
  const before = dialog.querySelector("img")!;
  expect(bar(dialog)).toBeNull();
  load(before, 2000, 1000);
  expect(dialog.querySelector("section")!.contains(bar(dialog))).toBe(true);
  expect(readout(dialog)).toBe("50%");
});

it("zooms both sides at proportional scales with one step, and announces it once", () => {
  const { dialog, before, after } = mountPair([2000, 1000], [4000, 2000]);
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("33%");
  // The After side is the leader's own scale; the Before side keeps its proportion to its own view: 0.33 * 0.5 / 0.25.
  expect(scaleDrawn(after, 4000)).toBeCloseTo(0.33);
  expect(scaleDrawn(before, 2000)).toBeCloseTo(0.66);
  expect(statuses(dialog)).toEqual([spoken("33%")]);
});

it("takes a zoom key once for the whole change, whichever side holds focus", () => {
  const { dialog, before, after } = mountPair([2000, 1000], [4000, 2000]);
  const event = keydown(before, { key: "=", code: "Equal", ctrlKey: true });
  expect(event.defaultPrevented).toBe(true);
  // One step, not one for each side.
  expect(readout(dialog)).toBe("33%");
  expect(scaleDrawn(before, 2000)).toBeCloseTo(0.66);
  expect(scaleDrawn(after, 4000)).toBeCloseTo(0.33);
  expect(statuses(dialog)).toEqual([spoken("33%")]);
  keydown(after, { key: "0", code: "Digit0", ctrlKey: true });
  expect(readout(dialog)).toBe("25%");
  expect(drawnWidth(before)).toBeCloseTo(1000);
  expect(drawnWidth(after)).toBeCloseTo(1000);
});

it("zooms in as far as the side that reaches its largest scale first, in the After side's terms", () => {
  // The After side may be drawn at 16.384x (65536 px), the Before side at 32x, which is 16x in the After side's terms.
  const { dialog, before, after } = mountPair([2000, 1000], [4000, 2000]);
  const seen = zoomInToTheEnd(dialog);
  expect(seen.at(-1)).toBe("1,600%");
  expect(control(dialog, "Zoom in").disabled).toBe(true);
  expect(scaleDrawn(after, 4000)).toBeCloseTo(16);
  expect(scaleDrawn(before, 2000)).toBeCloseTo(32);
});

it("goes to actual size, meaning the After side's, and back to fitted, from either side", () => {
  const { dialog, before, after, scrollers } = mountPair([2000, 1000], [4000, 2000]);
  act(() => void scrollers[0].dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 100, clientY: 100 })));
  expect(readout(dialog)).toBe("100%");
  expect(scaleDrawn(after, 4000)).toBeCloseTo(1);
  expect(scaleDrawn(before, 2000)).toBeCloseTo(2);
  expect(control(dialog, "Zoom to fit")).not.toBeNull();
  act(() => void scrollers[1].dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 100, clientY: 100 })));
  expect(readout(dialog)).toBe("25%");
  expect(drawnWidth(before)).toBeCloseTo(1000);
  expect(drawnWidth(after)).toBeCloseTo(1000);
});

it.each([0, 1])("zooms the whole change on Ctrl+wheel over side %s, by the same factor", (side) => {
  const { dialog, before, after, scrollers } = mountPair([2000, 1000], [4000, 2000]);
  const event = wheel(scrollers[side], { deltaY: -10, ctrlKey: true, clientX: 300, clientY: 200 });
  const factor = Math.exp(0.1);
  expect(event.defaultPrevented).toBe(true);
  expect(scaleDrawn(after, 4000)).toBeCloseTo(0.25 * factor);
  expect(scaleDrawn(before, 2000)).toBeCloseTo(0.5 * factor);
  expect(readout(dialog)).toBe("28%");
});

it("zooms the whole change by a Safari gesture over either side, by the ratio of successive scales", () => {
  const { dialog, after, scrollers } = mountPair([2000, 1000], [4000, 2000]);
  activate(control(dialog, "Actual size"));
  act(() => void scrollers[0].dispatchEvent(new Event("gesturestart", { cancelable: true })));
  const change = (scale: number) => act(() => void scrollers[0].dispatchEvent(Object.assign(new Event("gesturechange", { cancelable: true }), { scale, clientX: 10, clientY: 10 })));
  change(1.5);
  expect(readout(dialog)).toBe("150%");
  change(2);
  expect(readout(dialog)).toBe("200%");
  expect(scaleDrawn(after, 4000)).toBeCloseTo(2);
});

it("pinch-zooms the whole change with two touch pointers on either side", () => {
  const { dialog, before, after, scrollers } = mountPair([2000, 1000], [4000, 2000]);
  activate(control(dialog, "Actual size"));
  pointer(scrollers[0], "pointerdown", { pointerId: 1, clientX: 400, clientY: 300 });
  pointer(scrollers[0], "pointerdown", { pointerId: 2, clientX: 600, clientY: 300 });
  pointer(scrollers[0], "pointermove", { pointerId: 2, clientX: 800, clientY: 300 });
  expect(readout(dialog)).toBe("200%");
  expect(scaleDrawn(after, 4000)).toBeCloseTo(2);
  expect(scaleDrawn(before, 2000)).toBeCloseTo(4);
});

it("scrolls the other side to the same fraction of its own scroll range, whichever side scrolls", () => {
  // Different shapes: the Before image is twice as wide as high, the After image is square.
  const { dialog, scrollers } = mountPair([2000, 1000], [1000, 1000]);
  const [before, after] = scrollers;
  activate(control(dialog, "Actual size"));
  activate(control(dialog, "Zoom in"));
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("150%");

  // How far along its range a scroller stands: its image and padding make the content, its box the view.
  const fraction = (scroller: HTMLElement) => {
    const image = scroller.querySelector("img")!;
    return {
      x: scroller.scrollLeft / (drawnWidth(image) + 16 - layout.width),
      y: scroller.scrollTop / (parseFloat(image.style.height) + 16 - layout.height),
    };
  };
  act(() => void (before.scrollLeft = 900));
  act(() => void (before.scrollTop = 350));
  act(() => void before.dispatchEvent(new Event("scroll")));
  expect(fraction(after).x).toBeCloseTo(fraction(before).x);
  expect(fraction(after).y).toBeCloseTo(fraction(before).y);
  expect(after.scrollLeft).not.toBe(0);

  // And the other way; the side that was brought along does not send it back.
  act(() => void (after.scrollLeft = 100));
  act(() => void (after.scrollTop = 40));
  act(() => void after.dispatchEvent(new Event("scroll")));
  expect(fraction(before).x).toBeCloseTo(fraction(after).x);
  expect(fraction(before).y).toBeCloseTo(fraction(after).y);
  const settled = [before.scrollLeft, before.scrollTop, after.scrollLeft, after.scrollTop];
  act(() => void before.dispatchEvent(new Event("scroll")));
  act(() => void after.dispatchEvent(new Event("scroll")));
  expect([before.scrollLeft, before.scrollTop, after.scrollLeft, after.scrollTop]).toEqual(settled);
});

it("brings the other side to its edge when one side reaches its own", () => {
  const { dialog, scrollers } = mountPair([2000, 1000], [1000, 1000]);
  const [before, after] = scrollers;
  activate(control(dialog, "Actual size"));
  activate(control(dialog, "Zoom in"));
  const range = (scroller: HTMLElement) => {
    const image = scroller.querySelector("img")!;
    return { left: drawnWidth(image) + 16 - layout.width, top: parseFloat(image.style.height) + 16 - layout.height };
  };
  act(() => void (before.scrollLeft = range(before).left));
  act(() => void (before.scrollTop = range(before).top));
  act(() => void before.dispatchEvent(new Event("scroll")));
  expect(after.scrollLeft).toBeCloseTo(range(after).left);
  expect(after.scrollTop).toBeCloseTo(range(after).top);
});

it("drags either side to pan both", () => {
  const { dialog, scrollers } = mountPair([2000, 1000], [2000, 1000]);
  activate(control(dialog, "Actual size"));
  const [before, after] = scrollers;
  act(() => void (before.scrollLeft = 200));
  act(() => void before.dispatchEvent(new Event("scroll")));
  pointer(before, "pointerdown", { clientX: 500, clientY: 300 });
  pointer(before, "pointermove", { clientX: 400, clientY: 300 });
  expect(before.scrollLeft).toBe(300);
  act(() => void before.dispatchEvent(new Event("scroll")));
  // The same size on both sides: the same scroll.
  expect(after.scrollLeft).toBeCloseTo(300);
});

it("keeps the point under the pointer under it on the side it is over, and brings the other side to the same place", () => {
  const { dialog, before, after, scrollers } = mountPair([2000, 1000], [2000, 1000]);
  activate(control(dialog, "Actual size"));
  act(() => void (scrollers[0].scrollLeft = 0));
  act(() => void (scrollers[1].scrollLeft = 0));
  wheel(scrollers[0], { deltaY: -10, ctrlKey: true, clientX: 300, clientY: 200 });
  const factor = Math.exp(0.1);
  expect(drawnWidth(before)).toBeCloseTo(2000 * factor);
  expect(drawnWidth(after)).toBeCloseTo(2000 * factor);
  expect(scrollers[0].scrollLeft).toBeCloseTo(300 * factor - 300);
  // The other side is placed by the relative position of its view's centre.
  const centre = (scroller: HTMLElement, image: HTMLImageElement) => (scroller.scrollLeft + layout.width / 2) / drawnWidth(image);
  expect(centre(scrollers[1], after)).toBeCloseTo(centre(scrollers[0], before));
});

/** Makes the scroller hold a position written beyond `range` at the edge, as a browser does. */
function clampScroll(scroller: HTMLElement, range: { left: number; top: number }): void {
  let left = 0;
  let top = 0;
  Object.defineProperty(scroller, "scrollLeft", { configurable: true, get: () => left, set: (value: number) => void (left = Math.min(range.left, Math.max(0, value))) });
  Object.defineProperty(scroller, "scrollTop", { configurable: true, get: () => top, set: (value: number) => void (top = Math.min(range.top, Math.max(0, value))) });
}

it("lets the side with the most room set how far the pair scrolls: a side that cannot follow does not pull the other back", () => {
  const { dialog, scrollers } = mountPair([2000, 1000], [2000, 1000]);
  const [before, after] = scrollers;
  clampScroll(before, { left: 1000, top: 500 });
  clampScroll(after, { left: 100, top: 500 });
  activate(control(dialog, "Actual size"));

  act(() => void (before.scrollLeft = 600));
  act(() => void before.dispatchEvent(new Event("scroll")));
  expect(after.scrollLeft).toBe(100);
  // The scroll event the clamped side raises is the group's own doing, not the reader's.
  act(() => void after.dispatchEvent(new Event("scroll")));
  expect(before.scrollLeft).toBe(600);
  expect(after.scrollLeft).toBe(100);
});

it("follows only the axis the reader scrolled", () => {
  const { dialog, scrollers } = mountPair([2000, 1000], [2000, 1000]);
  const [before, after] = scrollers;
  activate(control(dialog, "Actual size"));
  // A first event records where Before stands; both axes follow it.
  act(() => void (before.scrollLeft = 200));
  act(() => void (before.scrollTop = 100));
  act(() => void before.dispatchEvent(new Event("scroll")));
  expect(after.scrollLeft).toBeCloseTo(200);
  expect(after.scrollTop).toBeCloseTo(100);

  act(() => void (after.scrollTop = 30));
  act(() => void (before.scrollLeft = 400));
  act(() => void before.dispatchEvent(new Event("scroll")));
  expect(after.scrollLeft).toBeCloseTo(400);
  expect(after.scrollTop).toBe(30);
});

// ---------------------------------------------------------------------------------------------------------------------
// Announcements, revisited

it("empties the region when a zoom that is not announced follows, so a later button zoom to the same percentage is spoken again", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  keydown(dialog, { key: "=", code: "Equal", ctrlKey: true });
  expect(statuses(dialog)).toEqual([spoken("67%")]);
  // A wheel zoom is not announced, and the earlier sentence is no longer about where the image is.
  wheel(scroller, { deltaY: -10, ctrlKey: true, clientX: 100, clientY: 100 });
  expect(readout(dialog)).toBe("74%");
  expect(statuses(dialog)).toEqual([]);
  activate(control(dialog, "Zoom out"));
  expect(readout(dialog)).toBe("67%");
  expect(statuses(dialog)).toEqual([spoken("67%")]);
});

it("announces nothing for a press or key that leaves the zoom where it was", () => {
  const { dialog } = mountLoaded(2000, 1000);
  // Already fitted.
  keydown(dialog, { key: "0", code: "Digit0", ctrlKey: true });
  keydown(dialog, { key: "-", code: "Minus", ctrlKey: true });
  expect(statuses(dialog)).toEqual([]);

  zoomInToTheEnd(dialog);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  expect(statuses(dialog)).toEqual([]);
  const event = keydown(dialog, { key: "=", code: "Equal", ctrlKey: true });
  expect(event.defaultPrevented).toBe(true);
  expect(readout(dialog)).toBe("3,200%");
  expect(statuses(dialog)).toEqual([]);
});

// ---------------------------------------------------------------------------------------------------------------------
// Focus

const focused = () => document.activeElement;

it("hands focus from Zoom in to Zoom out when a press leaves Zoom in disabled at the largest scale", () => {
  const { dialog } = mountLoaded(2000, 1000);
  const zoomIn = control(dialog, "Zoom in");
  act(() => zoomIn.focus());
  for (let press = 0; press < 40 && !zoomIn.disabled; press += 1) {
    expect(focused()).toBe(zoomIn);
    activate(zoomIn);
  }
  expect(zoomIn.disabled).toBe(true);
  expect(readout(dialog)).toBe("3,200%");
  expect(focused()).toBe(control(dialog, "Zoom out"));
});

it("hands focus from Zoom out to Zoom in when a press leaves Zoom out disabled at the fitted scale", () => {
  const { dialog } = mountLoaded(2000, 1000);
  activate(control(dialog, "Zoom in"));
  const zoomOut = control(dialog, "Zoom out");
  act(() => zoomOut.focus());
  activate(zoomOut);
  expect(readout(dialog)).toBe("50%");
  expect(zoomOut.disabled).toBe(true);
  expect(focused()).toBe(control(dialog, "Zoom in"));
});

it("hands focus from the fit button to Zoom in when going back to fitted leaves it disabled, on an image that fits at actual size", () => {
  const { dialog } = mountLoaded(200, 100);
  activate(control(dialog, "Zoom in"));
  const fit = control(dialog, "Zoom to fit");
  act(() => fit.focus());
  activate(fit);
  expect(readout(dialog)).toBe("100%");
  expect(control(dialog, "Actual size").disabled).toBe(true);
  expect(focused()).toBe(control(dialog, "Zoom in"));
});

it("leaves focus where it is when a press does not disable the button that has it", () => {
  const { dialog } = mountLoaded(2000, 1000);
  const zoomIn = control(dialog, "Zoom in");
  act(() => zoomIn.focus());
  activate(zoomIn);
  expect(focused()).toBe(zoomIn);
});

it("draws the focus ring on the scroller's frame while the overflowing region has keyboard focus, and not otherwise", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  expect(scroller.getAttribute("role")).toBe("region");
  const frame = scroller.parentElement!;
  expect(scroller.className).toContain("outline-none");
  expect(frame.className).not.toContain("ring-focus");

  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    scroller.focus();
  });
  expect(focused()).toBe(scroller);
  expect(frame.className).toContain("ring-focus");
  act(() => scroller.blur());
  expect(frame.className).not.toContain("ring-focus");

  // Focused by a pointer: no ring.
  act(() => {
    document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse" }));
    scroller.focus();
  });
  expect(frame.className).not.toContain("ring-focus");
});

it("names the region of each side of a pair, like a lone image's, by the file", () => {
  const { dialog } = mountPair([2000, 1000], [2000, 1000], { name: "photo.png" });
  const [before, after] = dialog.querySelectorAll("img");
  activate(control(dialog, "Actual size"));
  for (const image of [before, after]) {
    const region = image.parentElement!;
    expect(region.getAttribute("role")).toBe("region");
    expect(region.getAttribute("aria-label")).toBe("photo.png");
    expect(region.hasAttribute("aria-labelledby")).toBe(false);
  }
});

// ---------------------------------------------------------------------------------------------------------------------
// Focus kept through any zoom

const bodyHasFocus = () => document.activeElement === document.body;

it("moves focus from the scroller to the bar when ⌘0 takes the image back to fitted, where the scroller stops being a tab stop", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => scroller.focus());
  expect(focused()).toBe(scroller);
  keydown(scroller, { key: "0", code: "Digit0", ctrlKey: true });
  expect(scroller.hasAttribute("tabindex")).toBe(false);
  // Fitted: Zoom out has nowhere to go, so the first enabled button is Zoom in.
  expect(focused()).toBe(control(dialog, "Zoom in"));
  expect(bodyHasFocus()).toBe(false);
});

it("moves focus from Zoom out to Zoom in as the key steps down to the fitted scale", () => {
  const { dialog } = mountLoaded(2000, 1000);
  activate(control(dialog, "Zoom in"));
  const zoomOut = control(dialog, "Zoom out");
  act(() => zoomOut.focus());
  keydown(zoomOut, { key: "-", code: "Minus", ctrlKey: true });
  expect(readout(dialog)).toBe("50%");
  expect(zoomOut.disabled).toBe(true);
  expect(focused()).toBe(control(dialog, "Zoom in"));
});

it("moves focus from Zoom in to Zoom out as the key steps up to the largest scale", () => {
  const { dialog } = mountLoaded(2000, 1000);
  const zoomIn = control(dialog, "Zoom in");
  act(() => zoomIn.focus());
  for (let press = 0; press < 40 && !zoomIn.disabled; press += 1) keydown(zoomIn, { key: "=", code: "Equal", ctrlKey: true });
  expect(zoomIn.disabled).toBe(true);
  expect(readout(dialog)).toBe("3,200%");
  expect(focused()).toBe(control(dialog, "Zoom out"));
});

it("moves focus from the scroller to the bar on a double-click that fits the image again", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => scroller.focus());
  act(() => void scroller.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 100, clientY: 100 })));
  expect(readout(dialog)).toBe("50%");
  expect(focused()).toBe(control(dialog, "Zoom in"));
});

it("leaves focus on the scroller while a zoom keeps it a tab stop", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => scroller.focus());
  keydown(scroller, { key: "=", code: "Equal", ctrlKey: true });
  expect(readout(dialog)).toBe("125%");
  expect(focused()).toBe(scroller);
});

it("writes the announcement only after the focus move has been announced when a zoom moved focus, and at once otherwise", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  act(() => scroller.focus());
  keydown(scroller, { key: "0", code: "Digit0", ctrlKey: true });
  expect(focused()).toBe(control(dialog, "Zoom in"));
  expect(statuses(dialog)).toEqual([]);
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(statuses(dialog)).toEqual([spoken("50%")]);
  act(() => void vi.advanceTimersByTime(READ_TIME_MS));
  expect(statuses(dialog)).toEqual([]);

  // Focus on Zoom in, which the press keeps enabled: spoken at once.
  activate(control(dialog, "Zoom in"));
  expect(statuses(dialog)).toEqual([spoken("67%")]);
});

it("drops a delayed announcement when the next zoom comes first", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => scroller.focus());
  keydown(scroller, { key: "0", code: "Digit0", ctrlKey: true });
  expect(statuses(dialog)).toEqual([]);
  // A wheel zoom, which is not announced, before the delay has passed.
  wheel(scroller, { deltaY: -10, ctrlKey: true, clientX: 100, clientY: 100 });
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(statuses(dialog)).toEqual([]);
});

// ---------------------------------------------------------------------------------------------------------------------
// Focus on the follower's side, and the ring after a key

it("hands focus from the Before side's scroller to the bar when ⌘0 takes the pair back to fitted, though the After side leads", () => {
  const { dialog, scrollers } = mountPair([2000, 1000], [2000, 1000]);
  const [before] = scrollers;
  activate(control(dialog, "Actual size"));
  expect(before.getAttribute("role")).toBe("region");
  act(() => before.focus());
  expect(focused()).toBe(before);
  keydown(before, { key: "0", code: "Digit0", ctrlKey: true });
  expect(readout(dialog)).toBe("50%");
  expect(before.hasAttribute("tabindex")).toBe(false);
  expect(focused()).toBe(control(dialog, "Zoom in"));
  expect(bodyHasFocus()).toBe(false);
});

it("hands focus from the Before side's scroller to the bar when ⌘− steps the pair down to fitted", () => {
  const { dialog, scrollers } = mountPair([2000, 1000], [2000, 1000]);
  const [before] = scrollers;
  activate(control(dialog, "Zoom in"));
  activate(control(dialog, "Zoom in"));
  activate(control(dialog, "Zoom in"));
  expect(readout(dialog)).toBe("100%");
  // Down 75%, 67% and fitted.
  act(() => before.focus());
  for (const percent of ["75%", "67%"]) {
    keydown(before, { key: "-", code: "Minus", ctrlKey: true });
    expect(readout(dialog)).toBe(percent);
    expect(focused()).toBe(before);
  }
  keydown(before, { key: "-", code: "Minus", ctrlKey: true });
  expect(readout(dialog)).toBe("50%");
  expect(before.hasAttribute("tabindex")).toBe(false);
  expect(focused()).toBe(control(dialog, "Zoom in"));
});

it("draws the focus ring on the button that takes focus from a key pressed after a pointer interaction", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => {
    document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse" }));
    scroller.focus();
  });
  expect(scroller.parentElement!.className).not.toContain("ring-focus");
  keydown(scroller, { key: "0", code: "Digit0", ctrlKey: true });
  const zoomIn = control(dialog, "Zoom in");
  expect(focused()).toBe(zoomIn);
  expect(zoomIn.hasAttribute("data-focus-visible")).toBe(true);
});

it("does not turn the focus ring on for a zoom key that changes nothing", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  zoomInToTheEnd(dialog);
  expect(readout(dialog)).toBe("3,200%");
  act(() => {
    document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse" }));
    scroller.focus();
  });
  expect(focused()).toBe(scroller);
  expect(scroller.hasAttribute("data-focus-visible")).toBe(false);
  const event = keydown(scroller, { key: "=", code: "Equal", ctrlKey: true });
  expect(event.defaultPrevented).toBe(true);
  expect(readout(dialog)).toBe("3,200%");
  expect(focused()).toBe(scroller);
  expect(scroller.hasAttribute("data-focus-visible")).toBe(false);
  expect(scroller.parentElement!.className).not.toContain("ring-focus");
});

it("does not turn the focus ring on for a zoom key that zooms without moving focus", () => {
  const { dialog, scroller } = mountLoaded(2000, 1000);
  activate(control(dialog, "Actual size"));
  act(() => {
    document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse" }));
    scroller.focus();
  });
  expect(scroller.hasAttribute("data-focus-visible")).toBe(false);
  keydown(scroller, { key: "=", code: "Equal", ctrlKey: true });
  expect(readout(dialog)).toBe("125%");
  expect(focused()).toBe(scroller);
  expect(scroller.hasAttribute("data-focus-visible")).toBe(false);
  expect(scroller.parentElement!.className).not.toContain("ring-focus");
});
