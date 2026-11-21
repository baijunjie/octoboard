import React from "react";

/** The sentence that opens a settings section, above its rows. */
export function SettingsLead({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="pb-2 text-sm text-app-muted">{children}</p>;
}

/** The rows under a `SettingsLead`. `SettingRow` rules between its rows but not above the first, and the lead above
 * would otherwise run straight into a description in the same muted text; this wrapper draws that rule. Its last row
 * draws no rule under itself either, so where more of the section follows (`ruledBelow`) the wrapper draws that one.
 * `hidden` takes it away, rules and all, while a search in Settings hides every row in it. */
export function LeadRows({ ruledBelow = false, hidden = false, children }: { ruledBelow?: boolean; hidden?: boolean; children: React.ReactNode }): React.ReactElement {
  return <div className={ruledBelow ? "hairline-t hairline-b" : "hairline-t"} hidden={hidden}>{children}</div>;
}
