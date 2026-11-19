// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { ListBox, ListBoxItem, ListBoxItemLabel } from "./ListBox";
import { mount, pressWithMouse } from "./testSupport";

it.each([undefined, 36])("preserves item actions and disabled state with row height %s", (rowHeight) => {
  const action = vi.fn();
  const { container } = mount(
    <ListBox label="Branches" rowHeight={rowHeight} rowGap={4} rowPadding={6} onAction={action}>
      <ListBoxItem id="main" textValue="main"><ListBoxItemLabel>main</ListBoxItemLabel></ListBoxItem>
      <ListBoxItem id="busy" textValue="Busy" disabled>Busy</ListBoxItem>
    </ListBox>,
  );
  expect(container.querySelector('[role="listbox"]')?.getAttribute("aria-label")).toBe("Branches");
  const choices = [...container.querySelectorAll<HTMLElement>('[role="option"]')];
  expect(choices.map((item) => item.textContent)).toEqual(["main", "Busy"]);
  expect(choices[1]!.getAttribute("aria-disabled")).toBe("true");
  pressWithMouse(choices[1]!);
  expect(action).not.toHaveBeenCalled();
  pressWithMouse(choices[0]!);
  expect(action).toHaveBeenCalledExactlyOnceWith("main");
});

it("renders only a window of a large list while retaining its item count and positions", () => {
  onTestFinished(() => { vi.restoreAllMocks(); });
  for (const property of ["clientWidth", "clientHeight", "offsetWidth", "offsetHeight"] as const) {
    vi.spyOn(HTMLElement.prototype, property, "get").mockReturnValue(300);
  }
  const items = Array.from({ length: 1000 }, (_, index) => ({ id: String(index), label: `Branch ${index}` }));
  const { container } = mount(
    <ListBox label="Branches" rowHeight={36} rowGap={4} rowPadding={6} items={items}>
      {(item) => <ListBoxItem id={item.id} textValue={item.label}>{item.label}</ListBoxItem>}
    </ListBox>,
  );
  const choices = [...container.querySelectorAll<HTMLElement>('[role="option"]')];
  expect(choices.length).toBeGreaterThan(0);
  expect(choices.length).toBeLessThan(items.length);
  expect(choices[0]!.textContent).toBe("Branch 0");
  expect(choices[0]!.getAttribute("aria-posinset")).toBe("1");
  expect(choices[0]!.getAttribute("aria-setsize")).toBe("1000");
});

function selectable(selectedId: string | null | undefined, onSelectedChange: (id: string) => void, onAction?: (id: string) => void) {
  return (
    <ListBox label="Results" selectedId={selectedId} onSelectedChange={onSelectedChange} onAction={onAction}>
      <ListBoxItem id="a" textValue="a">a</ListBoxItem>
      <ListBoxItem id="b" textValue="b">b</ListBoxItem>
      <ListBoxItem id="c" textValue="c">c</ListBoxItem>
    </ListBox>
  );
}
const options = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>('[role="option"]')];
const selectedOf = (container: HTMLElement) => options(container).filter((item) => item.getAttribute("aria-selected") === "true").map((item) => item.textContent);

it("selects the item selectedId names, one at a time, and follows it as it changes", () => {
  const { container, render } = mount(selectable("b", vi.fn()));
  expect(selectedOf(container)).toEqual(["b"]);
  render(selectable("c", vi.fn()));
  expect(selectedOf(container)).toEqual(["c"]);
  render(selectable(null, vi.fn()));
  expect(selectedOf(container)).toEqual([]);
});

it("leaves its items unselectable without onSelectedChange", () => {
  const { container } = mount(
    <ListBox label="Plain"><ListBoxItem id="a" textValue="a">a</ListBoxItem></ListBox>,
  );
  expect(options(container)[0]!.hasAttribute("aria-selected")).toBe(false);
});

it("reports the item a click chooses without activating it, and never another one for a press on the selected item", () => {
  const changed = vi.fn();
  const action = vi.fn();
  const { container } = mount(selectable("a", changed, action));
  pressWithMouse(options(container)[2]!);
  expect(changed).toHaveBeenCalledExactlyOnceWith("c");
  expect(action).not.toHaveBeenCalled();
  changed.mockClear();
  // A selection is never emptied or moved by a press on the item already selected.
  pressWithMouse(options(container)[0]!);
  expect(changed).not.toHaveBeenCalled();
});

it("moves the selection with the arrow keys and activates the selected item on Enter", () => {
  const changed = vi.fn();
  const action = vi.fn();
  const { container } = mount(selectable("a", changed, action));
  const key = (target: Element, name: string) =>
    act(() => {
      target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
      target.dispatchEvent(new KeyboardEvent("keyup", { key: name, bubbles: true, cancelable: true }));
    });
  act(() => options(container)[0]!.focus());
  key(document.activeElement!, "ArrowDown");
  expect(changed).toHaveBeenLastCalledWith("b");
  expect(action).not.toHaveBeenCalled();
  key(document.activeElement!, "Enter");
  expect(action).toHaveBeenCalledTimes(1);
});
