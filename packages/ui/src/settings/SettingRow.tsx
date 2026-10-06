import React from "react";

/** One setting: its name and a line saying what it does at the start, its control at the end,
 * with a thin divider under every row but the last. */
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
    <div className="flex items-center justify-between gap-6 border-b border-separator py-4 last:border-b-0">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        <div className="text-sm text-muted">{description}</div>
      </div>
      {children && <div className="shrink-0">{children}</div>}
    </div>
  );
}
