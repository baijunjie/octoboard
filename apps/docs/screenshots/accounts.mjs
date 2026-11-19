// Screenshots of the account features. Entry fields are described in `entries.mjs`.
/** @type {import("./manifest.d.mts").ScreenshotEntry[]} */
export default [
  { id: "accounts-table", scenario: "settings-accounts-emails-short", width: 1180, height: 900 },
  { id: "account-actions-menu", scenario: "settings-account-menu-owned", width: 1180, height: 780 },
  { id: "account-login-dialog", scenario: "settings-account-login-waiting", width: 1000, height: 700, clip: "[role=dialog][aria-labelledby]" },
  { id: "switch-account-dialog", scenario: "dialog-switch-account-single-plain-name", width: 900, height: 700, clip: "[role=alertdialog]" },
  { id: "switch-account-menu", scenario: "sidebar-session-switch-account-quota-plain-menu", width: 1100, height: 760 },
  { id: "new-session-account-picker", scenario: "dialog-new-session-quota-picker", width: 1100, height: 760 },
  { id: "rail-usage-panel", scenario: "rail-quota-hover-plain", width: 1100, height: 760 },
  { id: "account-details-usage", scenario: "settings-account-details", width: 1000, height: 760, clip: "[role=dialog][aria-labelledby]" },
  { id: "account-sharing-switch", scenario: "settings-accounts-sharing", width: 1000, height: 760 },
  { id: "account-connectors-mcp", scenario: "settings-account-mcp-codex", width: 1200, height: 780 },
  { id: "start-usage-windows-help", scenario: "settings-accounts-start-windows-help-popover", width: 1180, height: 900 },
];
