import React from "react";

/** The rows of a `LabelledGroup` whose controls each carry a name of their own. Every name sits over its
 * control at any width and the rows sit side by side in equal columns from the `md` breakpoint up, one under
 * another below it: a control then has its column's full width, for values too long to share a row with a
 * name. */
export function GroupRows({ children }: { children: React.ReactNode }): React.ReactElement {
  return <div className="grid grid-cols-[minmax(0,1fr)] gap-3 md:grid-flow-col md:gap-x-6 md:grid-cols-none md:auto-cols-fr">{children}</div>;
}

/** One row of `GroupRows`. `label` is only the visible name: the control is named for assistive
 * technology by its own label, so this is hidden from it rather than read twice. */
export function GroupRow({ label, children }: { label: React.ReactNode; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <div aria-hidden="true" className="flex min-w-0 items-center gap-2 text-sm">{label}</div>
      {children}
    </div>
  );
}
