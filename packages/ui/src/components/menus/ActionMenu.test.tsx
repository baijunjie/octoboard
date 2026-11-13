// @vitest-environment jsdom
import { act, type ComponentProps, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { Trash2 } from "lucide-react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import { errorAlerts } from "../alerts/errorAlerts";
import { detailsButton } from "../alerts/detailsTestSupport";
import { AnchoredTooltip } from "../hint/AnchoredTooltip";
import { closeOpenTooltip, tooltipsCooledDown } from "../hint/tooltipTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../announce/StatusAnnouncer";
import { ActionMenu } from "./ActionMenu";
import { filterField, holdExitAnimations, openBy, pressInFilter, SUBMENU_HOVER_DELAY_MS, typeInFilter } from "./menuTestSupport";
import { runMenuCommand } from "./menuCommands";
import type { ActionMenuEntry, ActionMenuItem, ActionMenuPanelContext } from "./menuEntries";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { partSelector } from "../../primitives";

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
    closeOpenTooltip();
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
  fakeTimers();
  await openMenu([{ label: "Switch branch", icon: Trash2, items: [], loadItems: load }]);
  expect(load).not.toHaveBeenCalled();
  // The last input was a pointer, so the ring is shown only if the handoff sets the modality.
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

it.each([
  ["a mouse press", "pointer", () => new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0, width: 1, height: 1 })],
  ["a key", "keyboard", () => new KeyboardEvent("keydown", { key: "F1", bubbles: true })],
] as const)("shows the ring on the Try again that a failed load hands focus to only after a key, not after a press: %s", async (_, modality, input) => {
  let fail!: (error: Error) => void;
  const load = vi.fn(() => new Promise<ActionMenuEntry[]>((_, reject) => (fail = reject)));
  await openMenu([{ label: "Switch branch", icon: Trash2, items: [], loadItems: load }]);
  await act(async () => {
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    item.focus();
    item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  });
  expect(load).toHaveBeenCalledTimes(1);
  expect(document.activeElement?.closest("[role=menu]")).not.toBeNull();

  // The last input before the load fails, the way the user was last interacting with the open menu.
  act(() => void document.activeElement!.dispatchEvent(input()));
  await act(async () => fail(new Error("repository unavailable")));
  const retry = [...document.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  expect(document.activeElement).toBe(retry);
  expect(getInteractionModality()).toBe(modality);
  expect(retry.hasAttribute("data-focus-visible")).toBe(modality === "keyboard");
});

it("offers Download Git before Try again in a submenu whose load found no Git", async () => {
  const load = vi.fn().mockRejectedValueOnce(new CodedError("`git` was not found", "binary_not_found", { binary: "git" })).mockResolvedValueOnce([
    { label: "feature", icon: Trash2, onClick: () => {} },
  ]);
  await openMenu([{ label: "Switch branch", icon: Trash2, items: [], loadItems: load }]);
  await act(async () => {
    const item = document.querySelector<HTMLElement>("[role=menuitem]")!;
    item.focus();
    item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  });
  const alert = document.querySelector<HTMLElement>('[data-slot="alert-root"]')!;
  expect(alert.textContent).toContain("Git is not installed, or it is not on PATH.");
  const controls = [...alert.querySelectorAll("a, button")].map((control) => control.textContent);
  expect(controls.slice(-2)).toEqual([expect.stringContaining("Download Git"), "Try again"]);
  expect(alert.querySelector("a")?.getAttribute("href")).toBe("https://git-scm.com/downloads");
  // Try again is the last control, which the failure's focus move lands on.
  expect(document.activeElement).toBe([...alert.querySelectorAll("button")].find((button) => button.textContent === "Try again"));
  await act(async () => (document.activeElement as HTMLElement).click());
  expect(load).toHaveBeenCalledTimes(2);
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
    fakeTimers();
    await openMenu(panel);
    await act(async () => {
      item().dispatchEvent(new PointerEvent("pointerover", { bubbles: true, pointerType: "mouse" }));
      item().dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse" }));
      await vi.advanceTimersByTimeAsync(SUBMENU_HOVER_DELAY_MS);
    });
    expect(document.querySelector("[role=listbox]")).not.toBeNull();
    expect(seen.autoFocus).toBe(false);
    // The hover has focused the item, as it does in a browser.
    await act(async () => item().focus());
    await press(item(), "ArrowRight");
    expect(document.activeElement).toBe(button("first"));
  });
});

describe("a note", () => {
  it("is a muted line with no glyph that stays in the arrow-key walk, is read there, and cannot run anything", async () => {
    const onClick = vi.fn();
    await openMenu([
      { label: "Main", icon: Trash2, onClick },
      { label: "There are more than this list can show.", note: true },
    ]);
    const menu = document.querySelector<HTMLElement>("[role=menu]")!;
    const items = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
    const note = items.find((el) => el.textContent === "There are more than this list can show.")!;
    expect(note.querySelector("svg")).toBeNull();
    // Drawn as quiet text, not as a faded (disabled) item.
    expect(note.className).toContain("ui-disabled:opacity-100");
    expect(note.querySelector("label, span:not([aria-hidden])")?.className).toContain("text-app-muted");
    expect(note.querySelector('[data-slot="label"]')?.className).toContain("opacity-100");
    expect(note.getAttribute("aria-disabled")).toBe("true");
    await act(async () => void menu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true })));
    await act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true })));
    expect(document.activeElement).toBe(note);
    await act(async () => void note.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })));
    expect(onClick).not.toHaveBeenCalled();
    expect(document.querySelector("[role=menu]")).not.toBeNull();
  });
});

describe("a note in a menu of choices", () => {
  const noteOf = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === "There are more than this list can show.")!;

  it("reserves the check indicator's column too, so its text lines up with the labels", async () => {
    await openMenu([
      { label: "main", icon: Trash2, onClick: vi.fn(), selected: true },
      { label: "There are more than this list can show.", note: true },
    ]);
    expect(noteOf().className).toContain("ps-7");
  });

  it("keeps the plain indent in a menu with none", async () => {
    await openMenu([
      { label: "Main", icon: Trash2, onClick: vi.fn() },
      { label: "There are more than this list can show.", note: true },
    ]);
    expect(noteOf().className).not.toContain("ps-7");
  });
});

it("lets the pointer reach every item's label content, so a clipped label can open its tooltip, and keeps a disabled row from lighting or shrinking under it", async () => {
  await openMenu([
    { label: "Plain", icon: Trash2, onClick: () => {} },
    { label: "Own", content: <span>Own</span>, icon: Trash2, onClick: () => {} },
    { label: "Sub", icon: Trash2, items: [{ label: "Inner", icon: Trash2, onClick: () => {} }] },
    { label: "Off", icon: Trash2, disabled: true, onClick: () => {} },
  ]);
  const item = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === name)!;
  // HeroUI's label ignores the pointer, and the class gives it back to the label's children (jsdom has no CSS to measure).
  const label = (name: string) => item(name).querySelector('[data-slot="label"]')!.className;
  for (const name of ["Plain", "Own", "Sub", "Off"]) expect(label(name)).toContain("*:pointer-events-auto");
  // The cursor over it stays the row's: the hand where the item can be chosen, the arrow on a disabled row.
  for (const name of ["Plain", "Own", "Sub"]) expect(label(name)).toContain("cursor-[inherit]");
  expect(label("Off")).toContain("cursor-default");
  // HeroUI's `.menu-item:hover` fill and `:active` shrink, which the disabled row now matches under its label.
  expect(item("Off").className).toContain("ui-disabled:hover:bg-transparent");
  expect(item("Off").className).toContain("ui-disabled:active:transform-none");
});

describe("Escape with a label's tooltip open", () => {
  /** A label whose tooltip shows as the menu opens, as a clipped one's does under the pointer. */
  function Hovered({ text, immediate = true }: { text: string; immediate?: boolean }) {
    const anchor = useRef<HTMLSpanElement>(null);
    return (
      <>
        <span ref={anchor}>{text}</span>
        <AnchoredTooltip tooltip={`The whole ${text}`} anchor={anchor} hovered immediate={immediate} />
      </>
    );
  }
  const item = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === name)!;
  const tooltip = () => document.querySelector('[role="tooltip"]');
  const escape = (target: Element) => act(async () => void target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));

  it("closes only the tooltip, leaving the menu open with focus where it was, and the next Escape closes the menu", async () => {
    await openMenu([
      { label: "Plain", icon: Trash2, onClick: () => {} },
      { label: "Long", content: <Hovered text="Long" />, icon: Trash2, onClick: () => {} },
    ]);
    await act(async () => item("Plain").focus());
    expect(tooltip()?.textContent).toBe("The whole Long");

    await escape(item("Plain"));
    expect(tooltip()).toBeNull();
    expect(document.querySelector("[role=menu]")).not.toBeNull();
    expect(document.activeElement).toBe(item("Plain"));

    await escape(item("Plain"));
    expect(document.querySelector("[role=menu]")).toBeNull();
  });

  it("closes only the tooltip inside a submenu, leaving the submenu open", async () => {
    await openMenu([{ label: "Sub", icon: Trash2, items: [
      { label: "Inner", icon: Trash2, onClick: () => {} },
      { label: "Long", content: <Hovered text="Long" />, icon: Trash2, onClick: () => {} },
    ] }]);
    await act(async () => {
      item("Sub").focus();
      item("Sub").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    });
    expect(document.activeElement).toBe(item("Inner"));
    expect(tooltip()?.textContent).toBe("The whole Long");

    await escape(item("Inner"));
    expect(tooltip()).toBeNull();
    expect(document.querySelectorAll("[role=menu]")).toHaveLength(2);
    expect(document.activeElement).toBe(item("Inner"));

    await escape(item("Inner"));
    expect(document.querySelectorAll("[role=menu]")).toHaveLength(1);
  });

  it("closes the menu while the tooltip is still waiting to show", async () => {
    await tooltipsCooledDown();
    // The tooltip's delay stays pending on the fake clock while the menu is open and closed.
    fakeTimers();
    await openMenu([
      { label: "Plain", icon: Trash2, onClick: () => {} },
      { label: "Long", content: <Hovered text="Long" immediate={false} />, icon: Trash2, onClick: () => {} },
    ]);
    await act(async () => item("Plain").focus());
    expect(tooltip()).toBeNull();

    await escape(item("Plain"));
    expect(document.querySelector("[role=menu]")).toBeNull();
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

  it("is dimmed by its label's colour and its icon's fade, never by fading the row, with or without a reason", async () => {
    await openMenu([
      { label: "Plain", icon: Trash2, disabled: true, onClick: () => {} },
      { label: "Reasoned", icon: Trash2, reason: { state: "used up" }, onClick: () => {} },
      { label: "On", icon: Trash2, onClick: () => {} },
    ]);
    const item = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent?.startsWith(name))!;
    for (const name of ["Plain", "Reasoned"]) {
      expect(item(name).className).toContain("ui-disabled:opacity-100");
      expect(item(name).querySelector('[data-slot="label"]')?.className).toContain("text-app-muted");
      // HeroUI fades `[data-disabled=true] .label`; the class cancels it (jsdom has no CSS to measure).
      expect(item(name).querySelector('[data-slot="label"]')?.className).toContain("opacity-100");
      expect(item(name).querySelector('span.flex.shrink-0[class*="opacity-(--disabled-opacity)"] svg')).not.toBeNull();
    }
    expect(item("On").className).not.toContain("opacity-100");
    expect(item("On").querySelector('[data-slot="label"]')?.className).not.toContain("text-app-muted");
    expect(item("On").querySelector('[data-slot="label"]')?.className).not.toContain("opacity-100");
    expect(item("On").querySelector('[class*="opacity-(--disabled-opacity)"]')).toBeNull();
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
  function mountTarget(
    leavesContextToBrowser?: (event: MouseEvent) => boolean,
    items: ActionMenuEntry[] = [{ label: "Delete", icon: Trash2, onClick: () => {} }],
  ): { target: HTMLElement; inner: HTMLElement } {
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
          <ActionMenu label="Actions of X" items={items} contextTargetRef={ref} leavesContextToBrowser={leavesContextToBrowser} />
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

  // react-aria keeps the focus strategy of the trigger's last opening (the first item for a screen reader's press or a
  // key, the last for ArrowUp) and hands it to every later opening that does not go through the trigger. Enter and
  // Space leave the same strategy as a screen reader's press, so only the press stands for them. The menu has two items
  // so that the first and the last are told apart.
  it.each([
    ["a screen reader's press", (trigger: HTMLElement) => trigger.click()],
    ["ArrowUp", (trigger: HTMLElement) => void trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true, cancelable: true }))],
  ])("focuses the menu on a right-click and the first item on the Menu key after %s opened it from its trigger", async (_, openFromTrigger) => {
    const { target } = mountTarget(undefined, [
      { label: "Delete", icon: Trash2, onClick: () => {} },
      { label: "Archive", icon: Trash2, onClick: () => {} },
    ]);
    const trigger = target.querySelector<HTMLElement>("button")!;
    const closeMenu = async () => {
      act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
      await vi.waitFor(() => expect(document.querySelector("[role=menu]")).toBeNull());
    };
    act(() => trigger.focus());
    await act(async () => openFromTrigger(trigger));
    await vi.waitFor(() => expect(document.activeElement?.closest("[role=menu]")).not.toBeNull());
    await closeMenu();

    act(() => {
      target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 }));
    });
    await vi.waitFor(() => expect(document.querySelector("[role=menu]")).not.toBeNull());
    await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
    await closeMenu();

    act(() => target.focus());
    act(() => void target.dispatchEvent(new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true })));
    await vi.waitFor(() => expect(document.activeElement).toBe(document.querySelectorAll("[role=menuitem]")[0]));
  });

  // A browser's second right-click while the menu is open: the surface behind a modal menu is inert, so the press lands on
  // <body>, closing the menu, and the `contextmenu` event that follows opens it again.
  it("takes focus into the menu when a second right-click closes and opens it within one gesture", async () => {
    const { target } = mountTarget();
    const rightClickTarget = async () => {
      // Both events in one batch, so the popover has not unmounted between them (a browser flushes between its press and its
      // context menu event).
      await act(async () => {
        document.body.dispatchEvent(new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true }));
        target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 }));
      });
    };
    await rightClickTarget();
    await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
    // What the inert surface does to the menu's focus in a browser.
    await act(async () => { (document.activeElement as HTMLElement).blur(); });
    await rightClickTarget();
    await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
    act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
    await vi.waitFor(() => expect(document.querySelector("[role=menu]")).toBeNull());
  });

  it("leaves a right-click the target judges for the browser alone, and still opens from the keyboard there", async () => {
    const { target, inner } = mountTarget((event) => event.target === inner);
    const press = new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true });
    inner.dispatchEvent(press);
    expect(press.defaultPrevented).toBe(false);
    let unprevented = false;
    act(() => {
      unprevented = inner.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    });
    expect(unprevented).toBe(true);
    expect(document.querySelector("[role=menu]")).toBeNull();

    act(() => target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })));
    expect(document.querySelector("[role=menu]")).not.toBeNull();
    act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
    await vi.waitFor(() => expect(document.querySelector("[role=menu]")).toBeNull());

    act(() => inner.focus());
    act(() => void inner.dispatchEvent(new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true })));
    expect(document.querySelector("[role=menu]")).not.toBeNull();
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

it("chooses a letter only in its open menu and closes as a normal selection does", async () => {
  const chosen = vi.fn();
  await openMenu([{ label: "Rename", icon: Trash2, shortcut: "R", onClick: chosen }]);
  const menu = document.querySelector<HTMLElement>("[role=menu]")!;
  const key = (init: KeyboardEventInit = {}) => {
    const event = new KeyboardEvent("keydown", { key: "r", bubbles: true, cancelable: true, ...init });
    act(() => menu.dispatchEvent(event));
    return event;
  };
  for (const init of [{ ctrlKey: true }, { metaKey: true }, { altKey: true }, { shiftKey: true }, { isComposing: true }, { repeat: true }]) key(init);
  expect(chosen).not.toHaveBeenCalled();
  expect(key().defaultPrevented).toBe(true);
  expect(chosen).toHaveBeenCalledOnce();
  await act(async () => {});
  expect(document.querySelector("[role=menu]")).toBeNull();
});

it("does not choose a disabled item or an item with a reason by its letter", async () => {
  const chosen = vi.fn();
  await openMenu([
    { label: "Archive", icon: Trash2, shortcut: "A", disabled: true, onClick: chosen },
    { label: "Delete", icon: Trash2, shortcut: "D", reason: { state: "Locked" }, onClick: chosen },
  ]);
  const menu = document.querySelector<HTMLElement>("[role=menu]")!;
  for (const key of ["a", "d"]) act(() => menu.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true })));
  expect(chosen).not.toHaveBeenCalled();
  expect(document.querySelector("[role=menu]")).not.toBeNull();
});

it("opens a submenu with an arrow key and keeps parent letters out of it", async () => {
  const parent = vi.fn();
  const child = vi.fn();
  await openMenu([
    { label: "Root", icon: Trash2, shortcut: "R", onClick: parent },
    { label: "Switch", icon: Trash2, items: [{ label: "Choice", icon: Trash2, shortcut: "R", onClick: child }] },
  ]);
  const branch = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Switch")!;
  expect(branch.querySelector("kbd")).toBeNull();
  expect(branch.getAttribute("aria-keyshortcuts")).toBeNull();
  act(() => {
    branch.focus();
    branch.dispatchEvent(new KeyboardEvent("keydown", { key: "s", bubbles: true, cancelable: true }));
  });
  expect(document.querySelectorAll("[role=menu]")).toHaveLength(1);
  act(() => branch.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true })));
  await act(async () => {});
  const menus = document.querySelectorAll<HTMLElement>("[role=menu]");
  expect(menus).toHaveLength(2);
  expect(menus[1].contains(document.activeElement)).toBe(true);
  act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "r", bubbles: true, cancelable: true })));
  expect(child).toHaveBeenCalledOnce();
  expect(parent).not.toHaveBeenCalled();
});


it("shows a resolving action immediately without holding up siblings, and keeps keyboard focus as it enables", async () => {
  let resolve!: (item: ActionMenuItem) => void;
  const load = vi.fn(() => new Promise<ActionMenuItem>((done) => { resolve = done; }));
  const copy = vi.fn();
  await openMenu([
    { label: "Copy relative path", icon: Trash2, onClick: () => {} },
    { label: "Copy absolute path", icon: Trash2, onClick: () => {}, load: { key: "path", run: load } },
    "separator",
    { label: "Open in", icon: Trash2, items: [{ label: "File manager", icon: Trash2, onClick: () => {} }] },
  ]);
  const items = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
  expect(items.map((item) => item.textContent)).toEqual(["Copy relative path", "Copy absolute path", "Open in"]);
  expect(items[0].getAttribute("aria-disabled")).not.toBe("true");
  expect(items[1].getAttribute("aria-disabled")).toBe("true");
  expect(document.body.textContent).not.toContain("Loading");
  act(() => items[1].focus());
  await act(async () => resolve({ label: "Copy absolute path", icon: Trash2, onClick: copy }));
  expect(document.activeElement).toBe(items[1]);
  expect(items[1].getAttribute("aria-disabled")).not.toBe("true");
  act(() => items[1].click());
  expect(copy).toHaveBeenCalledOnce();
  expect(load).toHaveBeenCalledOnce();
});

it("refreshes resolving actions on reopening and ignores a reply from the closed menu", async () => {
  const pending: ((item: ActionMenuItem) => void)[] = [];
  const load = vi.fn(() => new Promise<ActionMenuItem>((done) => pending.push(done)));
  const oldCopy = vi.fn();
  const newCopy = vi.fn();
  await openMenu([{ label: "Absolute", icon: Trash2, onClick: () => {}, load: { key: "path", run: load } }]);
  const menu = document.querySelector<HTMLElement>("[role=menu]")!;
  await act(async () => menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(document.querySelector("[role=menu]")).toBeNull();
  const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="Actions of X"]')!;
  await act(async () => trigger.click());
  expect(load).toHaveBeenCalledTimes(2);
  await act(async () => pending[0]({ label: "Absolute", icon: Trash2, onClick: oldCopy }));
  expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-disabled")).toBe("true");
  await act(async () => pending[1]({ label: "Absolute", icon: Trash2, onClick: newCopy }));
  act(() => document.querySelector<HTMLElement>("[role=menuitem]")!.click());
  expect(newCopy).toHaveBeenCalledOnce();
  expect(oldCopy).not.toHaveBeenCalled();
});

it("invalidates a resolved action when its target changes, including returning to an earlier target", async () => {
  let changeTarget!: (key: string) => void;
  const pending: ((item: ActionMenuItem) => void)[] = [];
  const load = vi.fn(() => new Promise<ActionMenuItem>((done) => pending.push(done)));
  function TargetMenu() {
    const [target, setTarget] = useState("a");
    useEffect(() => { changeTarget = setTarget; }, []);
    return <ActionMenu label="Target actions" items={() => [{ label: target, icon: Trash2, onClick: () => {}, load: { key: target, run: load } }]} />;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  unmount = () => { act(() => root.unmount()); container.remove(); };
  act(() => root.render(<TargetMenu />));
  expect(load).not.toHaveBeenCalled();
  await act(async () => container.querySelector("button")!.click());
  await act(async () => pending[0]({ label: "a", icon: Trash2, onClick: () => {} }));
  expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-disabled")).not.toBe("true");
  await act(async () => changeTarget("b"));
  expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-disabled")).toBe("true");
  await act(async () => changeTarget("a"));
  expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-disabled")).toBe("true");
  await act(async () => pending[1]({ label: "b", icon: Trash2, onClick: () => {} }));
  expect(document.querySelector("[role=menuitem]")?.textContent).toBe("a");
  expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-disabled")).toBe("true");
  await act(async () => pending[2]({ label: "a", icon: Trash2, onClick: () => {} }));
  expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-disabled")).not.toBe("true");
  expect(load).toHaveBeenCalledTimes(3);
});

type MenuProps = ComponentProps<typeof ActionMenu>;

/** Mounts an `ActionMenu` over `props` (the label and the items default) and returns its trigger and a `rerender` that
 * gives it more props. The clock is faked first, as react-aria hands focus to a filter field a frame after a pointer
 * opens the menu. */
function mount(props: Partial<MenuProps> = {}): { trigger: HTMLButtonElement; rerender: (next: Partial<MenuProps>) => void } {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  unmount = () => {
    closeOpenTooltip();
    act(() => root.unmount());
    container.remove();
  };
  let current = props;
  const render = () => act(() => root.render(<ActionMenu label="Actions of X" items={[]} {...current} />));
  render();
  return {
    trigger: container.querySelector("button")!,
    rerender: (next) => {
      current = { ...current, ...next };
      render();
    },
  };
}

describe("a filter", () => {
  const FIELD = "Find an item";
  const EMPTY = "Nothing matches";
  const FILTER = { label: FIELD, emptyMessage: EMPTY };
  const item = (label: string, onClick: () => void = () => {}, extra: Partial<ActionMenuItem> = {}): ActionMenuItem => ({ label, icon: Trash2, onClick, ...extra });
  const mountFilter = (items: ActionMenuEntry[] | (() => ActionMenuEntry[])) => mount({ items, filter: FILTER }).trigger;

  const labels = () => [...document.querySelectorAll("[role=menuitem]")].map((el) => el.textContent).filter((text) => !text?.includes(EMPTY));
  const highlighted = () => [...document.querySelectorAll("[role=menuitem]")].filter((el) => el.hasAttribute("data-focused")).map((el) => el.textContent);
  const spoken = () => [...document.querySelectorAll("[role=status]")].map((el) => el.textContent);
  const three = (picked: (label: string) => void = () => {}) => ["Website", "Search API", "Docs"].map((label) => item(label, () => picked(label)));

  it("puts a text field named as given at the top of the menu, with every item below it", async () => {
    await openBy(mountFilter(three()), "pointer");
    const menu = document.querySelector("[role=menu]")!;
    expect(filterField().getAttribute("aria-label")).toBe(FIELD);
    expect(filterField().value).toBe("");
    // The field comes before the items in the popover and is not inside the menu itself.
    expect(menu.contains(filterField())).toBe(false);
    expect(filterField().compareDocumentPosition(menu) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(labels()).toEqual(["Website", "Search API", "Docs"]);
    expect(document.body.textContent).not.toContain(EMPTY);
  });

  it("adds no field to a menu that is not given a filter", async () => {
    await openMenu(three());
    expect(document.querySelector("input")).toBeNull();
    expect(document.querySelectorAll("[role=menuitem]")).toHaveLength(3);
  });

  it.each(["pointer", "keyboard"] as const)("has the field focused as the menu opens by %s", async (by) => {
    await openBy(mountFilter(three()), by);
    expect(document.activeElement).toBe(filterField());
  });

  it.each(["pointer", "keyboard"] as const)("leaves the field editable as the menu opens by %s", async (by) => {
    await openBy(mountFilter(three()), by);
    expect(filterField().readOnly).toBe(false);
  });

  it("has the field focused but read-only as a tap opens the menu, so no on-screen keyboard covers the items", async () => {
    await openBy(mountFilter(three()), "touch");
    expect(document.querySelector("[role=menu]")).not.toBeNull();
    expect(filterField().getAttribute("aria-label")).toBe(FIELD);
    expect(document.activeElement).toBe(filterField());
    expect(filterField().readOnly).toBe(true);
    expect(labels()).toEqual(["Website", "Search API", "Docs"]);
  });

  it("lets the arrow keys highlight and Enter choose from the read-only field a tap opened the menu with", async () => {
    const picked = vi.fn();
    const trigger = mountFilter(three(picked));
    await openBy(trigger, "touch");
    await pressInFilter("ArrowDown");
    await pressInFilter("ArrowDown");
    expect(highlighted()).toEqual(["Search API"]);
    expect(document.activeElement).toBe(filterField());
    expect(filterField().readOnly).toBe(true);
    await pressInFilter("Enter");
    expect(picked).toHaveBeenCalledExactlyOnceWith("Search API");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("takes text once the read-only field is tapped, keeping focus in it, and narrows the items to what is typed", async () => {
    await openBy(mountFilter(three()), "touch");
    act(() => filterField().click());
    runOutTimers();
    expect(filterField().readOnly).toBe(false);
    expect(document.activeElement).toBe(filterField());
    await typeInFilter("doc");
    expect(labels()).toEqual(["Docs"]);
  });

  it("takes text once a printable key is pressed in the read-only field, as on a hardware keyboard", async () => {
    await openBy(mountFilter(three()), "touch");
    await pressInFilter("d");
    expect(filterField().readOnly).toBe(false);
    expect(document.activeElement).toBe(filterField());
    await typeInFilter("oc");
    expect(labels()).toEqual(["Docs"]);
  });

  it("stays read-only through the keys that type nothing: the arrows and a letter held with Control or Meta", async () => {
    await openBy(mountFilter(three()), "touch");
    await pressInFilter("ArrowDown");
    await pressInFilter("ArrowUp");
    for (const held of [{ ctrlKey: true }, { metaKey: true }, { ctrlKey: true, metaKey: true }, { metaKey: true, altKey: true }]) {
      await act(async () => void filterField().dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true, cancelable: true, ...held })));
    }
    expect(filterField().readOnly).toBe(true);
  });

  it.each([
    ["Option on macOS, which reports Alt alone", { altKey: true }],
    ["AltGr on Windows, which reports Control with Alt", { ctrlKey: true, altKey: true }],
  ])("takes text once a character typed with %s is pressed in the read-only field", async (_, held) => {
    await openBy(mountFilter(three()), "touch");
    expect(filterField().readOnly).toBe(true);
    await act(async () => void filterField().dispatchEvent(new KeyboardEvent("keydown", { key: "ç", bubbles: true, cancelable: true, ...held })));
    expect(filterField().readOnly).toBe(false);
    expect(document.activeElement).toBe(filterField());
  });

  it.each([
    ["part of a label", "arch", ["Search API"]],
    ["any letter case", "sEaRcH", ["Search API"]],
    ["leading and trailing spaces", "  docs ", ["Docs"]],
  ])("narrows the items to those whose label contains what is typed: %s", async (_, text, expected) => {
    await openBy(mountFilter(three()), "pointer");
    await typeInFilter(text);
    expect(labels()).toEqual(expected);
    expect(document.body.textContent).not.toContain(EMPTY);
  });

  it("takes spaces inside the typed text as part of what the label must contain", async () => {
    await openBy(mountFilter(three()), "pointer");
    await typeInFilter("ch a");
    expect(labels()).toEqual(["Search API"]);
  });

  it("says the given message while no item matches, to a screen reader through the status region only, and drops it once the text clears", async () => {
    const trigger = mountFilter(three());
    await openBy(trigger, "pointer");
    await typeInFilter("zzz");
    expect(labels()).toEqual([]);
    expect(filterField().value).toBe("zzz");
    expect(document.querySelector("[role=menuitem]")).toBeNull();
    expect(document.querySelector("[role=dialog]")?.textContent).toContain(EMPTY);
    expect(spoken()).toContain(EMPTY);
    // The sentence is on the screen and in the status region, and the screen's copy is hidden from the reading walk.
    const copies = [...document.querySelectorAll("*")].filter((el) => el.children.length === 0 && el.textContent === EMPTY);
    expect(copies.map((el) => el.getAttribute("role") ?? (el.closest("[aria-hidden=true]") ? "hidden" : "plain")).sort()).toEqual(["hidden", "status"]);
    // The first Escape clears the text, which brings the items back.
    await pressInFilter("Escape");
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(labels()).toEqual(["Website", "Search API", "Docs"]);
    expect(spoken()).not.toContain(EMPTY);
    expect(document.body.textContent).not.toContain(EMPTY);
  });

  it.each(["pointer", "keyboard"] as const)("moves the highlight through the matches with the arrow keys while the field keeps focus, opened by %s", async (by) => {
    await openBy(mountFilter([...three(), item("Teams")]), by);
    await typeInFilter("e");
    expect(labels()).toEqual(["Website", "Search API", "Teams"]);
    expect(highlighted()).toEqual(["Website"]);
    await pressInFilter("ArrowDown");
    expect(highlighted()).toEqual(["Search API"]);
    await pressInFilter("ArrowDown");
    expect(highlighted()).toEqual(["Teams"]);
    await pressInFilter("ArrowUp");
    expect(highlighted()).toEqual(["Search API"]);
    expect(document.activeElement).toBe(filterField());
    // Narrowing the list takes the highlight to the first item that is left.
    await typeInFilter("am");
    expect(labels()).toEqual(["Teams"]);
    expect(highlighted()).toEqual(["Teams"]);
  });

  it.each(["pointer", "keyboard"] as const)("runs the highlighted item on Enter and closes the menu, opened by %s", async (by) => {
    const picked = vi.fn();
    const trigger = mountFilter(three(picked));
    await openBy(trigger, by);
    await typeInFilter("e");
    await pressInFilter("ArrowDown");
    await pressInFilter("Enter");
    expect(picked).toHaveBeenCalledExactlyOnceWith("Search API");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("keeps one status region for the life of the open menu: empty before any typing, saying the message while no item matches, empty again once matches return", async () => {
    await openBy(mountFilter(three()), "pointer");
    const regions = () => [...document.querySelectorAll("[role=status]")];
    expect(regions()).toHaveLength(1);
    const region = regions()[0]!;
    expect(region.textContent).toBe("");
    await typeInFilter("zzz");
    expect(regions()).toEqual([region]);
    expect(region.textContent).toBe(EMPTY);
    await pressInFilter("Escape");
    expect(regions()).toEqual([region]);
    expect(region.textContent).toBe("");
  });

  it("does not count a separator as a match, so the message is spoken while only separators would be left", async () => {
    await openBy(mountFilter([item("Website"), "separator", item("Docs")]), "pointer");
    await typeInFilter("zzz");
    expect(spoken()).toContain(EMPTY);
    await pressInFilter("Escape");
    expect(spoken()).not.toContain(EMPTY);
  });

  it("moves the highlight to the row the mouse moves onto after the arrow keys highlighted another, so one row is highlighted, and Enter chooses the hovered row", async () => {
    const picked = vi.fn();
    const trigger = mountFilter([...three(picked), item("Teams", () => picked("Teams"))]);
    await openBy(trigger, "pointer");
    await typeInFilter("e");
    await pressInFilter("ArrowDown");
    expect(highlighted()).toEqual(["Search API"]);
    const teams = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === "Teams")!;
    const init = { bubbles: true, cancelable: true, pointerId: 1, isPrimary: true, pointerType: "mouse" };
    await act(async () => {
      teams.dispatchEvent(new PointerEvent("pointerover", init));
      teams.dispatchEvent(new PointerEvent("pointermove", init));
    });
    runOutTimers();
    expect(highlighted()).toEqual(["Teams"]);
    await pressInFilter("Enter");
    expect(picked).toHaveBeenCalledExactlyOnceWith("Teams");
  });

  it("runs the first match on Enter right after typing", async () => {
    const picked = vi.fn();
    const trigger = mountFilter(three(picked));
    await openBy(trigger, "pointer");
    await typeInFilter("doc");
    await pressInFilter("Enter");
    expect(picked).toHaveBeenCalledExactlyOnceWith("Docs");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("does nothing on Enter while no item matches", async () => {
    const picked = vi.fn();
    const trigger = mountFilter(three(picked));
    await openBy(trigger, "pointer");
    await typeInFilter("zzz");
    await pressInFilter("Enter");
    expect(picked).not.toHaveBeenCalled();
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
  });

  it("runs an item chosen with the pointer from the narrowed list", async () => {
    const picked = vi.fn();
    const trigger = mountFilter(three(picked));
    await openBy(trigger, "pointer");
    await typeInFilter("sea");
    await act(async () => document.querySelector<HTMLElement>("[role=menuitem]")!.click());
    expect(picked).toHaveBeenCalledExactlyOnceWith("Search API");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("keeps a typed letter from choosing an item by its shortcut letter", async () => {
    const renamed = vi.fn();
    const trigger = mountFilter([item("Rename", renamed, { shortcut: "R" }), item("Delete"), item("Duplicate")]);
    await openBy(trigger, "pointer");
    await typeInFilter("r");
    expect(renamed).not.toHaveBeenCalled();
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(filterField().value).toBe("r");
    // Only Rename has an "r".
    expect(labels()).toEqual(["Rename"]);
  });

  it.each([
    ["an unfiltered menu", undefined, true, "R"],
    ["a filtered menu", FILTER, false, null],
  ])("draws and announces an item's shortcut letter only where a key runs it: %s", async (_, filter, drawn, announced) => {
    await openBy(mount({ items: [item("Rename", () => {}, { shortcut: "R" })], filter }).trigger, "pointer");
    const rename = document.querySelector("[role=menuitem]")!;
    expect(rename.querySelector("kbd") !== null).toBe(drawn);
    expect(rename.getAttribute("aria-keyshortcuts")).toBe(announced);
  });

  it.each([
    ["an unfiltered menu", undefined, true, "Shift+Control+F"],
    ["a filtered menu", FILTER, false, null],
  ])("runs a command shortcut an item advertises, and announces it, only where the menu's own item holds focus: %s", async (_, filter, runs, announced) => {
    const run = vi.fn();
    const commandShortcut = { label: "Shift+Control+F", matches: () => true };
    await openBy(mount({ items: [item("Focus mode", run, { commandShortcut })], filter }).trigger, "pointer");
    // A filtered menu gives the focus straight back to its field, which is outside the menu.
    act(() => document.querySelector<HTMLElement>("[role=menuitem]")!.focus());
    expect(document.querySelector("[role=menuitem]")?.getAttribute("aria-keyshortcuts")).toBe(announced);
    let taken = false;
    act(() => void (taken = runMenuCommand(new KeyboardEvent("keydown", { key: "f", bubbles: true, cancelable: true }))));
    expect(taken).toBe(runs);
    expect(run).toHaveBeenCalledTimes(runs ? 1 : 0);
  });

  it.each([
    ["a filtered menu", FILTER, true],
    ["an unfiltered menu", undefined, false],
  ])("keeps the top of the popover clear of the sticky filter row only in a menu with a filter: %s", async (_, filter, padded) => {
    await openBy(mount({ items: three(), filter }).trigger, "pointer");
    const popover = document.querySelector(partSelector.menuPopover)!;
    // jsdom lays nothing out, so the padding's own class stands for it.
    expect(popover.classList.contains("scroll-pt-13")).toBe(padded);
  });

  it("keeps typed letters from moving focus out of the field by typeahead", async () => {
    await openBy(mountFilter([item("Rename"), item("Delete"), item("Duplicate")]), "pointer");
    await typeInFilter("d");
    expect(labels()).toEqual(["Delete", "Duplicate"]);
    await typeInFilter("u");
    expect(filterField().value).toBe("du");
    expect(labels()).toEqual(["Duplicate"]);
    expect(document.activeElement).toBe(filterField());
  });

  it("clears the typed text on the first Escape and closes the menu on the second, giving focus back to a trigger the keyboard opened it from", async () => {
    const trigger = mountFilter(three());
    await openBy(trigger, "keyboard");
    await typeInFilter("doc");
    expect(labels()).toEqual(["Docs"]);
    await pressInFilter("Escape");
    expect(filterField().value).toBe("");
    expect(labels()).toEqual(["Website", "Search API", "Docs"]);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    await pressInFilter("Escape");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(trigger);
  });

  it("starts the next opening empty after an item was chosen from the narrowed list", async () => {
    const trigger = mountFilter(three());
    await openBy(trigger, "pointer");
    await typeInFilter("web");
    await pressInFilter("Enter");
    await openBy(trigger, "pointer");
    expect(filterField().value).toBe("");
    expect(labels()).toEqual(["Website", "Search API", "Docs"]);
  });

  it("filters the entries a function gives as the menu opens", async () => {
    await openBy(mountFilter(() => three()), "pointer");
    await typeInFilter("web");
    expect(labels()).toEqual(["Website"]);
  });

  describe("the one highlighted item", () => {
    const mouse = { bubbles: true, cancelable: true, pointerId: 1, isPrimary: true, pointerType: "mouse" };
    const rowOf = (label: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((el) => el.textContent === label)!;
    const hover = async (row: HTMLElement) => {
      await act(async () => {
        row.dispatchEvent(new PointerEvent("pointerover", mouse));
        row.dispatchEvent(new PointerEvent("pointermove", mouse));
      });
      runOutTimers();
    };
    const leave = async (row: HTMLElement) => {
      await act(async () => {
        row.dispatchEvent(new PointerEvent("pointermove", { ...mouse, relatedTarget: null }));
        row.dispatchEvent(new PointerEvent("pointerout", { ...mouse, relatedTarget: document.body }));
        row.dispatchEvent(new PointerEvent("pointerleave", { ...mouse, bubbles: false, relatedTarget: document.body }));
      });
      runOutTimers();
    };

    it("keeps exactly one item marked, and it is the one Enter chooses, after the pointer hovered a row and left the menu", async () => {
      const picked = vi.fn();
      await openBy(mountFilter([...three(picked), item("Teams", () => picked("Teams"))]), "pointer");
      await typeInFilter("e");
      await hover(rowOf("Teams"));
      expect(highlighted()).toEqual(["Teams"]);
      await leave(rowOf("Teams"));
      const marked = highlighted();
      expect(marked).toHaveLength(1);
      await pressInFilter("Enter");
      expect(picked).toHaveBeenCalledExactlyOnceWith(marked[0]);
    });

    it("marks only the arrowed item once an arrow key moves off the hovered row, and Enter chooses it", async () => {
      const picked = vi.fn();
      await openBy(mountFilter([...three(picked), item("Teams", () => picked("Teams"))]), "pointer");
      await typeInFilter("e");
      await hover(rowOf("Teams"));
      expect(highlighted()).toEqual(["Teams"]);
      await pressInFilter("ArrowUp");
      expect(highlighted()).toEqual(["Search API"]);
      await pressInFilter("Enter");
      expect(picked).toHaveBeenCalledExactlyOnceWith("Search API");
    });
  });

  describe("a submenu", () => {
    const withSubmenu = (picked: (label: string) => void) => [
      item("Rename", () => picked("Rename"), { shortcut: "R" }),
      { label: "Switch", icon: Trash2, items: [item("Choice", () => picked("Choice"), { shortcut: "C" }), item("Other", () => picked("Other"))] },
    ];
    /** Opens the submenu of "Switch" with the arrow key from the filter field, which has the item highlighted. */
    const openSwitch = async () => {
      await typeInFilter("sw");
      await pressInFilter("ArrowRight");
      runOutTimers();
    };

    it("is not narrowed or styled by its parent's filter: its items keep their key cap and announced shortcut", async () => {
      await openBy(mountFilter(withSubmenu(() => {})), "pointer");
      await openSwitch();
      expect(document.querySelectorAll("[role=menu]")).toHaveLength(2);
      // The parent's text "sw" matches neither of the submenu's labels, which are all there.
      expect([...document.querySelectorAll<HTMLElement>("[role=menu]")[1]!.querySelectorAll("[role=menuitem]")].map((el) => ["Choice", "Other"].find((label) => el.textContent?.startsWith(label)))).toEqual(["Choice", "Other"]);
      const choice = [...document.querySelectorAll("[role=menuitem]")].find((el) => el.textContent?.startsWith("Choice"))!;
      expect(choice.querySelector("kbd")).not.toBeNull();
      expect(choice.getAttribute("aria-keyshortcuts")).toBe("C");
    });

    it("runs the shortcut letters of its own items", async () => {
      const picked = vi.fn();
      await openBy(mountFilter(withSubmenu(picked)), "pointer");
      await openSwitch();
      const submenu = document.querySelectorAll<HTMLElement>("[role=menu]")[1]!;
      expect(submenu.contains(document.activeElement)).toBe(true);
      await act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "c", bubbles: true, cancelable: true })));
      expect(picked).toHaveBeenCalledExactlyOnceWith("Choice");
    });
  });

  describe("each opening", () => {
    it("starts a reopening during the popover's exit animation with an empty field, in the mode of this opening: editable after a tap's read-only one", async () => {
      const trigger = mountFilter(three());
      await openBy(trigger, "touch");
      expect(filterField().readOnly).toBe(true);
      holdExitAnimations();
      await pressInFilter("Escape");
      expect(trigger.getAttribute("aria-expanded")).toBe("false");
      expect(document.querySelector("[role=menu]")).not.toBeNull();
      await openBy(trigger, "pointer");
      expect(trigger.getAttribute("aria-expanded")).toBe("true");
      expect(document.querySelectorAll("input")).toHaveLength(1);
      expect(filterField().value).toBe("");
      expect(filterField().readOnly).toBe(false);
      expect(document.activeElement).toBe(filterField());
    });

    it("starts a reopening during the exit animation read-only after a tap, though the opening before it was typed into", async () => {
      const picked = vi.fn();
      const trigger = mountFilter(three(picked));
      await openBy(trigger, "pointer");
      await typeInFilter("web");
      expect(filterField().readOnly).toBe(false);
      holdExitAnimations();
      await pressInFilter("Enter");
      expect(picked).toHaveBeenCalledExactlyOnceWith("Website");
      expect(trigger.getAttribute("aria-expanded")).toBe("false");
      expect(document.querySelector("[role=menu]")).not.toBeNull();
      await openBy(trigger, "touch");
      expect(trigger.getAttribute("aria-expanded")).toBe("true");
      expect(document.querySelectorAll("input")).toHaveLength(1);
      expect(filterField().value).toBe("");
      expect(filterField().readOnly).toBe(true);
      expect(document.activeElement).toBe(filterField());
      expect(labels()).toEqual(["Website", "Search API", "Docs"]);
    });
  });

  describe("the read-only field after a tap", () => {
    it.each([
      ["a key named Process", { key: "Process" }],
      ["a key with the legacy code 229", { key: "Unidentified", keyCode: 229 }],
      ["a key pressed while composing", { key: "Unidentified", isComposing: true }],
    ])("takes text once an input method starts: %s", async (_, init) => {
      await openBy(mountFilter(three()), "touch");
      expect(filterField().readOnly).toBe(true);
      await act(async () => void filterField().dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init })));
      expect(filterField().readOnly).toBe(false);
      expect(document.activeElement).toBe(filterField());
    });

    it("stays read-only through the keys that type nothing, an input method's keys aside", async () => {
      await openBy(mountFilter(three()), "touch");
      for (const key of ["ArrowDown", "ArrowUp", "Shift", "Tab"]) {
        await act(async () => void filterField().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true })));
      }
      expect(filterField().readOnly).toBe(true);
    });
  });

  describe.each(["Backspace", "Delete"])("%s in the field", (key) => {
    it("has its native action cancelled in the read-only field a tap opened, which has nothing to delete, and does not go on past it either", async () => {
      await openBy(mountFilter(three()), "touch");
      expect(filterField().readOnly).toBe(true);
      const seen: string[] = [];
      const record = (event: Event) => void seen.push((event as KeyboardEvent).key);
      document.addEventListener("keydown", record);
      onTestFinished(() => document.removeEventListener("keydown", record));
      const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      await act(async () => void filterField().dispatchEvent(event));
      expect(event.defaultPrevented).toBe(true);
      expect(seen).toEqual([]);
      expect(filterField().readOnly).toBe(true);
    });

    it("does not go on past the field's row, so no highlighted item or listener above it is handed the key, and its native action is left alone", async () => {
      await openBy(mountFilter(three()), "pointer");
      await typeInFilter("e");
      const seen: string[] = [];
      const record = (event: Event) => void seen.push((event as KeyboardEvent).key);
      const row = document.querySelector<HTMLElement>("[role=menuitem][data-focused]")!;
      document.addEventListener("keydown", record);
      row.addEventListener("keydown", record);
      onTestFinished(() => {
        document.removeEventListener("keydown", record);
        row.removeEventListener("keydown", record);
      });
      const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      await act(async () => void filterField().dispatchEvent(event));
      expect(seen).toEqual([]);
      expect(event.defaultPrevented).toBe(false);
      // A key that types does go on, so the recorder would have heard these.
      await act(async () => void filterField().dispatchEvent(new KeyboardEvent("keydown", { key: "x", bubbles: true, cancelable: true })));
      expect(seen).toContain("x");
    });
  });

  describe("no match, judged on the resolved entries' labels", () => {
    const resolving = (placeholder: string) => {
      let resolve!: (loaded: ActionMenuItem) => void;
      const load = { key: "k", run: () => new Promise<ActionMenuItem>((done) => { resolve = done; }) };
      return { entries: [item(placeholder, () => {}, { load }), item("Docs")], resolve: (label: string) => act(async () => resolve(item(label))) };
    };

    it("follows the label a load gives: a text that matched only the placeholder stops matching, and the message is spoken", async () => {
      const { entries, resolve } = resolving("Placeholder");
      await openBy(mountFilter(entries), "pointer");
      await typeInFilter("placeholder");
      expect(labels()).toEqual(["Placeholder"]);
      expect(spoken()).not.toContain(EMPTY);
      await resolve("Loaded name");
      expect(labels()).toEqual([]);
      expect(spoken()).toContain(EMPTY);
      expect(document.querySelector("[role=menuitem]")).toBeNull();
    });

    it("follows the label a load gives: a text that matched only the loaded label ends the message once it arrives", async () => {
      const { entries, resolve } = resolving("Placeholder");
      await openBy(mountFilter(entries), "pointer");
      await typeInFilter("loaded");
      expect(spoken()).toContain(EMPTY);
      await resolve("Loaded name");
      expect(labels()).toEqual(["Loaded name"]);
      expect(spoken()).not.toContain(EMPTY);
    });

    it("keeps the status region and the drawn line out of the menu's list", async () => {
      await openBy(mountFilter(three()), "pointer");
      const menu = document.querySelector("[role=menu]")!;
      const region = document.querySelector("[role=status]")!;
      expect(menu.contains(region)).toBe(false);
      await typeInFilter("zzz");
      const line = [...document.querySelectorAll("[aria-hidden=true]")].find((el) => el.textContent === EMPTY)!;
      expect(line).toBeDefined();
      expect(menu.contains(line)).toBe(false);
      expect(menu.contains(region)).toBe(false);
      expect(menu.querySelector("[role=menuitem]")).toBeNull();
    });
  });
});

describe("onOpened", () => {
  const entries: ActionMenuEntry[] = [{ label: "Rename", icon: Trash2, onClick: () => {} }, { label: "Delete", icon: Trash2, onClick: () => {} }];

  async function close(trigger: HTMLButtonElement): Promise<void> {
    await act(async () => {
      document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      document.activeElement!.dispatchEvent(new KeyboardEvent("keyup", { key: "Escape", bubbles: true, cancelable: true }));
    });
    runOutTimers();
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  }

  it("is not called before the menu opens", () => {
    const onOpened = vi.fn();
    mount({ items: entries, onOpened });
    expect(onOpened).not.toHaveBeenCalled();
  });

  it.each(["pointer", "keyboard"] as const)("is called once as the menu opens by %s", async (by) => {
    const onOpened = vi.fn();
    const { trigger } = mount({ items: entries, onOpened });
    await openBy(trigger, by);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(onOpened).toHaveBeenCalledOnce();
  });

  it("is not called again as the menu closes, and is called once for each further opening", async () => {
    const onOpened = vi.fn();
    const { trigger } = mount({ items: entries, onOpened });
    await openBy(trigger, "keyboard");
    await close(trigger);
    expect(onOpened).toHaveBeenCalledOnce();
    await openBy(trigger, "pointer");
    expect(onOpened).toHaveBeenCalledTimes(2);
    await close(trigger);
    expect(onOpened).toHaveBeenCalledTimes(2);
  });

  it("is the latest one given when a right-click on the context target opens the menu", async () => {
    const stale = vi.fn();
    const latest = vi.fn();
    const target = document.body.appendChild(document.createElement("div"));
    onTestFinished(() => target.remove());
    const { rerender } = mount({ items: entries, onOpened: stale, contextTargetRef: { current: target } });
    rerender({ onOpened: latest });
    await act(async () => {
      target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 }));
    });
    runOutTimers();
    expect(document.querySelector("[role=menu]")).not.toBeNull();
    expect(latest).toHaveBeenCalledOnce();
    expect(stale).not.toHaveBeenCalled();
  });
});
