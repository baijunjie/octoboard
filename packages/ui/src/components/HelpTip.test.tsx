// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { HelpTip } from "./HelpTip";
import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "./tooltipTestSupport";

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

it("shows the explanation once the pointer has rested on it", () => {
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

function press(element: HTMLElement) {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

const dialog = () => document.querySelector('[role="dialog"]');

it("opens the explanation in a popover when pressed, leaving the tooltip out of its way", () => {
  const runTimers = withFakeTimers();
  const button = mount();
  enter(button);
  runTimers();
  expect(openTooltip()).not.toBeNull();
  press(button);
  expect(dialog()?.textContent).toBe("It runs under no account.");
  expect(dialog()?.getAttribute("aria-label")).toBe("About the default account");
  expect(button.getAttribute("aria-expanded")).toBe("true");
  runTimers();
  expect(openTooltip()).toBeNull();
});

it("closes the popover on Escape and returns focus to the button", () => {
  const button = mount();
  // A real press focuses the button before it opens the popover; synthetic events do not.
  act(() => button.focus());
  press(button);
  expect(dialog()).not.toBeNull();
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(button);
});

it("closes the popover on a press outside it", () => {
  const button = mount();
  press(button);
  expect(dialog()).not.toBeNull();
  press(document.body);
  expect(dialog()).toBeNull();
});

it("closes the popover when the button is pressed again", () => {
  const button = mount();
  press(button);
  press(button);
  expect(dialog()).toBeNull();
});

it("keeps the tooltip shut after Escape closes the popover, until focus leaves and returns", () => {
  const button = mount();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  press(button);
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(button);
  expect(openTooltip()).toBeNull();
  act(() => button.blur());
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("keeps the tooltip shut after the popover closes, until the pointer leaves and rests on it again", () => {
  const runTimers = withFakeTimers();
  const button = mount();
  enter(button);
  press(button);
  press(document.body);
  expect(dialog()).toBeNull();
  runTimers();
  expect(openTooltip()).toBeNull();
  leave(button);
  enter(button);
  runTimers();
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("shows the tooltip on the first hover after an outside press closed the popover", () => {
  const runTimers = withFakeTimers();
  const button = mount();
  press(button);
  press(document.body);
  expect(dialog()).toBeNull();
  enter(button);
  runTimers();
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("shows the tooltip on the next hover after the pointer moved away and pressed outside, as a mouse does", () => {
  const runTimers = withFakeTimers();
  const button = mount();
  enter(button);
  press(button);
  // jsdom has no layout, so every box is empty at the origin and a pointer anywhere else is off the "?".
  act(() => {
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 300, clientY: 300 }));
  });
  press(document.body);
  expect(dialog()).toBeNull();
  enter(button);
  runTimers();
  expect(openTooltip()?.textContent).toBe("It runs under no account.");
});

it("keeps the tooltip shut after Escape closes a keyboard-opened popover although the mouse moved meanwhile", () => {
  const button = mount();
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  act(() => void button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  act(() => void button.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true })));
  expect(dialog()).not.toBeNull();
  act(() => {
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 300, clientY: 300 }));
  });
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog()).toBeNull();
  act(() => button.focus());
  expect(openTooltip()).toBeNull();
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
  expect(dialog()?.querySelector("code")?.textContent).toBe("CLAUDE.md");
});
