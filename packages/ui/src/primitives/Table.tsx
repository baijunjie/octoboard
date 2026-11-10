import { Table as HeroTable } from "@heroui/react";
import React, { useEffect, useRef } from "react";
import { useFocusManager } from "react-aria";

import { FOCUS_SCOPE_RESTORE_EVENT } from "./focus";

/**
 * HeroUI's table, labelled by the element `labelledBy` names, in its scroll container: a table narrower than its
 * `minWidth` (a CSS length) scrolls sideways rather than squeezing its cells. Its parts are `TableHeader` holding the
 * `TableColumn`s, then `TableBody` holding the `TableRow`s, each with one `TableCell` per column. Tab moves through the
 * controls inside the cells in document order.
 */
export function Table({ labelledBy, minWidth, children }: { labelledBy: string; minWidth?: string; children: React.ReactNode }): React.ReactElement {
  const focusManager = useFocusManager();
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // React Aria 3.52 prevents FocusScope restoration in collections to account for recycled rows.
    // These static rows keep their controls, so let a closing popover restore its original button.
    const element = root.current;
    const keepRestoreTarget = (event: Event) => event.stopPropagation();
    element?.addEventListener(FOCUS_SCOPE_RESTORE_EVENT, keepRestoreTarget, true);
    return () => element?.removeEventListener(FOCUS_SCOPE_RESTORE_EVENT, keepRestoreTarget, true);
  }, []);
  return (
    <HeroTable
      ref={root}
      onKeyDownCapture={(event) => {
        // React Aria's collection handles Tab by leaving the whole table, even in its tab mode.
        // Visit every cell's controls while keeping the enclosing dialog's focus containment.
        if (event.key !== "Tab" || event.ctrlKey || event.metaKey || !event.currentTarget.contains(event.target as Node)) return;
        event.stopPropagation();
        if (focusManager) {
          event.preventDefault();
          if (event.shiftKey) focusManager.focusPrevious({ tabbable: true, wrap: true });
          else focusManager.focusNext({ tabbable: true, wrap: true });
        }
      }}
    >
      <HeroTable.ScrollContainer>
        <HeroTable.Content
          aria-labelledby={labelledBy}
          style={minWidth ? { minWidth } : undefined}
          render={(props, { state }) => (
            <table
              // React Aria also types virtualized tables as divs. This wrapper renders static tables.
              {...props as React.ComponentPropsWithRef<"table">}
              onFocus={(event) => {
                // The collection's entry handler restores its last row, pulling reverse Tab off a cell's button.
                // Each row and cell still tracks its own focus, while entry keeps the control the user reached.
                if (event.currentTarget.contains(event.target)) state.selectionManager.setFocused(true);
                props.onFocus?.(event);
              }}
            />
          )}
        >
          {children}
        </HeroTable.Content>
      </HeroTable.ScrollContainer>
    </HeroTable>
  );
}

export function TableHeader({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroTable.Header>{children}</HeroTable.Header>;
}

/** A column's header. The table's rows are named by the cells of its `rowHeader` column. */
export function TableColumn({ rowHeader, children }: { rowHeader?: boolean; children: React.ReactNode }): React.ReactElement {
  return <HeroTable.Column isRowHeader={rowHeader}>{children}</HeroTable.Column>;
}

export function TableBody({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroTable.Body>{children}</HeroTable.Body>;
}

/** A row, `id` unique in its table; `textValue` is its plain-text name for type-ahead and the like. */
export function TableRow({ id, textValue, children }: { id: string; textValue: string; children: React.ReactNode }): React.ReactElement {
  return <HeroTable.Row id={id} textValue={textValue}>{children}</HeroTable.Row>;
}

/** A cell; `className` lays its content out within it. */
export function TableCell({ className, children }: { className?: string; children?: React.ReactNode }): React.ReactElement {
  return <HeroTable.Cell className={className}>{children}</HeroTable.Cell>;
}
