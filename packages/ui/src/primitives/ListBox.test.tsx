// @vitest-environment jsdom
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
