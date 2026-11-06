// @vitest-environment jsdom
import React, { act, useRef } from "react";
import { toast } from "@heroui/react";
import { expect, it, onTestFinished, vi } from "vitest";

import { activateMenuItem, Menu, MenuChoices, MenuItem, MenuList, menuOfTrigger, MenuPopover, MenuTrigger } from "./Menu";
import { onFocusScopeRestore } from "./focus";
import { Modal, ModalBody, ModalHeading } from "./Modal";
import { partSelector, toastStateAttributes } from "./parts";
import { mount } from "./testSupport";
import { closeToast, showToast, Toast, ToastContent, ToastRegion } from "./Toast";
import { Tooltip, TooltipContent, TooltipTrigger, useTooltipState } from "./Tooltip";

it("announces a modal given the alert dialog role as one, described by the element it names", () => {
  mount(
    <Modal onClose={() => {}} role="alertdialog" describedBy="body">
      <ModalHeading>Failed</ModalHeading>
      <ModalBody id="body">Why it failed</ModalBody>
    </Modal>,
  );
  const dialog = document.querySelector('[role="alertdialog"]')!;
  expect(dialog).not.toBeNull();
  expect(dialog.getAttribute("aria-describedby")).toBe("body");
  expect(document.getElementById(dialog.getAttribute("aria-labelledby")!)?.textContent).toBe("Failed");
});

it("closes a modal on Escape unless keyboard dismissal is disabled", () => {
  const onClose = vi.fn();
  const { render } = mount(
    <Modal onClose={onClose} keyboardDismissDisabled>
      <ModalHeading>Settings</ModalHeading>
    </Modal>,
  );
  const escape = () => act(() => {
    document.querySelector('[role="dialog"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  });
  escape();
  expect(onClose).not.toHaveBeenCalled();
  render(
    <Modal onClose={onClose}>
      <ModalHeading>Settings</ModalHeading>
    </Modal>,
  );
  escape();
  expect(onClose).toHaveBeenCalled();
});

it("leaves a menu's popover dialog unnamed, keeps a disabled item reachable, and reports the chosen item's id", () => {
  const onAction = vi.fn();
  mount(
    <Menu open onOpenChange={() => {}}>
      <MenuTrigger label="More actions">…</MenuTrigger>
      <MenuPopover>
        <MenuList onAction={onAction} autoFocusFirst>
          <MenuItem id="rename" textValue="Rename">Rename</MenuItem>
          <MenuItem id="off" textValue="Off" disabled>Off</MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>,
  );
  const popover = document.querySelector("[data-slot=dropdown-popover]")!;
  expect(popover.getAttribute("aria-labelledby")).toBe("");
  const items = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  expect(items[1].getAttribute("aria-disabled")).toBe("true");
  // react-aria's default leaves a disabled item out of the arrow-key walk; kept, it is where the next key lands.
  expect(document.activeElement).toBe(items[0]);
  act(() => {
    items[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
    items[0].dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowDown", bubbles: true, cancelable: true }));
  });
  expect(document.activeElement).toBe(items[1]);
  act(() => items[0].click());
  expect(onAction).toHaveBeenCalledWith("rename");
});

it("keeps a disabled choice of MenuChoices reachable with the arrow keys", () => {
  mount(
    <Menu open onOpenChange={() => {}}>
      <MenuTrigger label="More actions">…</MenuTrigger>
      <MenuPopover>
        <MenuList onAction={() => {}}>
          <MenuChoices selected={["one"]}>
            <MenuItem id="one" textValue="One">One</MenuItem>
            <MenuItem id="two" textValue="Two" disabled>Two</MenuItem>
          </MenuChoices>
        </MenuList>
      </MenuPopover>
    </Menu>,
  );
  const items = [...document.querySelectorAll<HTMLElement>('[role="menuitemradio"]')];
  expect(items).toHaveLength(2);
  expect(items[1].getAttribute("aria-disabled")).toBe("true");
  // Focused by hand: the earlier test's click leaves react-aria's modality virtual, which defers a menu's autofocus.
  act(() => items[0].focus());
  act(() => {
    items[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
    items[0].dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowDown", bubbles: true, cancelable: true }));
  });
  expect(document.activeElement).toBe(items[1]);
});

it("draws a tooltip trigger around a mark as no tab stop, named when given a label", () => {
  const { container } = mount(
    <Tooltip>
      <TooltipTrigger role="img" label="Running">●</TooltipTrigger>
      <TooltipContent>Running</TooltipContent>
    </Tooltip>,
  );
  const trigger = container.querySelector<HTMLElement>('[role="img"]')!;
  expect(trigger.getAttribute("aria-label")).toBe("Running");
  expect(trigger.tabIndex).toBe(-1);
  const phrasing = mount(
    <Tooltip>
      <TooltipTrigger phrasing role="presentation">●</TooltipTrigger>
      <TooltipContent>Running</TooltipContent>
    </Tooltip>,
  ).container.querySelector('[role="presentation"]')!;
  expect(phrasing.tagName).toBe("SPAN");
});

function AnchoredTooltip(): React.ReactElement {
  const anchor = useRef<HTMLSpanElement>(null);
  return (
    <div id="region">
      <span ref={anchor}>clipped</span>
      <Tooltip>
        <OpenNow />
        <TooltipContent anchor={anchor}>clipped name</TooltipContent>
      </Tooltip>
    </div>
  );
}

function OpenNow(): null {
  const state = useTooltipState();
  React.useEffect(() => state?.open(true), []);
  return null;
}

it("closes an open tooltip once what it belongs to turns inert", async () => {
  const { container } = mount(<AnchoredTooltip />);
  expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
  await act(async () => {
    container.querySelector("#region")!.setAttribute("inert", "");
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  expect(document.querySelector('[role="tooltip"]')).toBeNull();
});

it("draws a toast of any queued variant as an accent or a danger one", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
  });
  mount(<ToastRegion placement="bottom end">{(entry) => <p data-variant={entry.variant}>{entry.title}</p>}</ToastRegion>);
  let keys: string[] = [];
  act(() => {
    keys = [showToast("Failed", { variant: "danger" }), showToast("Saved", { variant: "accent" }), toast("Done", { variant: "success" })];
  });
  const variantOf = (title: string) => [...document.querySelectorAll("p")].find((p) => p.textContent === title)?.dataset.variant;
  expect([variantOf("Failed"), variantOf("Saved"), variantOf("Done")]).toEqual(["danger", "accent", "accent"]);
  act(() => keys.forEach(closeToast));
});

it("announces a menu item's shortcut as aria-keyshortcuts and clears it when the shortcut goes", () => {
  const menu = (shortcut?: string) => (
    <Menu open onOpenChange={() => {}}>
      <MenuTrigger label="More actions">…</MenuTrigger>
      <MenuPopover>
        <MenuList onAction={() => {}}>
          <MenuItem id="rename" textValue="Rename" shortcut={shortcut}>Rename</MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>
  );
  const { render } = mount(menu("r"));
  const item = () => document.querySelector<HTMLElement>('[role="menuitem"]')!;
  expect(item().getAttribute("aria-keyshortcuts")).toBe("r");
  render(menu());
  expect(item().hasAttribute("aria-keyshortcuts")).toBe(false);
});

it("activates a menu item as the keyboard would: focused, run, and the menu closed", () => {
  const onAction = vi.fn();
  const onOpenChange = vi.fn();
  mount(
    <Menu open onOpenChange={onOpenChange}>
      <MenuTrigger label="More actions">…</MenuTrigger>
      <MenuPopover>
        <MenuList onAction={onAction}>
          <MenuItem id="rename" textValue="Rename">Rename</MenuItem>
          <MenuItem id="copy" textValue="Copy">Copy</MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>,
  );
  const items = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  act(() => activateMenuItem(items[1]));
  expect(document.activeElement).toBe(items[1]);
  expect(onAction).toHaveBeenCalledWith("copy");
  expect(onOpenChange).toHaveBeenCalledWith(false);
});

it("lets the caller take over a focus restore once, and stops listening on abort", () => {
  const restore = () => {
    const event = new Event("react-aria-focus-scope-restore", { cancelable: true });
    document.dispatchEvent(event);
    return event;
  };
  const controller = new AbortController();
  const listener = vi.fn((event: Event) => event.preventDefault());
  onFocusScopeRestore(listener, controller.signal);
  expect(restore().defaultPrevented).toBe(true);
  expect(restore().defaultPrevented).toBe(false);
  expect(listener).toHaveBeenCalledTimes(1);

  const aborted = new AbortController();
  const late = vi.fn();
  onFocusScopeRestore(late, aborted.signal);
  aborted.abort();
  restore();
  expect(late).not.toHaveBeenCalled();
});

it("finds the open menu a trigger's button labels, and none for a closed one", () => {
  const menu = (open: boolean) => (
    <Menu open={open} onOpenChange={() => {}}>
      <MenuTrigger label="More actions">…</MenuTrigger>
      <MenuPopover>
        <MenuList onAction={() => {}}>
          <MenuItem id="rename" textValue="Rename">Rename</MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>
  );
  const { container, render } = mount(menu(true));
  const trigger = container.querySelector("button")!;
  expect(menuOfTrigger(trigger)).toBe(document.querySelector('[role="menu"]'));
  expect(menuOfTrigger(trigger)).not.toBeNull();
  render(menu(false));
  expect(menuOfTrigger(trigger)).toBeNull();
  expect(menuOfTrigger(null)).toBeNull();
});

it("tells an open tooltip and a live toast apart from their closing forms by the part selectors", () => {
  mount(<AnchoredTooltip />);
  const tooltip = document.querySelector('[role="tooltip"]')!;
  expect(tooltip.matches(partSelector.liveTooltip)).toBe(true);
  tooltip.setAttribute("data-exiting", "true");
  expect(tooltip.matches(partSelector.liveTooltip)).toBe(false);
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
  });
  mount(<ToastRegion placement="bottom end">{(entry) => <Toast variant={entry.variant}><ToastContent>{entry.title}</ToastContent></Toast>}</ToastRegion>);
  let key = "";
  act(() => {
    key = showToast("Saved", { variant: "accent" });
  });
  const toastElement = document.querySelector(partSelector.toast)!;
  expect(toastElement.matches(partSelector.liveToast)).toBe(true);
  for (const name of toastStateAttributes) {
    toastElement.setAttribute(name, "true");
    expect(toastElement.matches(partSelector.liveToast)).toBe(false);
    toastElement.removeAttribute(name);
  }
  act(() => closeToast(key));
});
