// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../../input/focusGuard";
import { expect, it, onTestFinished, vi } from "vitest";

import { beginAccountDrag } from "./accountDrag";

/** Two account rows 50 pixels tall, the first one pressed by its grip at `y` 25 and dragged down past the second. */
function dragFirstRow() {
  const table = document.body.appendChild(document.createElement("table"));
  const body = table.appendChild(document.createElement("tbody"));
  const rows = [0, 1].map((index) => {
    const row = body.appendChild(document.createElement("tr"));
    row.dataset.accountRow = String(index);
    row.getBoundingClientRect = () => ({ top: index * 50, bottom: (index + 1) * 50, height: 50, left: 0, right: 600, width: 600, x: 0, y: index * 50, toJSON() {} });
    return row;
  });
  onTestFinished(() => table.remove());
  const onDrop = vi.fn();
  const pointer = (type: string, clientY: number) => new PointerEvent(type, { bubbles: true, pointerType: "mouse", pointerId: 4, button: 0, clientY });
  beginAccountDrag(pointer("pointerdown", 25), rows[0]!, onDrop);
  const move = (clientY: number) => document.dispatchEvent(pointer("pointermove", clientY));
  const release = () => document.dispatchEvent(pointer("pointerup", 80));
  return { rows, onDrop, move, release };
}

it("cancels a drag when the window loses focus", () => {
  const { rows, onDrop, move, release } = dragFirstRow();
  move(80);
  expect(rows[0]!.style.transform).toBe("translateY(50px)");
  window.dispatchEvent(new Event("blur"));
  expect(rows[0]!.style.transform).toBe("");
  release();
  expect(onDrop).not.toHaveBeenCalled();
});
