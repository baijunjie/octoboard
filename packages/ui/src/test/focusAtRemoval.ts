import { onTestFinished, vi } from "vitest";

/** Watches `element` leave the document for the rest of the test and reports which element held focus at that moment.
 * Removing a focused element fires no blur in WebKit and jsdom, and focus falls to `<body>` only afterwards, so what a
 * handoff sees is decided by where focus was when the removal began: a fold that moves focus first has it on a row that
 * stays by then, one that moves it afterwards still has it on the element being removed. Returns a getter that gives
 * `undefined` until the removal has happened. */
export function focusAtRemoval(element: Element): () => Element | null | undefined {
  let seen: Element | null | undefined;
  const original = Node.prototype.removeChild;
  const spy = vi.spyOn(Node.prototype, "removeChild").mockImplementation(function (this: Node, child: Node) {
    if (seen === undefined && child.contains(element)) seen = document.activeElement;
    return original.call(this, child) as never;
  });
  onTestFinished(() => spy.mockRestore());
  return () => seen;
}
