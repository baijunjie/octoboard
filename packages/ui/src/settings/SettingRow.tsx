import React from "react";

/** One setting: its name and a line saying what it does at the start, its control at the end,
 * with a thin divider under every row but the last. At phone widths the control moves under the
 * text instead, since beside it a wide control (a select) squeezes the description to a few
 * words per line. */
export function SettingRow({
  label,
  description,
  children,
}: {
  label: React.ReactNode;
  description: React.ReactNode;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="flex flex-col items-start gap-3 border-b border-separator py-4 last:border-b-0 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        <div className="text-sm text-muted">{description}</div>
      </div>
      {children && <div className="shrink-0">{children}</div>}
    </div>
  );
}
