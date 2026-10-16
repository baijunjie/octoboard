// @vitest-environment jsdom
import { act, useEffect, useRef } from "react";
import { createRoot } from "react-dom/client";
import { Trash2 } from "lucide-react";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { ActionMenu, type ActionMenuEntry, type ActionMenuPanelContext } from "./ActionMenu";

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

it.each(["keyboard", "pointer"])("runs a third-level branch action exactly once with %s", async (input) => {
  const picked = vi.fn();
  const wrong = vi.fn();
  await openMenu([
    { label: "Unrelated root", icon: Trash2, onClick: wrong },
    { label: "Git", icon: Trash2, items: [
      { label: "Unrelated Git action", icon: Trash2, onClick: wrong },
      { label: "Switch branch", icon: Trash2, items: [
        { label: "main", icon: Trash2, selected: true, onClick: () => {} },
        { label: "feature", icon: Trash2, selected: false, onClick: picked },
      ] },
    ] },
  ]);
  const find = (label: string) => [...document.querySelectorAll<HTMLElement>("[role^=menuitem]")].find((item) => item.textContent === label)!;
  for (const label of ["Git", "Switch branch"]) {
    await act(async () => {
      const item = find(label);
      item.focus();
      item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    });
  }
  expect(document.querySelectorAll("[role=menu]")).toHaveLength(3);
  await act(async () => {
    const item = find("feature");
    item.focus();
    if (input === "keyboard") {
      item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      item.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
    } else {
      item.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      item.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      item.click();
    }
  });
  expect(picked).toHaveBeenCalledTimes(1);
  expect(wrong).not.toHaveBeenCalled();
});

it("loads a submenu on open and retries its failed load without closing", async () => {
  const load = vi.fn().mockRejectedValueOnce(new Error("repository unavailable")).mockResolvedValueOnce([
    { label: "feature", icon: Trash2, onClick: () => {} },
  ]);
  await openMenu([{ label: "Switch branch", icon: Trash2, items: [], loadItems: load }]);
  expect(load).not.toHaveBeenCalled();
  await act(async () => {
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    item.focus();
    item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  });
  expect(load).toHaveBeenCalledTimes(1);
  expect(document.body.textContent).toContain("repository unavailable");
  await act(async () => {
    const retry = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Try again")!;
    retry.focus();
    retry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    retry.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  });
  expect(load).toHaveBeenCalledTimes(2);
  expect(document.querySelectorAll("[role=menu]")).toHaveLength(2);
  expect(document.body.textContent).toContain("feature");
});

/** Content that puts focus on its own first control as it mounts when asked to, as a list box does. */
function FocusingContent({ autoFocus }: { autoFocus: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (autoFocus) ref.current?.querySelector("button")?.focus();
  }, []);
  return (
    <div ref={ref} role="listbox" aria-label="Swatches">
      <button type="button" tabIndex={0} data-panel-edge="start">first</button>
      {/* Content that moves focus within itself handles the arrow keys itself. */}
      <button type="button" tabIndex={-1} onKeyDown={(event) => event.stopPropagation()}>second</button>
    </div>
  );
}

describe("a panel entry", () => {
  let seen: { autoFocus?: boolean; labelledBy?: string; close?: () => void } = {};
  beforeEach(() => {
    seen = {};
  });
  const panel = [{
    label: "Colour",
    icon: Trash2,
    panel: (context: ActionMenuPanelContext) => {
      Object.assign(seen, context);
      return <FocusingContent autoFocus={context.autoFocus} />;
    },
  }];
  const item = () => document.querySelector<HTMLElement>("[role=menuitem]")!;
  const press = (target: Element, name: string) => act(async () => void target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true })));
  const button = (text: string) => [...document.querySelectorAll<HTMLElement>("button")].find((el) => el.textContent === text)!;
  const menus = () => document.querySelectorAll("[role=menu]").length + document.querySelectorAll("[role=listbox]").length;

  async function openByKeyboard() {
    await openMenu(panel);
    await act(async () => {
      item().focus();
      item().dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    });
  }

  it("opens from the keyboard onto its content, named by the item", async () => {
    await openByKeyboard();
    expect(seen.autoFocus).toBe(true);
    expect(seen.labelledBy).toBe(item().id);
    expect(document.activeElement).toBe(button("first"));
  });

  it("leaves towards the menu only from its start edge, and Escape closes only the panel", async () => {
    await openByKeyboard();
    await act(async () => button("second").focus());
    await press(button("second"), "ArrowLeft");
    expect(document.activeElement).toBe(button("second"));

    await press(button("first"), "ArrowLeft");
    expect(document.querySelector("[role=listbox]")).toBeNull();
    expect(document.activeElement).toBe(item());

    await press(item(), "ArrowRight");
    await press(button("first"), "Escape");
    expect(document.querySelector("[role=listbox]")).toBeNull();
    expect(item()).not.toBeNull();
  });

  it("closes the whole menu through close()", async () => {
    await openByKeyboard();
    await act(async () => seen.close!());
    expect(menus()).toBe(0);
  });

  it("is entered with the arrow key after a hover opened it", async () => {
    vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
    onTestFinished(() => void vi.unstubAllGlobals());
    await openMenu(panel);
    await act(async () => {
      item().dispatchEvent(new PointerEvent("pointerover", { bubbles: true, pointerType: "mouse" }));
      item().dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse" }));
      await new Promise((resolve) => setTimeout(resolve, 300));
    });
    expect(document.querySelector("[role=listbox]")).not.toBeNull();
    expect(seen.autoFocus).toBe(false);
    // The hover has focused the item, as it does in a browser.
    await act(async () => item().focus());
    await press(item(), "ArrowRight");
    expect(document.activeElement).toBe(button("first"));
  });
});
