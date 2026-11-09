import { onTestFinished, vi } from "vitest";

/** Emulates Chromium's order as a focused element is removed for the rest of the test: React has detached its ref,
 * then removing the element blurs it synchronously, with focus going to nothing. jsdom, like WebKit and Firefox,
 * fires no blur on removal. */
export function blurOnRemoval(): void {
  const original = Node.prototype.removeChild;
  const spy = vi.spyOn(Node.prototype, "removeChild").mockImplementation(function (this: Node, child: Node) {
    const active = document.activeElement;
    if (active instanceof HTMLElement && active !== document.body && child.contains(active)) active.blur();
    return original.call(this, child) as never;
  });
  onTestFinished(() => spy.mockRestore());
}
