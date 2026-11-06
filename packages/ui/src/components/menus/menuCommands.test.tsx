// @vitest-environment jsdom
import { act, useState } from "react";
import { Focus } from "lucide-react";
import { expect, it, onTestFinished, vi } from "vitest";

import { Menu, MenuPopover, MenuTrigger } from "../../primitives";
import { eventKey } from "../../shortcuts/binding";
import { defaultBinding } from "../../shortcuts/commands";
import { matchesCommand } from "../../shortcuts/store";
import { recordKeys } from "../../shortcuts/recording";
import { useWindowShortcut } from "../../shortcuts/useWindowShortcut";
import { mountHost } from "../../test/mountHost";
import { MenuItems } from "./MenuItems";
import { MenuShortcutScope } from "./menuShortcutScope";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function mount(disabled = false) {
  const command = vi.fn();
  const childCommand = vi.fn();
  const windowCommand = vi.fn();
  const matches = (event: KeyboardEvent) => matchesCommand("focusMode", event);
  const shortcut = { label: "Shift+Control+F", matches };
  let deactivate!: () => void;
  function View() {
    const [open, setOpen] = useState(false);
    const [active, setActive] = useState(true);
    deactivate = () => setActive(false);
    useWindowShortcut((event) => matches(event) ? true : undefined, windowCommand);
    useWindowShortcut((event) => matchesCommand("openSettings", event) ? true : undefined, windowCommand);
    return <Menu open={open} onOpenChange={setOpen}>
      <MenuTrigger label="Actions">Open</MenuTrigger>
      <MenuPopover>
        <MenuShortcutScope value={active}>
          <MenuItems entries={[
            { label: "Focus mode", icon: Focus, commandShortcut: shortcut, onClick: command, disabled },
            { label: "Child", icon: Focus, items: [{ label: "Child focus", icon: Focus, commandShortcut: shortcut, onClick: childCommand }] },
          ]} />
        </MenuShortcutScope>
      </MenuPopover>
    </Menu>;
  }
  const host = mountHost(<View />);
  await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
  const menu = document.querySelector<HTMLElement>("[role=menu]")!;
  act(() => menu.focus());
  const press = (target: EventTarget = menu, init: KeyboardEventInit = {}) => {
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...eventKey(defaultBinding("focusMode")), ...init });
    act(() => { target.dispatchEvent(event); });
    return event;
  };
  return { menu, command, childCommand, windowCommand, press, deactivate: () => act(deactivate) };
}

it("lets only an advertised matching window command activate the root item", async () => {
  const { menu, command, windowCommand, press } = await mount();
  expect(press(menu, eventKey(defaultBinding("openSettings"))).defaultPrevented).toBe(true);
  expect(command).not.toHaveBeenCalled();
  expect(windowCommand).not.toHaveBeenCalled();
  expect(press().defaultPrevented).toBe(true);
  expect(command).toHaveBeenCalledOnce();
  expect(windowCommand).not.toHaveBeenCalled();
  expect(document.querySelector("[role=menu]")).toBeNull();
});

it("does not delegate while a child menu is open, even when its item advertises that command", async () => {
  const { menu, command, childCommand, press } = await mount();
  const child = [...menu.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Child")!;
  act(() => {
    child.focus();
    child.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true }));
  });
  await act(async () => {});
  expect(document.querySelectorAll("[role=menu]")).toHaveLength(2);
  press(document.activeElement!);
  expect(command).not.toHaveBeenCalled();
  expect(childCommand).not.toHaveBeenCalled();
});

it("leaves inactive content alone during a menu's closing phase", async () => {
  const { command, press, deactivate } = await mount();
  deactivate();
  expect(press().defaultPrevented).toBe(true);
  expect(command).not.toHaveBeenCalled();
});

it("does not let delegation bypass a disabled item", async () => {
  const { command, press } = await mount(true);
  press();
  expect(command).not.toHaveBeenCalled();
});

it("lets shortcut recording consume the chord before an enabled menu command", async () => {
  const { command, press } = await mount();
  const recorded = vi.fn(() => "consumed" as const);
  const stop = recordKeys(recorded);
  onTestFinished(stop);
  press();
  expect(recorded).toHaveBeenCalledOnce();
  expect(command).not.toHaveBeenCalled();
  stop();
  press();
  expect(command).toHaveBeenCalledOnce();
});
