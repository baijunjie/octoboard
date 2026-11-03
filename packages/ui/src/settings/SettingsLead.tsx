import React from "react";

/** The sentence that opens a settings section, above its rows. */
export function SettingsLead({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="pb-2 text-sm text-app-muted">{children}</p>;
}

/** The rows under a `SettingsLead`. `SettingRow` rules between its rows but not above the first, and the lead above
 * would otherwise run straight into a description in the same muted text; this wrapper draws that rule. Its last row
 * draws no rule under itself either, so where more of the section follows (`ruledBelow`) the wrapper draws that one. */
export function LeadRows({ ruledBelow = false, children }: { ruledBelow?: boolean; children: React.ReactNode }): React.ReactElement {
  // Two grooves on one element would share one `border-image`, drawing only one of them.
  return <div className="groove-t">{ruledBelow ? <div className="groove-b">{children}</div> : children}</div>;
}
