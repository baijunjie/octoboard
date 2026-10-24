// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { HelpTip } from "./HelpTip";
import { HOVER_CLOSE_DELAY_MS, HOVER_OPEN_DELAY_MS } from "./Hint";
import { activate, enter, leave, mountForTooltip, openPopover, openTooltip, press, tap, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount() {
  const container = mountForTooltip(<HelpTip label="About the default account">It runs under no account.</HelpTip>);
  return container.querySelector("button")!;
}

const hoverDelay = () => act(() => void vi.advanceTimersByTime(HOVER_OPEN_DELAY_MS));
const graceDelay = () => act(() => void vi.advanceTimersByTime(HOVER_CLOSE_DELAY_MS));

function focusFromKeyboard(button: HTMLElement) {
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
}

it("is a button named for what it explains, with no text of its own", () => {
  const button = mount();
  expect(button.getAttribute("aria-label")).toBe("About the default account");
  expect(button.textContent).toBe("");
  expect(openPopover()).toBeNull();
});

it("opens the explanation once the mouse has rested on it, without moving focus", () => {
  withFakeTimers();
  const button = mount();
  enter(button);
  expect(openPopover()).toBeNull();
  hoverDelay();
  expect(openPopover()?.textContent).toBe("It runs under no account.");
  expect(openPopover()?.contains(document.activeElement)).toBe(false);
  expect(openTooltip()).toBeNull();
});

it("stays open while the pointer moves into it, and closes shortly after the pointer has left both", () => {
  withFakeTimers();
  const button = mount();
  enter(button);
  hoverDelay();
  leave(button);
  enter(openPopover()!);
  graceDelay();
  expect(openPopover()).not.toBeNull();
  leave(openPopover()!);
  graceDelay();
  expect(openPopover()).toBeNull();
});

it("opens nothing on a mouse click: the hover is what opens it", () => {
  const button = mount();
  press(button, "mouse");
  expect(openPopover()).toBeNull();
  expect(button.getAttribute("aria-expanded")).not.toBe("true");
});

it("opens on a touch press with focus inside, named by the label", () => {
  const button = mount();
  press(button, "touch");
  const popover = openPopover();
  expect(popover?.textContent).toBe("It runs under no account.");
  expect(popover?.getAttribute("aria-label")).toBe("About the default account");
  expect(popover?.contains(document.activeElement)).toBe(true);
});

it("opens on a pen press as well", () => {
  press(mount(), "pen");
  expect(openPopover()?.textContent).toBe("It runs under no account.");
});

it.each([["Enter", "Enter"], ["Space", " "]] as const)("opens on %s with focus inside", (_name, key) => {
  const button = mount();
  focusFromKeyboard(button);
  tap(button, key);
  expect(openPopover()?.textContent).toBe("It runs under no account.");
  expect(openPopover()?.contains(document.activeElement)).toBe(true);
});

it("opens on a virtual press, which is what assistive technology sends", async () => {
  const button = mount();
  activate(button);
  // react-aria hands focus over once a transition has run for a virtual press.
  await act(() => new Promise((resolve) => setTimeout(resolve, 100)));
  expect(openPopover()?.textContent).toBe("It runs under no account.");
  expect(openPopover()!.contains(document.activeElement)).toBe(true);
});

it("closes on Escape and gives focus back to the button", async () => {
  const button = mount();
  act(() => button.focus());
  press(button);
  expect(openPopover()).not.toBeNull();
  act(() => void openPopover()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(openPopover()).toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.activeElement).toBe(button);
});

it("closes on a tap outside it", () => {
  press(mount());
  expect(openPopover()).not.toBeNull();
  press(document.body);
  expect(openPopover()).toBeNull();
});

// A popover is portalled out of the pane that carries the fills, so it carries them itself, and it
// is drawn with the tooltip's own class.
it("is drawn like a tooltip, with the control fills", () => {
  press(mount());
  expect(openPopover()?.closest(".tooltip")).not.toBeNull();
  expect(openPopover()?.closest(".control-fills")).not.toBeNull();
});

it("shows rich content", () => {
  const container = mountForTooltip(
    <HelpTip label="About detecting the agent">
      Looks for <code>CLAUDE.md</code> in each repository.
    </HelpTip>,
  );
  press(container.querySelector("button")!);
  expect(openPopover()?.querySelector("code")?.textContent).toBe("CLAUDE.md");
});
