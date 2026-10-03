import React, { useEffect, useRef, useState } from "react";

import { useDismissOnOutsideOrEscape } from "../hooks/useDismissOnOutsideOrEscape";

export interface ActionMenuItem {
  label: string;
  onClick: () => void;
  destructive?: boolean;
}

/**
 * A small "⋯" action menu, built the same way as `Dropdown` and for the same reason: no native
 * popup, so it stays reachable for scripted UI verification. Unlike `Dropdown` this does not track
 * a selected value — each item is a one-shot action.
 */
export function ActionMenu({ items }: { items: ActionMenuItem[] }): React.ReactElement {
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<{ top: number; right: number }>();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useDismissOnOutsideOrEscape(open, rootRef, () => setOpen(false));

  // The menu is `position: fixed` against a bounding rect captured once, at open — it does not
  // track the trigger if the row scrolls out from under it afterward, so it closes rather than
  // drifting away from the row it belongs to. Only a scroll that actually moves the trigger counts:
  // the listener must be on the capture phase to see the sidebar tree's own scroll at all, and that
  // also puts every unrelated scroll in the window through it — the terminal's viewport scrolls
  // while an agent produces output, which says nothing about where this menu's row is.
  useEffect(() => {
    if (!open) return;
    const onScroll = (event: Event) => {
      const target = event.target;
      const movesTheTrigger =
        target === document ||
        (target instanceof Node && rootRef.current !== null && target.contains(rootRef.current));
      if (movesTheTrigger) setOpen(false);
    };
    document.addEventListener("scroll", onScroll, true);
    return () => document.removeEventListener("scroll", onScroll, true);
  }, [open]);

  const toggle = () => {
    if (!open) {
      // Positioned against the trigger's own bounding rect rather than left to normal flow: a
      // menu near the bottom of the sidebar would otherwise be clipped by its `overflow-y: auto`
      // ancestor. Anchored by `right` (the distance from the viewport's right edge) rather than
      // `left`, so the menu's own (variable) width never has to be known up front.
      const rect = triggerRef.current?.getBoundingClientRect();
      if (rect) setMenuPosition({ top: rect.bottom + 4, right: window.innerWidth - rect.right });
    }
    setOpen((o) => !o);
  };

  return (
    <div className="action-menu" ref={rootRef} onClick={(e) => e.stopPropagation()}>
      <button
        type="button"
        ref={triggerRef}
        className="action-menu-trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        // Keeps the mousedown from moving focus off whatever had it (typically the terminal) —
        // the click that opens the menu still fires via the normal click event afterward.
        onMouseDown={(e) => e.preventDefault()}
        onClick={toggle}
      >
        ⋯
      </button>
      {open && menuPosition && (
        <ul
          className="action-menu-list"
          role="menu"
          style={{ position: "fixed", top: menuPosition.top, right: menuPosition.right }}
        >
          {items.map((item) => (
            <li key={item.label}>
              <button
                type="button"
                role="menuitem"
                className={`action-menu-item${item.destructive ? " action-menu-item-destructive" : ""}`}
                onClick={() => {
                  setOpen(false);
                  item.onClick();
                }}
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
