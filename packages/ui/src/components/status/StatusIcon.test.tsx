// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import type { ShownStatusKind, StatusKind } from "../../sessions/sessionStatus";
import { BUBBLE_SHIFT, HAND_SHIFT, shiftedViewBox } from "./opticalCentring";
import { StatusIcon } from "./StatusIcon";
import { enter, leave, mountForTooltip, openPopover, openTooltip, press, trigger } from "../hint/tooltipTestSupport";
import { SLEEP_PATHS } from "./sleepGlyph";
import { fakeTimers, runOutTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const STATUS_TEXT: Array<[StatusKind, string]> = [
  ["starting", "Starting"],
  ["working", "Working"],
  ["waiting_user", "Waiting for you"],
  ["idle", "Awaiting instructions"],
  ["dormant", "Dormant"],
  ["paused", "Paused"],
  ["background", "Working in background"],
  ["interrupted", "Interrupted"],
  ["unknown", "Status unknown"],
  ["archived", "Archived"],
];

it.each(STATUS_TEXT)("says what the %s glyph stands for, after the delay the pointer rests on it", (status, text) => {
  fakeTimers();
  const container = mountForTooltip(<StatusIcon status={status} decorative withTooltip />);

  enter(trigger(container));
  expect(openTooltip()).toBeNull();
  runOutTimers();
  expect(openTooltip()?.textContent).toBe(text);

  leave(trigger(container));
  runOutTimers();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it.each([
  ["paused", "Paused", "text-success-soft-foreground", (glyph: Element) => glyph.classList.contains("lucide-circle-pause")],
  // Dormant shares the waiting hand's orange; its own drawn "zz" tells it apart.
  ["dormant", "Dormant", "text-warning-soft-foreground", (glyph: Element) => [...glyph.querySelectorAll("g:first-child path")].map((path) => path.getAttribute("d")).join() === SLEEP_PATHS.join()],
] as const)("names the %s glyph and draws it as its own shape in its own colour", (status, name, colour, isItsShape) => {
  const container = mountForTooltip(<StatusIcon status={status} />);
  const slot = container.querySelector("[role=img]")!;
  expect(slot.getAttribute("aria-label")).toBe(name);
  const glyph = slot.firstElementChild!;
  expect(isItsShape(glyph)).toBe(true);
  expect(glyph.classList.contains(colour)).toBe(true);
});

it.each([
  ["starting", "Starting"],
  ["switching", "Switching account"],
  ["reconnecting", "Reconnecting"],
] as const)("draws %s as the grey spinner, decorative inside a slot that names the status", (status, name) => {
  for (const [small, size] of [[false, "size-4"], [true, "size-3.5"]] as const) {
    const container = mountForTooltip(<StatusIcon status={status} small={small} />);
    const slot = container.querySelector("[role=img]")!;
    expect(slot.getAttribute("aria-label")).toBe(name);
    expect(slot.className).toContain(size);
    const spinner = slot.querySelector("[data-slot=spinner]")!;
    expect(spinner).not.toBeNull();
    expect(spinner.getAttribute("aria-hidden")).toBe("true");
    expect(spinner.classList.contains("text-app-muted")).toBe(true);
    expect(spinner.classList.contains("spinner--sm")).toBe(true);
    if (small) expect(spinner.classList.contains("size-3.5")).toBe(true);
    // Nothing amber and no rotation of its own: the labels alone tell the three apart.
    expect(container.innerHTML).not.toMatch(/animate-spin|tint-warning-fg/);
  }
});

it("draws starting, switching and reconnecting alike, apart from the name they carry", () => {
  // The spinner's gradient ids come from `useId`, so they differ per mount; they are not part of what is drawn.
  const drawn = (status: "starting" | "switching" | "reconnecting") => mountForTooltip(<StatusIcon status={status} decorative />).innerHTML.replace(/ id="[^"]*"|url\(#[^)]*\)/g, "");
  expect(drawn("switching")).toBe(drawn("starting"));
  expect(drawn("reconnecting")).toBe(drawn("starting"));
});

it("draws a status this client does not know as a neutral dashed circle, still", () => {
  const container = mountForTooltip(<StatusIcon status="unknown" />);
  const slot = container.querySelector("[role=img]")!;
  expect(slot.getAttribute("aria-label")).toBe("Status unknown");
  const glyph = slot.firstElementChild!;
  expect(glyph.classList.contains("lucide-circle-dashed")).toBe(true);
  // Not the "?" of the help tips.
  expect(container.querySelector(".lucide-circle-help, .lucide-circle-question-mark")).toBeNull();
  expect(glyph.classList.contains("text-app-muted")).toBe(true);
  expect(container.innerHTML).not.toMatch(/animate-/);
});

it("keeps the paused glyph still", () => {
  expect(mountForTooltip(<StatusIcon status="paused" />).innerHTML).not.toMatch(/animate-/);
});

it("adds nothing to the accessible name of the row or trail a decorative glyph sits in", () => {
  const container = mountForTooltip(<StatusIcon status={"working"} decorative withTooltip />);
  expect(trigger(container).hasAttribute("aria-label")).toBe(false);
  expect(container.querySelector("[role=img]")).toBeNull();
  expect(container.querySelector("[aria-hidden=true]")).not.toBeNull();
});

it("names the tooltip's own trigger where the glyph is the only thing saying the status", () => {
  const container = mountForTooltip(<StatusIcon status={"interrupted"} withTooltip />);
  // The name sits on the trigger, the element the tooltip is linked to, and the glyph inside it is
  // hidden, so the status is announced once.
  expect(trigger(container).getAttribute("role")).toBe("img");
  expect(trigger(container).getAttribute("aria-label")).toBe("Interrupted");
  expect(trigger(container).querySelector("[role=img]")).toBeNull();
});

it("takes no focus and no press: the glyph is no tab stop and a press on it falls through", () => {
  const container = mountForTooltip(<StatusIcon status={"idle"} decorative withTooltip />);
  expect(trigger(container).tabIndex).toBe(-1);
  const press = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
  act(() => void trigger(container).dispatchEvent(press));
  expect(press.defaultPrevented).toBe(true);
});

it("carries no tooltip where a visible status label sits beside the glyph", () => {
  fakeTimers();
  const container = mountForTooltip(<StatusIcon status={"working"} decorative />);
  enter(trigger(container));
  runOutTimers();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("opens its text in a popover on a tap where the caller leaves the popover on", () => {
  const container = mountForTooltip(<StatusIcon status="interrupted" withTooltip touchPopover />);
  press(trigger(container));
  expect(openPopover()?.textContent).toBe("Interrupted");
});

it("opens no popover on a tap where the caller turns it off, as in a pressable row", () => {
  const container = mountForTooltip(<StatusIcon status="interrupted" decorative withTooltip touchPopover={false} />);
  press(trigger(container));
  expect(openPopover()).toBeNull();
});

const EVERY_STATUS: ShownStatusKind[] = ["waiting_user", "dormant", "paused", "done", "switching", "reconnecting", "idle", "delegated", "background", "working", "starting", "interrupted", "archived"];

it.each([
  [false, "size-4"],
  [true, "size-3.5"],
])("draws every status in one slot, the same size, centring what it holds (small: %s)", (small, size) => {
  for (const status of EVERY_STATUS) {
    const slot = mountForTooltip(<StatusIcon status={status} decorative small={small} />).firstElementChild!;
    expect(slot.className, status).toBe(`flex shrink-0 items-center justify-center ${size}`);
    expect(slot.children, status).toHaveLength(1);
  }
});

it("moves the bubble and the hand a little on their grid, and no other glyph", () => {
  const viewBox = (status: ShownStatusKind) => mountForTooltip(<StatusIcon status={status} decorative />).querySelector("svg")!.getAttribute("viewBox");
  expect(viewBox("idle")).toBe(shiftedViewBox(BUBBLE_SHIFT));
  expect(viewBox("waiting_user")).toBe(shiftedViewBox(HAND_SHIFT));
  for (const status of ["dormant", "paused", "done", "interrupted"] as const) expect(viewBox(status)).toBe("0 0 24 24");
});
