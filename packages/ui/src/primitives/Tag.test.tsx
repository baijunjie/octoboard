// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { mountHost } from "../test/mountHost";
import { Tag, TagGroup, TagList, TagRemoveButton } from "./Tag";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function group({ description, onAction, onRemove }: { description?: string; onAction?: () => void; onRemove?: (tags: string[]) => void }) {
  return (
    <TagGroup label="Files" onRemove={onRemove}>
      <TagList>
        <Tag id="a" textValue="report.pdf" data-tag="a" onAction={onAction} description={description}>
          <span>report.pdf</span>
          <TagRemoveButton label="Remove" />
        </Tag>
      </TagList>
    </TagGroup>
  );
}

const tag = (container: HTMLElement) => container.querySelector<HTMLElement>('[data-tag="a"]')!;
const description = (element: HTMLElement) => (element.getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ");

it("describes a tag by its description from the first render, and stops describing it once the description goes", () => {
  const container = mountHost(group({ description: "Opens in the file viewer" }));
  expect(description(tag(container))).toContain("Opens in the file viewer");
  container.render(group({}));
  expect(description(tag(container))).not.toContain("Opens in the file viewer");
  expect(document.body.textContent).not.toContain("Opens in the file viewer");
});

it("runs the tag's own action on a press of the tag, and never on its remove button", () => {
  const onAction = vi.fn();
  const onRemove = vi.fn();
  const container = mountHost(group({ onAction, onRemove }));
  act(() => tag(container).click());
  expect(onAction).toHaveBeenCalledOnce();
  act(() => container.querySelector<HTMLElement>("button")!.click());
  expect(onAction).toHaveBeenCalledOnce();
  expect(onRemove).toHaveBeenCalledExactlyOnceWith(["a"]);
});

it("tells the group which tag is removed by Delete or Backspace on the focused tag", () => {
  for (const key of ["Delete", "Backspace"]) {
    const onRemove = vi.fn();
    const container = mountHost(group({ onRemove }));
    act(() => tag(container).focus());
    act(() => {
      tag(container).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
      tag(container).dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true }));
    });
    expect(onRemove, key).toHaveBeenCalledExactlyOnceWith(["a"]);
  }
});
