import React, { createContext, useContext } from "react";

const ColumnsContext = createContext(false);

/** The rows of a `LabelledGroup` whose controls each carry a name of their own. The names share
 * one column wide enough for the longest, so the controls start in line; below the `sm` breakpoint,
 * each name sits over its control. With `columns` every name sits over its control at any width and
 * the rows sit side by side in equal columns from the `md` breakpoint up, one under another below
 * it: a control then has its column's full width, for values too long to share a row with a name. */
export function GroupRows({ columns, children }: { columns?: boolean; children: React.ReactNode }): React.ReactElement {
  return (
    <ColumnsContext.Provider value={!!columns}>
      <div className={columns ? "grid grid-cols-[minmax(0,1fr)] gap-3 md:grid-flow-col md:gap-x-6 md:grid-cols-none md:auto-cols-fr" : "grid gap-y-3 sm:grid-cols-[max-content_minmax(0,1fr)]"}>{children}</div>
    </ColumnsContext.Provider>
  );
}

/** One row of `GroupRows`. `label` is only the visible name: the control is named for assistive
 * technology by its own label, so this is hidden from it rather than read twice. */
export function GroupRow({ label, children }: { label: React.ReactNode; children: React.ReactNode }): React.ReactElement {
  const columns = useContext(ColumnsContext);
  return (
    <div className={`flex min-w-0 flex-col gap-1.5${columns ? "" : " sm:col-span-full sm:grid sm:grid-cols-subgrid sm:items-center sm:gap-x-3"}`}>
      <div aria-hidden="true" className="flex min-w-0 items-center gap-2 text-sm">{label}</div>
      {children}
    </div>
  );
}
