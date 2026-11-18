// @vitest-environment jsdom
import React, { act, useRef, useState } from "react";
import { Switch } from "@heroui/react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { enter, mountForTooltip, openPopover, openTooltip, press, trigger } from "../components/hint/tooltipTestSupport";
import { fakeTimers, runOutTimers } from "../test/clock";
import { Modal, ModalCloseButton } from "./Modal";
import { ToggleButton } from "./ToggleButton";
import { Tooltip } from "./Tooltip";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The one `Tooltip`, by trigger: a control, a mark and an anchored element. What `Button`, `Switch` and `Checkbox` add
// to it for a control unavailable for a reason is in `unavailable.test.tsx`.

const unmounts: Array<() => void> = [];

afterEach(() => {
  for (const unmount of unmounts.splice(0)) act(unmount);
});

// Control.

/** Mounts `control` in a `Tooltip` and opens its tooltip the way a keyboard user does, by
 * focusing it after a key press. Returns the control's button. */
function openByFocus(title: string, control: React.ReactElement<{ label?: string; describedBy?: string }>, frame?: (node: React.ReactElement) => React.ReactElement): HTMLButtonElement {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  unmounts.push(() => root.unmount());
  const titled = <Tooltip content={title}>{control}</Tooltip>;
  act(() => root.render(frame ? frame(titled) : titled));
  // A modal is portalled out of the container, and holds a hidden dismiss button of its own before the control.
  const button = container.querySelector("button") ?? document.querySelector(`button[aria-label="${title}"]`);
  if (!button) throw new Error("no button rendered");
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  return button;
}

it("opens its tooltip when the control takes keyboard focus", () => {
  openByFocus("Refresh", <ToggleButton label="Refresh">R</ToggleButton>);
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Refresh");
});

it("gives its tooltip the control fills, which it does not inherit from where it is portalled", () => {
  openByFocus("Refresh", <ToggleButton label="Refresh">R</ToggleButton>);
  expect(document.querySelector('[role="tooltip"]')?.classList.contains("control-fills")).toBe(true);
});

it("does not describe a control by a tooltip that only repeats its name", () => {
  const button = openByFocus("Refresh", <ToggleButton label="Refresh">R</ToggleButton>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("keeps describing a control by a tooltip that says more than its name", () => {
  const button = openByFocus("Remove this folder from the trusted list", <ToggleButton label="Remove">R</ToggleButton>);
  const tooltip = document.querySelector('[role="tooltip"]');
  expect(button.getAttribute("aria-describedby")).toBe(tooltip?.id);
});

it("does not describe a close button by a tooltip that only repeats its name", () => {
  const button = openByFocus("Close", <ModalCloseButton label="Close" />, (node) => <Modal onClose={() => {}} label="Dialog">{node}</Modal>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("leaves a control's own aria-describedby alone", () => {
  const button = openByFocus("Refresh", <ToggleButton label="Refresh" describedBy="hint">R</ToggleButton>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBe("hint");
});

it("opens no popover on a touch press: the tap is the control's action", () => {
  let pressed = 0;
  const button = mountForTooltip(<Tooltip content="Refresh the list"><ToggleButton label="Refresh" onSelectedChange={() => pressed++}>R</ToggleButton></Tooltip>).querySelector("button")!;
  press(button, "touch");
  expect(pressed).toBe(1);
  expect(openPopover()).toBeNull();
});

it("closes its tooltip once a modal opening over the page makes the control inert, and not for another inert region", async () => {
  const button = openByFocus("Refresh", <ToggleButton label="Refresh">R</ToggleButton>);
  // jsdom has no `inert`, so react-aria hides with `aria-hidden` there; the attribute is set as a browser's modal does.
  const sibling = document.body.appendChild(document.createElement("div"));
  onTestFinished(() => { sibling.remove(); });
  act(() => sibling.setAttribute("inert", ""));
  await act(() => Promise.resolve());
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Refresh");
  act(() => button.parentElement!.setAttribute("inert", ""));
  await act(() => Promise.resolve());
  expect(document.querySelector('[role="tooltip"]')).toBeNull();
});

// Mark.


function mountMark(touchPopover?: boolean) {
  const container = mountForTooltip(
    <Tooltip trigger="mark" content="Waiting for you" label="Waiting" touchPopover={touchPopover}>
      <span>!</span>
    </Tooltip>,
  );
  return trigger(container);
}

it("shows the tooltip once a mouse has rested on the mark", () => {
  fakeTimers();
  enter(mountMark());
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Waiting for you");
});

it("opens nothing on a mouse press", () => {
  press(mountMark(), "mouse");
  expect(openPopover()).toBeNull();
});

it("opens the tooltip's content in a popover on a touch press, named by the mark's name", () => {
  const mark = mountMark();
  press(mark, "touch");
  const popover = openPopover();
  expect(popover?.textContent).toBe("Waiting for you");
  expect(popover?.getAttribute("aria-label")).toBe("Waiting");
  expect(mark.getAttribute("aria-describedby")).toBeFalsy();
});

it("opens the popover on a pen press as well", () => {
  press(mountMark(), "pen");
  expect(openPopover()).not.toBeNull();
});

it("draws the popover like a tooltip, with the control fills", () => {
  press(mountMark());
  expect(openPopover()?.classList.contains("tooltip")).toBe(true);
  expect(openPopover()?.classList.contains("control-fills")).toBe(true);
});

it.each([undefined, "Waiting for you"])("names the popover by a generic word where the mark's name is %s and the tooltip says \"Waiting for you\"", (label) => {
  const container = mountForTooltip(
    <Tooltip trigger="mark" content="Waiting for you" label={label}>
      <span>!</span>
    </Tooltip>,
  );
  press(trigger(container), "touch");
  expect(openPopover()?.getAttribute("aria-label")).toBe("Hint");
  expect(openPopover()?.hasAttribute("aria-labelledby")).toBe(false);
  expect(openPopover()?.textContent).toBe("Waiting for you");
  expect(trigger(container).getAttribute("aria-label")).toBe(label ?? null);
});

it("keeps the tooltip out while the popover is open", () => {
  fakeTimers();
  const mark = mountMark();
  enter(mark);
  runOutTimers();
  expect(openTooltip()).not.toBeNull();
  press(mark, "pen");
  runOutTimers();
  expect(openPopover()).not.toBeNull();
  expect(openTooltip()).toBeNull();
});

it("closes the popover on a tap outside it and on Escape", () => {
  press(mountMark());
  press(document.body);
  expect(openPopover()).toBeNull();
  press(mountMark());
  expect(openPopover()).not.toBeNull();
  act(() => void openPopover()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(openPopover()).toBeNull();
});

it("opens no popover on a touch press where the mark turns it off, and keeps its tooltip", () => {
  fakeTimers();
  const mark = mountMark(false);
  press(mark, "touch");
  expect(openPopover()).toBeNull();
  expect(mark.classList.contains("touch-target")).toBe(false);
  enter(mark);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Waiting for you");
});

it("grows a mark that opens a popover into a touch target", () => {
  expect(mountMark().classList.contains("touch-target")).toBe(true);
});

it("leaves a switch it wraps working: a mouse press toggles it and takes its focus, a touch press opens no popover", () => {
  function Toggle() {
    const [on, setOn] = useState(false);
    return (
      <Tooltip trigger="mark" content="Include console sessions" touchPopover={false}>
        <Switch aria-label="Include" isSelected={on} onChange={setOn}>
          <Switch.Content>
            <Switch.Control>
              <Switch.Thumb />
            </Switch.Control>
          </Switch.Content>
        </Switch>
      </Tooltip>
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

// Anchored.


const anchoredTip = () => document.querySelector('[role="tooltip"]');

/** The tooltip's anchor inside a region that scrolls, as a row inside a list is. */
function Anchored({ hovered }: { hovered: boolean }): React.ReactElement {
  const anchor = useRef<HTMLSpanElement>(null);
  return (
    <div data-scroller="">
      <span ref={anchor}>a-long-branch-name</span>
      <Tooltip trigger="anchored" content="The whole name" anchor={anchor} hovered={hovered} />
    </div>
  );
}

/** Mounts the anchor and returns the container along with a way to move the pointer on and off it.
 * The app's tooltip delay is counted on faked timers, which `settle` runs out; `finish` takes the
 * pointer off and shows that nothing is left counting. */
function mountAnchored(): { container: HTMLElement; hover: (hovered: boolean) => void; settle: () => void; finish: () => void } {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const hover = (hovered: boolean) => act(() => root.render(<Anchored hovered={hovered} />));
  hover(false);
  const settle = () => act(() => void vi.runOnlyPendingTimers());
  const finish = () => {
    hover(false);
    settle();
  };
  return { container, hover, settle, finish };
}

it("renders nothing until the pointer arrives, waits out the tooltip delay, and adds no element of its own", () => {
  const { container, hover, settle, finish } = mountAnchored();
  const anchored = container.innerHTML;
  expect(anchoredTip()).toBeNull();

  hover(true);
  expect(anchoredTip()).toBeNull();
  settle();
  expect(anchoredTip()?.textContent).toBe("The whole name");
  // Marked, for the one thing that has to tell it from a tooltip with a trigger element.
  expect(anchoredTip()?.hasAttribute("data-anchored-tooltip")).toBe(true);
  // Portalled out of the pane that carries the control fills, it carries them itself.
  expect(anchoredTip()?.classList.contains("control-fills")).toBe(true);
  // The tooltip is portalled, so the anchor's own markup is untouched: no wrapper, no tab stop.
  expect(container.innerHTML).toBe(anchored);

  hover(false);
  expect(anchoredTip()).toBeNull();
  finish();
});

it("closes when something under the anchor scrolls, and opens again only once the pointer has left and come back", () => {
  const { container, hover, settle, finish } = mountAnchored();
  hover(true);
  settle();
  expect(anchoredTip()).not.toBeNull();

  act(() => void container.querySelector("[data-scroller]")!.dispatchEvent(new Event("scroll")));
  expect(anchoredTip()).toBeNull();
  settle();
  expect(anchoredTip()).toBeNull();

  hover(false);
  hover(true);
  settle();
  expect(anchoredTip()).not.toBeNull();
  finish();
});

it("leaves Escape to whatever is waiting for it while it is open outside a popup holding focus", () => {
  const { container, hover, settle, finish } = mountAnchored();
  hover(true);
  settle();
  expect(anchoredTip()).not.toBeNull();

  // react-aria takes Escape above the React tree while a tooltip with a trigger element of its own
  // is open; this tooltip has none, so outside a popup holding focus (where it takes the key itself)
  // the key reaches the surface the pointer happens to be over.
  let reached = false;
  const listen = () => void (reached = true);
  window.addEventListener("keydown", listen);
  onTestFinished(() => window.removeEventListener("keydown", listen));
  act(() => void container.querySelector("span")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(reached).toBe(true);
  finish();
});

it.each([
  ["pointerdown", false],
  ["pointerdown", true],
  ["keydown", false],
  ["keydown", true],
] as const)("closes on %s without pointerleave, including while waiting (shown: %s)", (event, shown) => {
  const { container, hover, settle, finish } = mountAnchored();
  hover(true);
  if (shown) {
    settle();
    expect(anchoredTip()).not.toBeNull();
  }

  // Opening an overlay need not move the pointer or send pointerleave to the covered anchor.
  act(() => void container.querySelector("span")!.dispatchEvent(event === "keydown" ? new KeyboardEvent(event, { key: "Enter", bubbles: true }) : new Event(event, { bubbles: true })));
  settle();
  expect(anchoredTip()).toBeNull();
  hover(true);
  settle();
  expect(anchoredTip()).toBeNull();

  hover(false);
  hover(true);
  settle();
  expect(anchoredTip()).not.toBeNull();
  finish();
});

it("closes once its anchor is inside a region a modal opening over the page makes inert", async () => {
  const { container, hover, settle, finish } = mountAnchored();
  hover(true);
  settle();
  expect(anchoredTip()).not.toBeNull();
  // jsdom has no `inert`, so react-aria hides with `aria-hidden` there; the attribute is set as a browser's modal does.
  act(() => container.setAttribute("inert", ""));
  await act(() => Promise.resolve());
  expect(anchoredTip()).toBeNull();
  container.removeAttribute("inert");
  finish();
});
