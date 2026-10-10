// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { Trash2 } from "lucide-react";
import { afterEach, expect, it } from "vitest";

import { ActionMenu, type ActionMenuEntry } from "./ActionMenu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let unmount: (() => void) | undefined;

afterEach(() => {
  unmount?.();
  unmount = undefined;
});

async function openMenu(items: ActionMenuEntry[]): Promise<void> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  unmount = () => {
    act(() => root.unmount());
    container.remove();
  };
  act(() => {
    root.render(<ActionMenu label="Actions of X" items={items} />);
  });
  const trigger = container.querySelector("button")!;
  await act(async () => {
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
}

/** The roles of the open menu's elements that carry an accessible name of their own. */
function namedRoles(): (string | null)[] {
  return [...document.querySelectorAll("[role=dialog], [role=menu], [role=group]")]
    .filter((el) => el.getAttribute("aria-label") || el.getAttribute("aria-labelledby"))
    .map((el) => el.getAttribute("role"));
}

it("names an open menu once: on the menu, not also on its popover dialog", async () => {
  await openMenu([{ label: "Delete", icon: Trash2, onClick: () => {} }]);
  expect(document.querySelectorAll("[role=dialog]")).toHaveLength(1);
  expect(namedRoles()).toEqual(["menu"]);
});

it("names a submenu once, though its items form a single-selection group", async () => {
  await openMenu([
    {
      label: "Switch account",
      icon: Trash2,
      items: [
        { label: "A", icon: Trash2, selected: true, onClick: () => {} },
        { label: "B", icon: Trash2, selected: false, onClick: () => {} },
      ],
    },
  ]);
  const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
  await act(async () => {
    item.focus();
    item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  });
  expect(document.querySelectorAll("[role=dialog]")).toHaveLength(2);
  expect(document.querySelector("[role=group]")).not.toBeNull();
  expect(namedRoles()).toEqual(["menu", "menu"]);
});
