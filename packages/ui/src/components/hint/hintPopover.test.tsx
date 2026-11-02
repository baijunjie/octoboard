// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { HOVER_CLOSE_DELAY_MS, HOVER_OPEN_DELAY_MS, PopoverHint } from "./hintPopover";
import { enter, leave, mountForTooltip, openPopover as panel, pointer, trigger as triggerOf, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(holdsOpen?: () => boolean) {
  const container = mountForTooltip(
    <PopoverHint
      label="Panel"
      holdsOpen={holdsOpen}
      trigger={({ ref, onMouseHoverChange }) => (
        <button ref={ref} onPointerOver={() => onMouseHoverChange(true)} onPointerOut={() => onMouseHoverChange(false)}>
          Open
        </button>
      )}
    >
      {() => <span>Body</span>}
    </PopoverHint>,
  );
  return triggerOf(container);
}

/** Opens the panel by hover, then takes the pointer off the trigger: what the grace is counted from. */
function hoverThenLeave(runFor: (ms: number) => void, button: HTMLElement) {
  enter(button);
  runFor(HOVER_OPEN_DELAY_MS);
  expect(panel()).not.toBeNull();
  leave(button);
}

/** Fake timers for the test; returns the call that lets `ms` of them pass. */
function clock() {
  withFakeTimers();
  return (ms: number) => act(() => void vi.advanceTimersByTime(ms));
}

it("closes once the pointer has been off the trigger for the grace", () => {
  const runFor = clock();
  hoverThenLeave(runFor, mount());
  runFor(HOVER_CLOSE_DELAY_MS);
  expect(panel()).toBeNull();
});

it("stays open past the grace while a surface it holds is up, and closes one grace after that is gone", () => {
  let nested = true;
  const runFor = clock();
  hoverThenLeave(runFor, mount(() => nested));
  runFor(HOVER_CLOSE_DELAY_MS * 5);
  expect(panel()).not.toBeNull();
  nested = false;
  runFor(HOVER_CLOSE_DELAY_MS);
  expect(panel()).toBeNull();
});

it("stays open when the panel's own pointer leave comes while a surface it holds is up", () => {
  let nested = false;
  const runFor = clock();
  const button = mount(() => nested);
  enter(button);
  runFor(HOVER_OPEN_DELAY_MS);
  nested = true;
  act(() => void panel()!.dispatchEvent(pointer("pointerout")));
  runFor(HOVER_CLOSE_DELAY_MS * 5);
  expect(panel()).not.toBeNull();
  nested = false;
  runFor(HOVER_CLOSE_DELAY_MS);
  expect(panel()).toBeNull();
});

/** Makes `:hover` true for the elements `resting` names (the real engine answers it from a pointer that jsdom has not). */
function restingOn(resting: () => Array<Element | null>) {
  const matches = Element.prototype.matches;
  const spy = vi.spyOn(Element.prototype, "matches").mockImplementation(function (this: Element, selector: string) {
    return selector === ":hover" ? resting().includes(this) : matches.call(this, selector);
  });
  onTestFinished(() => spy.mockRestore());
}

it("stays open after the hold ends while the pointer rests on the panel, and closes once it is off", () => {
  let nested = true;
  let resting = true;
  const runFor = clock();
  restingOn(() => (resting ? [panel()] : []));
  const button = mount(() => nested);
  enter(button);
  runFor(HOVER_OPEN_DELAY_MS);
  act(() => void panel()!.dispatchEvent(pointer("pointerout")));
  runFor(HOVER_CLOSE_DELAY_MS * 3);
  nested = false;
  runFor(HOVER_CLOSE_DELAY_MS * 5);
  expect(panel()).not.toBeNull();
  // Leaving the panel arms the grace again, now with nothing held and the pointer off it.
  resting = false;
  act(() => void panel()!.dispatchEvent(pointer("pointerout")));
  runFor(HOVER_CLOSE_DELAY_MS);
  expect(panel()).toBeNull();
});

it("stays open after the hold ends while the pointer rests on the trigger", () => {
  let nested = true;
  const runFor = clock();
  const button = mount(() => nested);
  restingOn(() => [button]);
  hoverThenLeave(runFor, button);
  runFor(HOVER_CLOSE_DELAY_MS * 3);
  nested = false;
  runFor(HOVER_CLOSE_DELAY_MS * 5);
  expect(panel()).not.toBeNull();
});

it("closes when nothing is held even though the pointer reads as resting, since the leave was a real one", () => {
  const runFor = clock();
  const button = mount();
  restingOn(() => [button, panel()]);
  hoverThenLeave(runFor, button);
  runFor(HOVER_CLOSE_DELAY_MS);
  expect(panel()).toBeNull();
});

it("closes within a short poll of the hold ending, not another grace later", () => {
  let nested = true;
  const runFor = clock();
  hoverThenLeave(runFor, mount(() => nested));
  runFor(HOVER_CLOSE_DELAY_MS * 3);
  nested = false;
  runFor(HOVER_CLOSE_DELAY_MS / 2);
  expect(panel()).toBeNull();
});

it("counts a second grace afresh when the first is still running", () => {
  const runFor = clock();
  const button = mount();
  hoverThenLeave(runFor, button);
  runFor(HOVER_CLOSE_DELAY_MS - 10);
  act(() => void panel()!.dispatchEvent(pointer("pointerout")));
  runFor(HOVER_CLOSE_DELAY_MS - 10);
  expect(panel()).not.toBeNull();
  runFor(10);
  expect(panel()).toBeNull();
});
