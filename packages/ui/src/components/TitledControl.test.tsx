// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { CloseButton } from "@heroui/react";
import { afterEach, expect, it } from "vitest";

import { Button } from "./Button";
import { TitledControl } from "./TitledControl";
import { activate, mountForTooltip, openPopover, press, tap } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const unmounts: Array<() => void> = [];

afterEach(() => {
  for (const unmount of unmounts.splice(0)) act(unmount);
});

/** Mounts `control` in a `TitledControl` and opens its tooltip the way a keyboard user does, by
 * focusing it after a key press. Returns the control's button. */
function openTooltip(title: string, control: React.ComponentProps<typeof TitledControl>["children"]): HTMLButtonElement {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  unmounts.push(() => root.unmount());
  act(() => root.render(<TitledControl title={title}>{control}</TitledControl>));
  const button = container.querySelector("button");
  if (!button) throw new Error("no button rendered");
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  return button;
}

it("opens its tooltip when the control takes keyboard focus", () => {
  openTooltip("Refresh", <Button aria-label="Refresh">R</Button>);
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Refresh");
});

it("gives its tooltip the control fills, which it does not inherit from where it is portalled", () => {
  openTooltip("Refresh", <Button aria-label="Refresh">R</Button>);
  expect(document.querySelector('[role="tooltip"]')?.classList.contains("control-fills")).toBe(true);
});

it("does not describe a control by a tooltip that only repeats its name", () => {
  const button = openTooltip("Refresh", <Button aria-label="Refresh">R</Button>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("keeps describing a control by a tooltip that says more than its name", () => {
  const button = openTooltip("Remove this folder from the trusted list", <Button aria-label="Remove">R</Button>);
  const tooltip = document.querySelector('[role="tooltip"]');
  expect(button.getAttribute("aria-describedby")).toBe(tooltip?.id);
});

it("does not describe a close button by a tooltip that only repeats its name", () => {
  const button = openTooltip("Close", <CloseButton aria-label="Close" />);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("leaves a control's own aria-describedby alone", () => {
  const button = openTooltip("Refresh", <Button aria-label="Refresh" aria-describedby="hint">R</Button>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBe("hint");
});

/** Mounts `control` in a `TitledControl` with `touchPopover` as given, and returns its button. */
function mountControl(control: React.ComponentProps<typeof TitledControl>["children"], touchPopover?: boolean): HTMLButtonElement {
  const container = mountForTooltip(<TitledControl title="Refresh the list" touchPopover={touchPopover}>{control}</TitledControl>);
  return container.querySelector("button")!;
}

it("opens no popover on a touch press by default: the tap is the control's action", () => {
  let pressed = 0;
  const button = mountControl(<Button aria-label="Refresh" onPress={() => pressed++}>R</Button>);
  press(button, "touch");
  expect(pressed).toBe(1);
  expect(openPopover()).toBeNull();
});

it("opens the title in a popover on a touch press where the control asks for one, its own action still run", () => {
  let pressed = 0;
  const button = mountControl(<Button aria-label="Refresh" onPress={() => pressed++}>R</Button>, true);
  press(button, "touch");
  expect(pressed).toBe(1);
  expect(openPopover()?.textContent).toBe("Refresh the list");
  expect(openPopover()?.getAttribute("aria-label")).toBe("Refresh");
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("opens no popover on a mouse press even where the control asks for one", () => {
  const button = mountControl(<Button aria-label="Refresh">R</Button>, true);
  press(button, "mouse");
  expect(openPopover()).toBeNull();
});

it("adds no element around the control with a popover asked for", () => {
  const container = mountForTooltip(<TitledControl title="Refresh" touchPopover><Button aria-label="Refresh">R</Button></TitledControl>);
  expect(container.firstElementChild?.tagName).toBe("BUTTON");
});

it("opens the title in a popover on a virtual press where the control asks for one", () => {
  const button = mountControl(<Button aria-label="Refresh">R</Button>, true);
  activate(button);
  expect(openPopover()?.textContent).toBe("Refresh the list");
});

it("opens no popover on a key press, with the popover asked for or not", () => {
  for (const touchPopover of [undefined, true]) {
    const button = mountControl(<Button aria-label="Refresh">R</Button>, touchPopover);
    tap(button, "Enter");
    tap(button, " ");
    expect(openPopover()).toBeNull();
  }
});
