// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished } from "vitest";

import { TitledControl } from "./TitledControl";
import { mountForTooltip, openPopover, press } from "./tooltipTestSupport";
import { Button, Modal, ModalCloseButton } from "../../primitives";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const unmounts: Array<() => void> = [];

afterEach(() => {
  for (const unmount of unmounts.splice(0)) act(unmount);
});

/** Mounts `control` in a `TitledControl` and opens its tooltip the way a keyboard user does, by
 * focusing it after a key press. Returns the control's button. */
function openTooltip(title: string, control: React.ComponentProps<typeof TitledControl>["children"], frame?: (node: React.ReactElement) => React.ReactElement): HTMLButtonElement {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  unmounts.push(() => root.unmount());
  const titled = <TitledControl title={title}>{control}</TitledControl>;
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
  openTooltip("Refresh", <Button label="Refresh">R</Button>);
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Refresh");
});

it("gives its tooltip the control fills, which it does not inherit from where it is portalled", () => {
  openTooltip("Refresh", <Button label="Refresh">R</Button>);
  expect(document.querySelector('[role="tooltip"]')?.classList.contains("control-fills")).toBe(true);
});

it("does not describe a control by a tooltip that only repeats its name", () => {
  const button = openTooltip("Refresh", <Button label="Refresh">R</Button>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("keeps describing a control by a tooltip that says more than its name", () => {
  const button = openTooltip("Remove this folder from the trusted list", <Button label="Remove">R</Button>);
  const tooltip = document.querySelector('[role="tooltip"]');
  expect(button.getAttribute("aria-describedby")).toBe(tooltip?.id);
});

it("does not describe a close button by a tooltip that only repeats its name", () => {
  const button = openTooltip("Close", <ModalCloseButton label="Close" />, (node) => <Modal onClose={() => {}} label="Dialog">{node}</Modal>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("leaves a control's own aria-describedby alone", () => {
  const button = openTooltip("Refresh", <Button label="Refresh" describedBy="hint">R</Button>);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  expect(button.getAttribute("aria-describedby")).toBe("hint");
});

it("opens no popover on a touch press: the tap is the control's action", () => {
  let pressed = 0;
  const button = mountForTooltip(<TitledControl title="Refresh the list"><Button label="Refresh" onPress={() => pressed++}>R</Button></TitledControl>).querySelector("button")!;
  press(button, "touch");
  expect(pressed).toBe(1);
  expect(openPopover()).toBeNull();
});

it("closes its tooltip once a modal opening over the page makes the control inert, and not for another inert region", async () => {
  const button = openTooltip("Refresh", <Button label="Refresh">R</Button>);
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
