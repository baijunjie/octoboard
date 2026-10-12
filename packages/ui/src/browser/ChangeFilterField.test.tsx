// @vitest-environment jsdom
import React, { act, useState } from "react";
import { expect, it, vi } from "vitest";

import { ChangeFilterField } from "./ChangeFilterField";
import { ChangeList } from "./ChangeList";
import type { ChangeViewState } from "./changeView";
import { change, changeViewFixture, mountRows } from "./changeListTestSupport";
import { changeItems } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const items = changeItems([change("staged", "src/utils/alpha.ts"), change("unstaged", "docs/alpha.md"), change("unstaged", "src/beta.ts")]);
const field = (container: HTMLElement) => container.querySelector<HTMLInputElement>("input")!;

/** The field over a change list, with what the Git mode holds for both of them — whether the field
 * is open and the text in it — held in state here, so the returned functions stand in for the tab
 * row's toggle and for the user at the keyboard. */
function render(open = false) {
  const fixture = changeViewFixture();
  const toggle = fixture.filterToggleRef.current!;
  let setOpen: (open: boolean) => void = () => {};
  let isOpen = open;
  function Harness(): React.ReactElement {
    const [filterOpen, setFilterOpen] = useState(open);
    const [filter, setFilter] = useState("");
    setOpen = setFilterOpen;
    isOpen = filterOpen;
    const view: ChangeViewState = { ...fixture, filterOpen, onFilterOpenChange: setFilterOpen, filter, onFilterChange: setFilter };
    return (
      <>
        <ChangeFilterField changeView={view} />
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

it("clears the filter with the field's own Clear button", () => {
  const view = render(true);
  view.type("alpha");
  act(() => view.container.querySelector<HTMLElement>("button")!.click());
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
