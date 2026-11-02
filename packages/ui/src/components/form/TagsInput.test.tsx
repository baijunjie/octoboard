// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { TagsInput } from "./TagsInput";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mountTags(initial: string[], suggestions = initial) {
  const onChange = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  function Host() {
    const [value, setValue] = useState(initial);
    return <TagsInput label="Tags" value={value} suggestions={suggestions} onChange={(tags) => { onChange(tags); setValue(tags); }} />;
  }
  act(() => root.render(<Host />));
  const input = container.querySelector("input")!;
  const type = (text: string) => act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const enter = () => act(() => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  return { container, input, onChange, type, enter };
}

it("names a tag richly in its removal tooltip without repeating its accessible name", async () => {
  const tag = "مراجعة-中文/Alpha(2)";
  const { container } = mountTags([tag, "Other"]);
  const row = [...container.querySelectorAll('[role="row"]')].find((node) => node.textContent === tag)!;
  expect(row.getAttribute("data-list-motion")).toBe(`tag:${tag}`);
  expect(row.textContent).toBe(tag);
  expect(row.getAttribute("data-key")).toBe(tag);
  const remove = row.querySelector<HTMLButtonElement>("button")!;
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  });
  await act(async () => {});
  act(() => remove.focus());
  await act(async () => {});
  const tooltip = document.querySelector('[role="tooltip"]')!;
  expect(tooltip.textContent).toBe(`Remove tag ${tag}`);
  const chip = tooltip.querySelector("[data-inline-reference]")!;
  expect(chip.textContent).toBe(tag);
  expect(chip.getAttribute("dir")).toBe("auto");
  expect(chip.getAttribute("data-size")).toBe("xs");
  expect(chip.classList.contains("inline-reference--wrap")).toBe(true);
  expect(chip.querySelector("code")).toBeNull();
  expect(remove.getAttribute("aria-label")).toBe("Remove tag");
  expect(remove.getAttribute("aria-describedby")).toBeFalsy();
  const labelIds = remove.getAttribute("aria-labelledby")!.split(" ");
  expect(labelIds.map((id) => document.getElementById(id)?.textContent ?? document.getElementById(id)?.getAttribute("aria-label")).join(" ")).toContain(tag);
  act(() => remove.blur());
  expect(document.querySelector('[role="tooltip"]')).toBeNull();
});

it("removes by the raw mixed-script tag key and returns focus to the input", () => {
  const tag = "مراجعة-中文/Alpha(2)";
  const { container, input, onChange } = mountTags([tag, "Other"]);
  const row = [...container.querySelectorAll('[role="row"]')].find((node) => node.textContent === tag)!;
  expect(row.getAttribute("data-key")).toBe(tag);
  act(() => row.querySelector<HTMLButtonElement>("button")!.click());
  expect(onChange).toHaveBeenCalledExactlyOnceWith(["Other"]);
  expect(document.activeElement).toBe(input);
  expect(document.querySelector('[role="tooltip"]')).toBeNull();
});

it.each(["Delete", "Backspace"])("keeps the tags one tab stop, removing the focused tag on %s and handing focus to the input", (key) => {
  const { container, input, onChange } = mountTags(["Alpha", "Beta"]);
  const grid = container.querySelector<HTMLElement>('[role="grid"]')!;
  const [first] = [...grid.querySelectorAll<HTMLElement>('[role="row"]')];
  act(() => first.focus());
  expect([...grid.querySelectorAll<HTMLElement>("*")].filter((element) => element.tabIndex >= 0)).toEqual([first]);
  act(() => {
    first.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    first.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });
  expect(onChange).toHaveBeenCalledExactlyOnceWith(["Beta"]);
  expect(document.activeElement).toBe(input);
});

it("matches suggestions case-insensitively and submits their raw spelling without sentence isolates", () => {
  const raw = "مراجعة-中文/Alpha(2)";
  const { type, enter, input, onChange, container } = mountTags([], [raw]);
  type(`  ${raw.replace("Alpha", "alpha")}  `);
  enter();
  expect(onChange).toHaveBeenCalledExactlyOnceWith([raw]);
  expect(input.value).toBe("");
  expect(container.querySelector('[data-list-motion="tag:' + raw + '"]')?.textContent).toBe(raw);
  type(raw.toLocaleUpperCase());
  enter();
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(input.value).toBe("");
});

it("shows the suggestions tooltip on its own button's keyboard focus and leaves input focus undescribed", async () => {
  const { container, input } = mountTags(["Picked"], ["Picked", "مراجعة/中文"]);
  const trigger = container.querySelector<HTMLButtonElement>('[data-slot="combo-box-trigger"]')!;
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    trigger.focus();
  });
  await act(async () => {});
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Show suggestions");
  expect(trigger.getAttribute("aria-describedby")).toBeFalsy();
  act(() => input.focus());
  await act(async () => {});
  expect(document.querySelector('[role="tooltip"]')).toBeNull();
  expect(document.activeElement).toBe(input);
});

it.each(["mouse", "virtual"] as const)(
  "brings a partly hidden field into view on a %s press of the suggestions trigger and keeps the list open",
  (pointer) => {
    const { container, input } = mountTags(["review"], ["review", "frontend", "backend"]);
    const box = container.querySelector<HTMLElement>('[data-slot="combo-box-input-group"]')!;
    const trigger = container.querySelector<HTMLButtonElement>('[data-slot="combo-box-trigger"]')!;
    // jsdom lays nothing out and never scrolls, so the field stands partly hidden until it is scrolled into view, and a
    // focus a browser would scroll for is recorded. Either scroll lands once the list is open, as a browser's does.
    let boxScrolled = false;
    let focusScrolled = false;
    box.getBoundingClientRect = () => DOMRect.fromRect({ x: 0, y: boxScrolled ? 400 : 520, width: 300, height: 36 });
    box.scrollIntoView = vi.fn(() => {
      boxScrolled = true;
    });
    const nativeFocus = HTMLElement.prototype.focus;
    HTMLElement.prototype.focus = function (this: HTMLElement, options?: FocusOptions) {
      if (this === input && !options?.preventScroll && document.activeElement !== input) focusScrolled = true;
      nativeFocus.call(this, options);
    };
    onTestFinished(() => {
      HTMLElement.prototype.focus = nativeFocus;
    });

    act(() => {
      if (pointer === "mouse") {
        const init = { bubbles: true, cancelable: true, button: 0, pointerId: 1, pointerType: "mouse" };
        trigger.dispatchEvent(new PointerEvent("pointerdown", init));
        trigger.dispatchEvent(new PointerEvent("pointerup", init));
      }
      trigger.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, button: 0, detail: pointer === "mouse" ? 1 : 0 }));
    });
    expect(document.querySelector('[role="listbox"]')).not.toBeNull();
    expect(box.scrollIntoView).toHaveBeenCalledWith({ block: "nearest", inline: "nearest" });
    expect(focusScrolled).toBe(false);
    act(() => container.dispatchEvent(new Event("scroll")));

    expect(document.activeElement).toBe(input);
    expect(document.querySelector('[role="listbox"]')).not.toBeNull();
    // A screen reader's press starts on the first suggestion, as on a field in view.
    if (pointer === "virtual") expect(input.getAttribute("aria-activedescendant")).toMatch(/frontend$/);
  },
);
