// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { HelpTip } from "./HelpTip";
import { activate, enter, mountForTooltip, openPopover, openTooltip, press, tap, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount() {
  const container = mountForTooltip(<HelpTip label="About the default account">It runs under no account.</HelpTip>);
  return container.querySelector("button")!;
}

it("is a button named for what it explains, with no text of its own", () => {
  const button = mount();
  expect(button.getAttribute("aria-label")).toBe("About the default account");
  expect(button.textContent).toBe("");
  expect(openTooltip()).toBeNull();
});

it("shows the explanation once the mouse has rested on it", () => {
  const runTimers = withFakeTimers();
  const button = mount();
  enter(button);
  runTimers();
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("shows the explanation at once on keyboard focus and describes the button by it", () => {
  const button = mount();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  const tooltip = openTooltip();
  expect(tooltip?.textContent).toBe("It runs under no account.");
  expect(button.getAttribute("aria-describedby")).toBe(tooltip?.id);
});

it("opens nothing more on a mouse press: the tooltip is all a mouse has", () => {
  const button = mount();
  press(button, "mouse");
  expect(openPopover()).toBeNull();
  expect(button.hasAttribute("aria-expanded")).toBe(false);
});

it("opens the explanation in a popover on a touch press", () => {
  const button = mount();
  press(button, "touch");
  const popover = openPopover();
  expect(popover?.textContent).toBe("It runs under no account.");
  expect(popover?.getAttribute("aria-label")).toBe("About the default account");
  // The modal popover hides its trigger from assistive technology, so it is not linked to it.
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

function focusFromKeyboard(button: HTMLElement) {
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
}

it("opens the popover on a virtual press, which is what assistive technology sends", async () => {
  const button = mount();
  focusFromKeyboard(button);
  expect(openTooltip()).not.toBeNull();
  activate(button);
  expect(openPopover()?.textContent).toBe("It runs under no account.");
  // The tooltip that focus opened is closed, or its own Escape listener would swallow the popover's.
  expect(openTooltip()).toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 100)));
  act(() => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(openPopover()).toBeNull();
});

it.each([["Enter", "Enter"], ["Space", " "]] as const)("keeps the tooltip open on %s and opens no popover", (_name, key) => {
  const button = mount();
  focusFromKeyboard(button);
  tap(button, key);
  expect(openPopover()).toBeNull();
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("closes the tooltip on Escape after a keyboard press, the dialog around it left to the next one", () => {
  const button = mount();
  focusFromKeyboard(button);
  tap(button, "Enter");
  const escape = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  act(() => void button.dispatchEvent(escape));
  expect(openTooltip()).toBeNull();
});

it("shows the tooltip again after Escape closes a virtual press's popover, as focus returns by keyboard", async () => {
  const button = mount();
  act(() => button.focus());
  activate(button);
  // The popover takes focus once the press has settled.
  await act(() => new Promise((resolve) => setTimeout(resolve, 100)));
  expect(openPopover()!.contains(document.activeElement)).toBe(true);
  act(() => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(openPopover()).toBeNull();
  await act(() => new Promise((resolve) => setTimeout(resolve, 300)));
  expect(document.activeElement).toBe(button);
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("opens the popover on a pen press as well", () => {
  press(mount(), "pen");
  expect(openPopover()?.textContent).toBe("It runs under no account.");
});

// A popover is portalled out of the pane that carries the fills, so it carries them itself, and it
// is drawn with the tooltip's own class.
it("draws its popover like a tooltip, with the control fills", () => {
  press(mount());
  expect(openPopover()?.classList.contains("tooltip")).toBe(true);
  expect(openPopover()?.classList.contains("control-fills")).toBe(true);
});

it("keeps the tooltip out while the popover is open", () => {
  const runTimers = withFakeTimers();
  const button = mount();
  enter(button);
  runTimers();
  expect(openTooltip()).not.toBeNull();
  press(button, "pen");
  runTimers();
  expect(openPopover()).not.toBeNull();
  expect(openTooltip()).toBeNull();
});

it("closes the popover on a tap outside it", () => {
  press(mount());
  expect(openPopover()).not.toBeNull();
  press(document.body);
  expect(openPopover()).toBeNull();
});

it("closes the popover on Escape and gives focus back to the button after a touch alone", async () => {
  const button = mount();
  // A tap focuses the button as the browser's emulated mouse press does, with no key pressed.
  act(() => button.focus());
  press(button);
  expect(openPopover()).not.toBeNull();
  act(() => void openPopover()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(openPopover()).toBeNull();
  // Focus goes back a frame later.
  await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
  expect(document.activeElement).toBe(button);
});

it("shows rich content in the tooltip, as the button's description, and again in the popover", () => {
  const container = mountForTooltip(
    <HelpTip label="About detecting the agent">
      Looks for <code>CLAUDE.md</code> in each repository.
    </HelpTip>,
  );
  const button = container.querySelector("button")!;
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  const tooltip = openTooltip();
  expect(tooltip?.querySelector("code")?.textContent).toBe("CLAUDE.md");
  expect(button.getAttribute("aria-describedby")).toBe(tooltip?.id);
  press(button);
  expect(openPopover()?.querySelector("code")?.textContent).toBe("CLAUDE.md");
});
