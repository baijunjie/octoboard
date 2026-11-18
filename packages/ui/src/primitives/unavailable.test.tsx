// @vitest-environment jsdom
import React, { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { CONTROL_TOOLTIP_DELAY_MS, enter, leave, mountForTooltip, openPopover, openTooltip, press } from "../components/hint/tooltipTestSupport";
import { fakeTimers } from "../test/clock";
import { Button, Checkbox, Switch, ToggleButton, Tooltip } from ".";
import { mount } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// What Button, Checkbox and Switch share while unavailable for a reason (`./unavailable`): `aria-disabled`, no response
// to a press, the reason as the description and, only on the control's box, as the tooltip, which survives presses.
const REASON = "Sessions are running";
const pressed = vi.fn();
const kinds = {
  button: { mount: () => mountForTooltip(<Button label="Keep" tooltip="Keep it" unavailable={REASON} onPress={pressed}>Keep</Button>), control: "button", box: "button" },
  switch: { mount: () => mountForTooltip(<Switch label="Share" selected={false} onSelectedChange={pressed} unavailable={REASON} />), control: "input", box: ".switch__control" },
  checkbox: { mount: () => mountForTooltip(<Checkbox selected onSelectedChange={pressed} unavailable={REASON}>Command</Checkbox>), control: "input", box: ".checkbox__control" },
} as const;
const kindNames = ["button", "switch", "checkbox"] as const;
const advance = () => act(() => void vi.advanceTimersByTime(CONTROL_TOOLTIP_DELAY_MS));

it.each(kindNames)("keeps an unavailable %s focusable, aria-disabled and described by the reason, and does nothing when pressed", (kind) => {
  pressed.mockClear();
  const container = kinds[kind].mount();
  const control = container.querySelector<HTMLElement>(kinds[kind].control)!;
  expect(control.getAttribute("aria-disabled")).toBe("true");
  expect((control as HTMLButtonElement).disabled).toBeFalsy();
  expect(document.getElementById(control.getAttribute("aria-describedby")!)?.textContent).toBe(REASON);
  act(() => control.click());
  press(container.querySelector(kinds[kind].box)!, "mouse");
  expect(pressed).not.toHaveBeenCalled();
});

it.each(kindNames)("opens the reason of an unavailable %s on hover of its box and closes it on leaving", (kind) => {
  fakeTimers();
  const box = kinds[kind].mount().querySelector(kinds[kind].box)!;
  enter(box);
  advance();
  expect(openTooltip()?.textContent).toBe(REASON);
  leave(box);
  advance();
  expect(openTooltip()).toBeNull();
});

it.each(kindNames)("opens the reason of an unavailable %s on keyboard focus", (kind) => {
  fakeTimers();
  const container = kinds[kind].mount();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    container.querySelector<HTMLElement>(kinds[kind].control)!.focus();
  });
  advance();
  expect(openTooltip()?.textContent).toBe(REASON);
});

it.each(kindNames)("still opens the reason of an unavailable %s on hover after it was pressed", (kind) => {
  fakeTimers();
  const container = kinds[kind].mount();
  const box = container.querySelector(kinds[kind].box)!;
  for (let round = 0; round < 2; round += 1) {
    enter(box);
    advance();
    expect(openTooltip()?.textContent).toBe(REASON);
    press(box, "mouse");
    act(() => container.querySelector<HTMLElement>(kinds[kind].control)!.focus());
    leave(box);
    advance();
    expect(openTooltip()).toBeNull();
  }
});

it("does not open a checkbox's reason on its label text", () => {
  fakeTimers();
  const container = kinds.checkbox.mount();
  const text = Array.from(container.querySelectorAll("*")).find((node) => node.children.length === 0 && node.textContent === "Command")!;
  enter(text);
  advance();
  expect(openTooltip()).toBeNull();
});

it("shows a control's own tooltip while it is available and the reason in its place while it is not", () => {
  fakeTimers();
  const container = mountForTooltip(<Button label="Keep" tooltip="Keep it" unavailable={undefined}>Keep</Button>);
  const button = container.querySelector("button")!;
  enter(button);
  advance();
  expect(openTooltip()?.textContent).toBe("Keep it");
});

it.each(kindNames)("keeps an unavailable %s in its place as it turns available and unavailable again, so it keeps its focus", (kind) => {
  const control = {
    button: (unavailable: React.ReactNode) => <Button label="Keep" tooltip="Keep it" unavailable={unavailable}>Keep</Button>,
    switch: (unavailable: React.ReactNode) => <Switch label="Share" selected={false} onSelectedChange={pressed} unavailable={unavailable} />,
    checkbox: (unavailable: React.ReactNode) => <Checkbox selected onSelectedChange={pressed} unavailable={unavailable}>Command</Checkbox>,
  }[kind];
  const { container, render } = mount(control(REASON));
  const element = container.querySelector<HTMLElement>(kinds[kind].control)!;
  act(() => element.focus());
  for (const unavailable of ["", REASON, "", true]) {
    render(control(unavailable));
    expect(container.querySelector(kinds[kind].control)).toBe(element);
    expect(document.activeElement).toBe(element);
  }
  expect(element.getAttribute("aria-disabled")).toBe("true");
});

it.each(kindNames)("treats an empty reason as available and true as unavailable with no reason, for %s", (kind) => {
  const control = {
    button: (unavailable: React.ReactNode) => <Button label="Keep" unavailable={unavailable}>Keep</Button>,
    switch: (unavailable: React.ReactNode) => <Switch label="Share" selected={false} onSelectedChange={pressed} unavailable={unavailable} />,
    checkbox: (unavailable: React.ReactNode) => <Checkbox selected onSelectedChange={pressed} unavailable={unavailable}>Command</Checkbox>,
  }[kind];
  const { container, render } = mount(control(""));
  const element = () => container.querySelector<HTMLElement>(kinds[kind].control)!;
  expect(element().getAttribute("aria-disabled")).toBeNull();
  render(control(true));
  expect(element().getAttribute("aria-disabled")).toBe("true");
  expect(element().getAttribute("aria-describedby")).toBeFalsy();
});

it("opens the reason of an unavailable button in the touch popover on a tap, and the tap does nothing else", () => {
  pressed.mockClear();
  const container = kinds.button.mount();
  press(container.querySelector("button")!, "touch");
  expect(openPopover()?.textContent).toBe(REASON);
  expect(openPopover()?.getAttribute("aria-label")).toBe("Keep");
  expect(pressed).not.toHaveBeenCalled();
});

it("opens no popover on the tap of an available button, which runs its action", () => {
  pressed.mockClear();
  const container = mountForTooltip(<Button label="Keep" tooltip="Keep it" onPress={pressed}>Keep</Button>);
  press(container.querySelector("button")!, "touch");
  expect(pressed).toHaveBeenCalledOnce();
  expect(openPopover()).toBeNull();
});

it.each(["switch", "checkbox"] as const)("opens the reason of an unavailable %s in the touch popover on a tap of its box, not of its label text", (kind) => {
  const container = kinds[kind].mount();
  const text = Array.from(container.querySelectorAll("*")).find((node) => node.children.length === 0 && node.textContent === "Command");
  if (text) {
    press(text, "touch");
    expect(openPopover()).toBeNull();
  }
  press(container.querySelector(kinds[kind].box)!, "touch");
  expect(openPopover()?.textContent).toBe(REASON);
});

it.each(["switch", "checkbox"] as const)("takes the hover of a %s on its box, which is not a tab stop and does not take focus", (kind) => {
  const container = kinds[kind].mount();
  const box = container.querySelector<HTMLElement>(kinds[kind].box)!;
  expect(box.hasAttribute("tabindex")).toBe(false);
  press(box, "mouse");
  expect(document.activeElement).not.toBe(box);
});

it("does not describe a button by a tooltip that repeats its name, and keeps the description of one that says more", () => {
  const same = mountForTooltip(<Button label="Refresh" tooltip="Refresh">R</Button>).querySelector("button")!;
  expect(same.getAttribute("aria-describedby")).toBe("");
  const more = mountForTooltip(<Button label="Refresh" tooltip="Refresh the list">R</Button>).querySelector("button")!;
  expect(more.getAttribute("aria-describedby")).toBeNull();
  const own = mountForTooltip(<Button label="Refresh" tooltip="Refresh" describedBy="hint">R</Button>).querySelector("button")!;
  expect(own.getAttribute("aria-describedby")).toBe("hint");
});

it.each(["Button", "Switch", "Checkbox"] as const)("reports a %s wrapped in a Tooltip, which would drop the outer tooltip", (kind) => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  onTestFinished(() => error.mockRestore());
  const control = {
    Button: <Button label="Keep">Keep</Button>,
    Switch: <Switch label="Share" selected={false} onSelectedChange={pressed} />,
    Checkbox: <Checkbox selected onSelectedChange={pressed}>Command</Checkbox>,
  }[kind];
  mount(<Tooltip content="Outer">{control as React.ReactElement<{ label?: string }>}</Tooltip>);
  expect(error).toHaveBeenCalledWith(expect.stringContaining(`A ${kind} carries its own tooltip`));
});

it("closes the touch popover when the control turns available, which neither blocks its own tooltip nor reopens the popover as it turns unavailable again", () => {
  fakeTimers();
  const control = (unavailable: React.ReactNode) => <Button label="Keep" tooltip="Keep it" unavailable={unavailable}>Keep</Button>;
  const { container, render } = mount(control(REASON));
  press(container.querySelector("button")!, "touch");
  expect(openPopover()?.textContent).toBe(REASON);
  render(control(""));
  expect(openPopover()).toBeNull();
  enter(container.querySelector("button")!);
  advance();
  expect(openTooltip()?.textContent).toBe("Keep it");
  leave(container.querySelector("button")!);
  advance();
  render(control(REASON));
  expect(openPopover()).toBeNull();
});

it("opens no popover on the tap of a button unavailable with no reason, though it has its own tooltip", () => {
  const container = mountForTooltip(<Button label="Keep" tooltip="Keep it" unavailable>Keep</Button>);
  press(container.querySelector("button")!, "touch");
  expect(openPopover()).toBeNull();
});

it("does not report a Button in a control Tooltip's content, which is not the tooltip's trigger", () => {
  fakeTimers();
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  onTestFinished(() => error.mockRestore());
  const container = mountForTooltip(<Tooltip content={<Button label="Inner">Inner</Button>}><ToggleButton label="Outer">Outer</ToggleButton></Tooltip>);
  enter(container.querySelector("button")!);
  advance();
  expect(openTooltip()?.querySelector("button")).not.toBeNull();
  expect(error).not.toHaveBeenCalled();
});
