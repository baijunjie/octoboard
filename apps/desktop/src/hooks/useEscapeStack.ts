import { useEffect, useRef } from "react";

type EscapeHandler = () => void;

/**
 * A module-level stack of open "Escape-dismissable" layers (modals, dropdowns, action menus),
 * topmost last. A single document-level listener, installed lazily on first use, calls only the
 * top entry — so pressing Escape closes exactly one layer, the one actually on top, no matter how
 * many are open at once (a `Dropdown` inside a `Modal`, a `Modal` opened over another `Modal`).
 * Without a shared stack, each layer's own independent Escape listener would all fire at once.
 */
const stack: EscapeHandler[] = [];

let listenerInstalled = false;
function ensureListenerInstalled(): void {
  if (listenerInstalled) return;
  listenerInstalled = true;
  document.addEventListener("keydown", (ev) => {
    if (ev.key !== "Escape") return;
    stack[stack.length - 1]?.();
  });
}

/** Registers `onEscape` as a stack entry while `active` is true, topmost for as long as nothing
 * else registers after it. */
export function useEscapeStack(active: boolean, onEscape: EscapeHandler): void {
  // The entry is registered once per layer and reads the current handler through a ref. Registering
  // the handler itself would re-register on every render that passes a fresh arrow function, and
  // re-registration reorders the stack: React commits children before parents, so an inner layer
  // would end up *below* the one containing it and Escape would close the wrong one.
  const handler = useRef(onEscape);
  handler.current = onEscape;

  useEffect(() => {
    if (!active) return;
    ensureListenerInstalled();
    const entry: EscapeHandler = () => handler.current();
    stack.push(entry);
    return () => {
      const index = stack.lastIndexOf(entry);
      if (index !== -1) stack.splice(index, 1);
    };
  }, [active]);
}
