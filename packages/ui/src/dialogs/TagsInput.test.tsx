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
  const chip = tooltip.querySelector("[data-inline-chip]")!;
  expect(chip.textContent).toBe(tag);
  expect(chip.getAttribute("dir")).toBe("auto");
  expect(chip.getAttribute("data-size")).toBe("inherit");
  expect(chip.classList.contains("inline-chip--wrap")).toBe(true);
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
