// @vitest-environment jsdom
import { Switch } from "@heroui/react";
import { act, useState } from "react";
import { expect, it } from "vitest";

import { IndicatorTooltip } from "./IndicatorTooltip";
import { enter, mountForTooltip, openPopover, openTooltip, press, trigger, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(touchPopover?: boolean) {
  const container = mountForTooltip(
    <IndicatorTooltip tooltip="Waiting for you" label="Waiting" touchPopover={touchPopover}>
      <span>!</span>
    </IndicatorTooltip>,
  );
  return trigger(container);
}

it("shows the tooltip once a mouse has rested on the mark", () => {
  const runTimers = withFakeTimers();
  enter(mount());
  runTimers();
  expect(openTooltip()?.textContent).toBe("Waiting for you");
});

it("opens nothing on a mouse press", () => {
  press(mount(), "mouse");
  expect(openPopover()).toBeNull();
});

it("opens the tooltip's content in a popover on a touch press, named by the mark's name", () => {
  const mark = mount();
  press(mark, "touch");
  const popover = openPopover();
  expect(popover?.textContent).toBe("Waiting for you");
  expect(popover?.getAttribute("aria-label")).toBe("Waiting");
  expect(mark.getAttribute("aria-describedby")).toBeFalsy();
});

it("opens the popover on a pen press as well", () => {
  press(mount(), "pen");
  expect(openPopover()).not.toBeNull();
});

it("draws the popover like a tooltip, with the control fills", () => {
  press(mount());
  expect(openPopover()?.classList.contains("tooltip")).toBe(true);
  expect(openPopover()?.classList.contains("control-fills")).toBe(true);
});

it("names a popover by its text where the mark has no name of its own", () => {
  const container = mountForTooltip(<IndicatorTooltip tooltip="Waiting for you"><span>!</span></IndicatorTooltip>);
  press(trigger(container));
  const popover = openPopover()!;
  expect(document.getElementById(popover.getAttribute("aria-labelledby")!)?.textContent).toBe("Waiting for you");
});

it("keeps the tooltip out while the popover is open", () => {
  const runTimers = withFakeTimers();
  const mark = mount();
  enter(mark);
  runTimers();
  expect(openTooltip()).not.toBeNull();
  press(mark, "pen");
  runTimers();
  expect(openPopover()).not.toBeNull();
  expect(openTooltip()).toBeNull();
});

it("closes the popover on a tap outside it and on Escape", () => {
  press(mount());
  press(document.body);
  expect(openPopover()).toBeNull();
  press(mount());
  expect(openPopover()).not.toBeNull();
  act(() => void openPopover()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(openPopover()).toBeNull();
});

it("opens no popover on a touch press where the mark turns it off, and keeps its tooltip", () => {
  const runTimers = withFakeTimers();
  const mark = mount(false);
  press(mark, "touch");
  expect(openPopover()).toBeNull();
  expect(mark.classList.contains("touch-target")).toBe(false);
  enter(mark);
  runTimers();
  expect(openTooltip()?.textContent).toBe("Waiting for you");
});

it("grows a mark that opens a popover into a touch target", () => {
  expect(mount().classList.contains("touch-target")).toBe(true);
});

it("leaves a switch it wraps working: a mouse press toggles it and takes its focus, a touch press opens no popover", () => {
  function Toggle() {
    const [on, setOn] = useState(false);
    return (
      <IndicatorTooltip tooltip="Include console sessions" touchPopover={false}>
        <Switch aria-label="Include" isSelected={on} onChange={setOn}>
          <Switch.Content>
            <Switch.Control>
              <Switch.Thumb />
            </Switch.Control>
          </Switch.Content>
        </Switch>
      </IndicatorTooltip>
    );
  }
  const container = mountForTooltip(<Toggle />);
  const input = container.querySelector("input")!;
  const label = container.querySelector("label")!;
  press(label, "mouse");
  expect(input.checked).toBe(true);
  press(label, "touch");
  expect(input.checked).toBe(false);
  expect(openPopover()).toBeNull();
});
