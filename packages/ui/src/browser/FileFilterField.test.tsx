// @vitest-environment jsdom
import React, { act, useState } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { FileFilterField } from "./FileFilterField";
import { ChangeList } from "./ChangeList";
import type { ChangeViewState } from "./changeView";
import { change, changeViewFixture, mountRows } from "./browserTestSupport";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const items = changeItems([change("staged", "src/utils/alpha.ts"), change("unstaged", "docs/alpha.md"), change("unstaged", "src/beta.ts")]);
const field = (container: HTMLElement) => container.querySelector<HTMLInputElement>("input")!;

/** The field over a change list, with what the Git mode holds for both of them — whether the field
 * is open and the text in it — held in state here, so the returned functions stand in for the tab
 * row's toggle and for the user at the keyboard. */
function render(open = false, incompleteOf: () => boolean = () => false) {
  const fixture = changeViewFixture();
  const toggle = fixture.filterToggleRef.current!;
  let setOpen: (open: boolean) => void = () => {};
  let isOpen = open;
  let rerender: () => void = () => {};
  function Harness(): React.ReactElement {
    const [filterOpen, setFilterOpen] = useState(open);
    const [filter, setFilter] = useState("");
    const [, bump] = useState(0);
    rerender = () => bump((n) => n + 1);
    setOpen = setFilterOpen;
    isOpen = filterOpen;
    const view: ChangeViewState = { ...fixture, filterOpen, onFilterOpenChange: setFilterOpen, filter, onFilterChange: setFilter };
    return (
      <>
        <FileFilterField state={view} label="File name contains" placeholder="Filter changes" incomplete={incompleteOf()} />
        <ChangeList items={items} selected={undefined} label="Changes" onOpen={() => {}} listRef={() => {}} changeView={view} />
      </>
    );
  }
  const container = mountRows(<Harness />, { clipboard: { writeText: async () => {} } });
  return {
    container,
    toggle,
    isOpen: () => isOpen,
    rows: () => container.querySelectorAll("[role=row][data-key]").length,
    press: (next: boolean) => act(() => setOpen(next)),
    rerender: () => act(() => rerender()),
    type: (text: string) =>
      act(() => {
        const input = field(container);
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, text);
        input.dispatchEvent(new Event("input", { bubbles: true }));
      }),
    focusAway: (to: Element | null) =>
      act(() => {
        const input = field(container);
        if (to instanceof HTMLElement) to.focus();
        else input.blur();
        input.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: to }));
      }),
  };
}

it("has no field until the toggle opens it, and puts keyboard focus in it then", () => {
  const view = render();
  expect(view.container.querySelector("input")).toBeNull();
  expect(view.rows()).toBe(3);
  view.press(true);
  expect(document.activeElement).toBe(field(view.container));
  expect(field(view.container).getAttribute("aria-label")).toBe("File name contains");
});

it("leaves focus where it is when the view is mounted again with the field already open", () => {
  const view = render(true);
  expect(view.container.querySelector("input")).not.toBeNull();
  expect(document.activeElement).not.toBe(field(view.container));
});

it("takes an empty field away once focus leaves it, and keeps one with text typed in it", () => {
  const view = render(true);
  view.focusAway(document.body);
  expect(view.container.querySelector("input")).toBeNull();
  view.press(true);
  view.type("alpha");
  view.focusAway(document.body);
  expect(field(view.container).value).toBe("alpha");
  expect(view.rows()).toBe(2);
});

it("keeps an empty field through the window itself losing focus, which the user is coming back from", () => {
  const view = render(true);
  const hasFocus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
  try {
    view.focusAway(null);
    expect(view.container.querySelector("input")).not.toBeNull();
  } finally {
    hasFocus.mockRestore();
  }
});

it("hands keyboard focus to the toggle when the field goes from under it", () => {
  const view = render(true);
  act(() => field(view.container).focus());
  view.press(false);
  expect(document.activeElement).toBe(view.toggle);
});

it("clears the filter with the field's own Clear button, named from the catalog", () => {
  const view = render(true);
  view.type("alpha");
  const clear = view.container.querySelector<HTMLElement>("button")!;
  // HeroUI's close button carries an English literal of its own where it is not named here.
  expect(clear.getAttribute("aria-label")).toBe("Clear");
  act(() => clear.click());
  expect(field(view.container).value).toBe("");
  expect(view.rows()).toBe(3);
});

it("keeps the field out of the list, so its keys never reach the list's type-ahead or arrows", () => {
  const view = render(true);
  const input = field(view.container);
  expect(input.closest("[role=grid], [role=treegrid]")).toBeNull();
  act(() => input.focus());
  act(() => {
    for (const key of ["b", "ArrowDown"]) input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
  expect(view.container.querySelector("[data-current]")).toBeNull();
  expect(document.activeElement).toBe(input);
});

it("keeps an input method's composition from anything above the field", () => {
  const view = render(true);
  view.type("alpha");
  const above = vi.fn();
  view.container.addEventListener("keydown", above);
  const input = field(view.container);
  act(() => input.focus());
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, isComposing: true })));
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, keyCode: 229 })));
  expect(above).not.toHaveBeenCalled();
  expect(input.value).toBe("alpha");
  // Outside a composition Escape clears the field, and goes on up.
  act(() => void input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(above).toHaveBeenCalledOnce();
  expect(input.value).toBe("");
  // Emptied by Escape, the field is still there: focus has not left it.
  expect(view.isOpen()).toBe(true);
});

it("puts the incomplete-search mark inside the field before the Clear button, which stays visible and clears the filter", () => {
  const view = render(true, () => true);
  view.type("alpha");
  const group = field(view.container).parentElement!;
  const mark = group.querySelector<HTMLElement>("button[data-status-mark]")!;
  const clear = group.querySelector<HTMLElement>("button[aria-label='Clear']")!;
  expect(mark.getAttribute("aria-label")).toBe("The search is incomplete");
  // Mark, then Clear at the trailing end; both inside the field's group, after the input.
  expect(field(view.container).compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(mark.compareDocumentPosition(clear) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(group.lastElementChild).toBe(clear);
  // The field's clear behaviour is not the mark's: pressing the mark leaves the filter alone.
  act(() => mark.click());
  expect(field(view.container).value).toBe("alpha");
  act(() => clear.click());
  expect(field(view.container).value).toBe("");
});

it("hands focus to the search input when the mark that holds it goes because the search completed", () => {
  let incomplete = true;
  const view = render(true, () => incomplete);
  view.type("alpha");
  const mark = view.container.querySelector<HTMLElement>("button[data-status-mark]")!;
  act(() => mark.focus());
  expect(document.activeElement).toBe(mark);
  incomplete = false;
  view.rerender();
  expect(view.container.querySelector("button[data-status-mark]")).toBeNull();
  expect(document.activeElement).toBe(field(view.container));
  // Focus elsewhere is left alone.
  incomplete = true;
  view.rerender();
  act(() => view.toggle.focus());
  incomplete = false;
  view.rerender();
  expect(document.activeElement).toBe(view.toggle);
});

it("leaves focus on another mark in the window alone when its own mark goes", () => {
  let incomplete = true;
  const view = render(true, () => incomplete);
  view.type("alpha");
  const other = document.body.appendChild(document.createElement("button"));
  other.setAttribute("data-status-mark", "");
  onTestFinished(() => other.remove());
  act(() => other.focus());
  incomplete = false;
  view.rerender();
  expect(document.activeElement).toBe(other);
});
