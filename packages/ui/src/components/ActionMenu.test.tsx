// @vitest-environment jsdom
import { act, useEffect, useRef } from "react";
import { createRoot } from "react-dom/client";
import { Trash2 } from "lucide-react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { detailsButton } from "./detailsTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "./StatusAnnouncer";
import { ActionMenu, type ActionMenuEntry, type ActionMenuPanelContext } from "./ActionMenu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let unmount: (() => void) | undefined;

afterEach(() => {
  unmount?.();
  unmount = undefined;
});

beforeEach(() => {
  const modality = getInteractionModality();
  onTestFinished(() => {
    // The getter can return the initial null state, which the setter accepts at runtime.
    act(() => setInteractionModality(modality as Parameters<typeof setInteractionModality>[0]));
  });
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

it("draws a separator only between two items", async () => {
  const item = (label: string): ActionMenuEntry => ({ label, icon: Trash2, onClick: () => {} });
  await openMenu(["separator", item("A"), "separator", "separator", item("B"), "separator"]);
  const menu = document.querySelector("[role=menu]")!;
  expect([...menu.children].map((el) => el.getAttribute("role") ?? el.textContent)).toEqual(["menuitem", "separator", "menuitem"]);
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
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  await openMenu([{ label: "Switch branch", icon: Trash2, items: [], loadItems: load }]);
  expect(load).not.toHaveBeenCalled();
  // The last input was a pointer, so the ring is shown only if the hand-off sets the modality.
  act(() => setInteractionModality("pointer"));
  await act(async () => {
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    item.focus();
    item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  });
  expect(load).toHaveBeenCalledTimes(1);
  // The failure is one Alert in the submenu's popover holding the application's sentence, the system's
  // text behind Details, and Try again, not a disabled item; the raw text is not in the visible copy.
  const alert = document.querySelector('[data-slot="alert-root"]')!;
  expect(alert.querySelector('[data-slot="alert-title"]')?.textContent).toBe("Could not load the items");
  expect(alert.querySelector('[data-slot="alert-description"]')?.textContent).not.toContain("repository unavailable");
  expect(detailsButton(alert)).toBeDefined();
  const retry = [...alert.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  expect(retry).toBeDefined();
  expect([...document.querySelectorAll("[role=menuitem]")].some((item) => item.textContent?.includes("repository unavailable"))).toBe(false);
  // Focus that was in the submenu moves to Try again instead of falling to the page, with its ring
  // shown, and the failure is not an error alert dialog. The alert announces itself once that focus
  // move is past, not at the moment it mounts.
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(document.activeElement).toBe(retry);
  expect(getInteractionModality()).toBe("keyboard");
  expect(alert.querySelector("[role=alert]")).toBeNull();
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(alert.querySelector("[role=alert]")?.textContent).toContain("Could not load the items");
  await act(async () => retry.click());
  expect(load).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(document.querySelectorAll("[role=menu]")).toHaveLength(2);
  expect(document.body.textContent).toContain("feature");
  // ... and from there into the menu that took its place.
  expect(document.activeElement).not.toBe(document.body);
  expect(document.activeElement?.closest("[role=menu]")).not.toBeNull();
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

describe("a disabled item", () => {
  const key = (target: Element, name: string) =>
    act(async () => {
      target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
      target.dispatchEvent(new KeyboardEvent("keyup", { key: name, bubbles: true, cancelable: true }));
    });

  it("stays reachable with the arrow keys but does not run", async () => {
    const onClick = vi.fn();
    const onOther = vi.fn();
    await openMenu([
      { label: "Off", icon: Trash2, disabled: true, onClick },
      { label: "On", icon: Trash2, onClick: onOther },
    ]);
    const menu = document.querySelector<HTMLElement>("[role=menu]")!;
    await key(menu, "ArrowDown");
    const off = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === "Off")!;
    expect(document.activeElement).toBe(off);
    expect(off.getAttribute("aria-disabled")).toBe("true");
    await key(off, "Enter");
    expect(onClick).not.toHaveBeenCalled();
  });

  it("stays reachable inside a group of selectable items, and Enter on it neither runs nor selects", async () => {
    const onOff = vi.fn();
    await openMenu([
      { label: "Current", icon: Trash2, selected: true, onClick: () => {} },
      { label: "Off", icon: Trash2, selected: false, disabled: true, onClick: onOff },
      { label: "Next", icon: Trash2, selected: false, onClick: () => {} },
    ]);
    const menu = document.querySelector<HTMLElement>("[role=menu]")!;
    const radio = (label: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitemradio]")].find((el) => el.textContent === label)!;
    await key(menu, "ArrowDown");
    expect(document.activeElement).toBe(radio("Current"));
    await key(document.activeElement!, "ArrowDown");
    expect(document.activeElement).toBe(radio("Off"));
    await key(radio("Off"), "Enter");
    await key(document.activeElement!, "ArrowDown");
    expect(document.activeElement).toBe(radio("Next"));
    expect(onOff).not.toHaveBeenCalled();
    expect(radio("Off").getAttribute("aria-checked")).toBe("false");
    expect(radio("Current").getAttribute("aria-checked")).toBe("true");
  });

  it("with a reason is disabled, shows the reason under its label and is named with it", async () => {
    await openMenu([{ label: "Work", icon: Trash2, reason: { state: "used up" }, onClick: () => {} }]);
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    expect(item.getAttribute("aria-disabled")).toBe("true");
    expect(item.getAttribute("aria-label")).toContain("used up");
    expect(item.getAttribute("aria-label")).toContain("Work");
    expect(item.querySelector("[aria-hidden=true].text-xs")?.textContent).toBe("used up");
  });

  it("sets a reason with a detail on two lines and names the item with both", async () => {
    await openMenu([{ label: "Work", icon: Trash2, reason: { state: "Used up", detail: "resets in 2h" }, onClick: () => {} }]);
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    expect(item.getAttribute("aria-label")).toContain("Used up · resets in 2h");
    const lines = Array.from(item.querySelectorAll("[aria-hidden=true].text-xs span"), (line) => line.textContent);
    expect(lines).toEqual(["Used up", "resets in 2h"]);
  });

  it("names an item with a description and a reason by its label alone, the description carrying the reason", async () => {
    await openMenu([{ label: "Work", icon: Trash2, reason: { state: "Used up" }, description: "Used up: weekly", onClick: () => {} }]);
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    expect(item.getAttribute("aria-label")).toBeNull();
    expect(document.getElementById(item.getAttribute("aria-describedby")!)?.textContent).toBe("Used up: weekly");
  });

  it("gives an item's description to assistive technology without drawing it", async () => {
    await openMenu([{ label: "Work", icon: Trash2, description: "60% left", onClick: () => {} }]);
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    const description = document.getElementById(item.getAttribute("aria-describedby")!);
    expect(description?.textContent).toBe("60% left");
    expect(description?.classList.contains("sr-only")).toBe(true);
  });
});

describe("the Menu key and Shift+F10 on the context target", () => {
  function mountTarget(): { target: HTMLElement; inner: HTMLElement } {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    unmount = () => {
      act(() => root.unmount());
      container.remove();
    };
    function Target() {
      const ref = useRef<HTMLDivElement>(null);
      return (
        <div ref={ref} tabIndex={0} data-testid="target">
          <span tabIndex={-1} data-testid="inner" />
          <ActionMenu label="Actions of X" items={[{ label: "Delete", icon: Trash2, onClick: () => {} }]} contextTargetRef={ref} />
        </div>
      );
    }
    act(() => root.render(<Target />));
    return { target: container.querySelector<HTMLElement>("[data-testid=target]")!, inner: container.querySelector<HTMLElement>("[data-testid=inner]")! };
  }

  it.each([{ key: "ContextMenu" }, { key: "F10", shiftKey: true }])("opens the menu from %o, cancelling the key's own context menu, and gives focus back to the target", async (init) => {
    const { target } = mountTarget();
    act(() => target.focus());
    let unprevented = true;
    act(() => {
      unprevented = target.dispatchEvent(new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true }));
    });
    expect(unprevented).toBe(false);
    expect(document.querySelector("[role=menu]")).not.toBeNull();
    // The first item, as for a menu opened from its trigger with the keyboard, so Enter acts at once.
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    await vi.waitFor(() => expect(document.activeElement).toBe(item));
    act(() => {
      item.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    });
    await vi.waitFor(() => expect(document.querySelector("[role=menu]")).toBeNull());
    await vi.waitFor(() => expect(document.activeElement).toBe(target));
  });

  it("skips the contextmenu event a browser raises after the key it opened from, and opens again on the next right-click", async () => {
    const { target } = mountTarget();
    act(() => target.focus());
    act(() => {
      target.dispatchEvent(new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true }));
    });
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    act(() => item.focus());
    let late = true;
    act(() => {
      late = item.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    });
    expect(late).toBe(false);
    expect(document.querySelectorAll("[role=menu]")).toHaveLength(1);
    expect(document.activeElement).toBe(item);
    act(() => {
      item.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    });
    await vi.waitFor(() => expect(document.querySelector("[role=menu]")).toBeNull());
    act(() => {
      target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 }));
    });
    expect(document.querySelector("[role=menu]")).not.toBeNull();
  });

  it("focuses the menu itself, not its first item, when a right-click opens it", async () => {
    const { target } = mountTarget();
    act(() => {
      target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 }));
    });
    await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
  });

  it("leaves a Menu key that an element inside the target has already handled", () => {
    const { inner } = mountTarget();
    inner.addEventListener("keydown", (event) => event.preventDefault());
    act(() => inner.focus());
    act(() => {
      inner.dispatchEvent(new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true }));
    });
    expect(document.querySelector("[role=menu]")).toBeNull();
  });
});
