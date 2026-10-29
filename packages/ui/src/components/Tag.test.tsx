// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { Tag } from "./Tag";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("lets a read-only tooltip tag wrap an unbroken label without adding collection semantics", () => {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const text = "分類".repeat(100);
  try {
    act(() => root.render(<Tag isReadOnly wrap tag={text} />));
    const tag = host.querySelector<HTMLElement>('[data-slot="tag"]')!;
    expect(tag.textContent).toBe(text);
    expect(tag.classList.contains("whitespace-normal")).toBe(true);
    expect(tag.classList.contains("[overflow-wrap:anywhere]")).toBe(true);
    expect(tag.classList.contains("max-w-full")).toBe(true);
    expect(tag.classList.contains("whitespace-nowrap")).toBe(false);
    expect(tag.getAttribute("role")).toBeNull();
    expect(tag.tabIndex).toBe(-1);
    expect(tag.querySelector("button")).toBeNull();
  } finally {
    act(() => root.unmount());
    host.remove();
  }
});
