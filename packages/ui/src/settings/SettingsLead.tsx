import React from "react";

/** The sentence that opens a settings section, above its rows. */
export function SettingsLead({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="pb-2 text-sm text-app-muted">{children}</p>;
}

/** The rows under a `SettingsLead`. `SettingRow` rules between its rows but not above the first, and the lead above
 * would otherwise run straight into a description in the same muted text; this wrapper draws that rule. */
export function LeadRows({ children }: { children: React.ReactNode }): React.ReactElement {
  return <div className="groove-t">{children}</div>;
}
