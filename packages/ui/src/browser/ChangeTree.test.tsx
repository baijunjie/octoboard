// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import type { ChangeEntry } from "../protocol";
import { ChangeTree } from "./ChangeTree";
import { changeItems, directoryKey } from "./changes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver, which the rows' names (FadeOverflow) need.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

afterEach(() => document.body.replaceChildren());

const change = (group: "staged" | "unstaged", path: string): ChangeEntry => ({
  group,
  old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" } },
  new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "v" } },
});
const items = changeItems([change("staged", "src/utils/kickback/a.ts"), change("staged", "src/b.ts"), change("unstaged", "src/b.ts"), change("unstaged", "top.ts")]);

function render(collapsed: ReadonlySet<string>, handlers: { onOpen?: () => void; onCollapsedChange?: (next: ReadonlySet<string>) => void } = {}) {
  const container = document.body.appendChild(document.createElement("div"));
  act(() =>
    createRoot(container).render(
      <ChangeTree
        items={items}
        selected={items[1].key}
        label="Changes"
        onOpen={handlers.onOpen ?? (() => {})}
        listRef={() => {}}
        collapsed={collapsed}
        onCollapsedChange={handlers.onCollapsedChange ?? (() => {})}
      />,
    ),
  );
  return container;
}
const labels = (container: HTMLElement) => [...container.querySelectorAll("[role=row]")].map((row) => row.getAttribute("aria-label") ?? row.textContent);

const row = (container: HTMLElement, key: string) => container.querySelector<HTMLElement>(`[role=row][data-key="${CSS.escape(key)}"]`)!;
const press = (element: HTMLElement, key: string) =>
  act(() => {
    element.focus();
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });

it("shows each section with its count and its changes under their directories, a chain of single-child directories as one row", () => {
  const container = render(new Set());
  expect(labels(container)).toEqual(["Staged2", "src", "utils/kickback", "a.ts, modified", "b.ts, modified, selected", "Unstaged2", "src", "b.ts, modified", "top.ts, modified"]);
  expect(row(container, directoryKey("staged", "src/utils/kickback")).getAttribute("aria-expanded")).toBe("true");
});

it("drops the rows under a collapsed directory and says so on the directory's row", () => {
  const container = render(new Set([directoryKey("staged", "src/utils/kickback"), directoryKey("unstaged", "src")]));
  expect(labels(container)).toEqual(["Staged2", "src", "utils/kickback", "b.ts, modified, selected", "Unstaged2", "src", "top.ts, modified"]);
  expect(row(container, directoryKey("staged", "src/utils/kickback")).getAttribute("aria-expanded")).toBe("false");
  expect(row(container, directoryKey("unstaged", "src")).getAttribute("aria-expanded")).toBe("false");
});

it("folds a directory on Left, unfolds one on Right and opens a change on Enter, keeping the other view's folded directories", () => {
  const onCollapsedChange = vi.fn();
  const onOpen = vi.fn();
  const other = directoryKey("committed", "docs");
  const container = render(new Set([other, directoryKey("unstaged", "src")]), { onCollapsedChange, onOpen });
  press(row(container, directoryKey("staged", "src/utils/kickback")), "ArrowLeft");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([other, directoryKey("unstaged", "src"), directoryKey("staged", "src/utils/kickback")]));
  press(row(container, directoryKey("unstaged", "src")), "ArrowRight");
  expect(onCollapsedChange).toHaveBeenLastCalledWith(new Set([other]));
  press(row(container, items[0].key), "Enter");
  expect(onOpen).toHaveBeenCalledWith(items[0]);
});
