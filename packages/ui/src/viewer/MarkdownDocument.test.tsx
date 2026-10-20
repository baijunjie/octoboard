// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import { MarkdownDocument } from "./MarkdownDocument";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: Root[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
});

async function render(text: string, embedded: boolean): Promise<HTMLElement> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  await act(async () => root.render(<MarkdownDocument text={text} path="" theme="light" renderCode={() => null} embedded={embedded} />));
  return container;
}

const levels = (container: HTMLElement) => [...container.querySelectorAll("h1, h2, h3, h4, h5, h6")].map((node) => node.tagName);

it("keeps a document's headings and gives them ids by default", async () => {
  const container = await render("# One\n\n## Two", false);
  expect(levels(container)).toEqual(["H1", "H2"]);
  expect([...container.querySelectorAll("h1, h2")].map((node) => node.id)).toEqual(["one", "two"]);
});

it("demotes an embedded document's headings two levels, clamped at h6, and gives them no ids", async () => {
  const container = await render("# a\n\n## b\n\n### c\n\n#### d\n\n##### e\n\n###### f", true);
  expect(levels(container)).toEqual(["H3", "H4", "H5", "H6", "H6", "H6"]);
  expect(container.querySelector("[id]")).toBeNull();
});

it("gives two embedded documents' footnotes ids that do not clash", async () => {
  const note = "Text[^1]\n\n[^1]: Note";
  const first = await render(note, true);
  const second = await render(note, true);
  const ids = (container: HTMLElement) => [...container.querySelectorAll("[id]")].map((node) => node.id);
  expect(ids(first).length).toBeGreaterThan(0);
  expect(ids(first).filter((id) => ids(second).includes(id))).toEqual([]);
});

it("points each embedded footnote reference at the footnotes heading of its own document", async () => {
  const note = "Text[^1]\n\n[^1]: Note";
  const first = await render(note, true);
  const second = await render(note, true);
  for (const container of [first, second]) {
    const described = container.querySelector("[aria-describedby]");
    expect(described).not.toBeNull();
    const target = document.getElementById(described!.getAttribute("aria-describedby")!);
    expect(target).not.toBeNull();
    expect(container.contains(target)).toBe(true);
  }
});
