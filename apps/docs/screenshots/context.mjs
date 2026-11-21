// Screenshots of the context features. Entry fields are described in `entries.mjs`.
/** @type {import("./manifest.d.mts").ScreenshotEntry[]} */
export default [
  { id: "context-window-card", scenario: "focus-context-tooltip-warning", width: 1100, height: 720, clip: { selector: '[data-region="sidebar"]:visible >> nth=0', padding: 0 }, keep: "[role=tooltip]" },
  { id: "session-history", scenario: "history-loaded", width: 1100, height: 760 },
  { id: "handoff-dialog", scenario: "dialog-handoff", width: 1100, height: 1100, clip: '[role="dialog"]' },
  { id: "open-in-app-menu", scenario: "sidebar-session-menu-open-in-app", width: 1100, height: 760, clip: '[role="menu"]' },
  { id: "open-in-app-archive", scenario: "archive-open-in-app-row", width: 1100, height: 420, hover: '[role=button]:has-text("Review the release checklist")' },
  { id: "memory-setting", scenario: "settings-memory-left-behind", width: 1100, height: 800 },
  { id: "memory-dialog", scenario: "console-memory-list-plain", width: 1100, height: 900, clip: '[role="dialog"]' },
  { id: "memory-entry-edit", scenario: "console-memory-edit-plain", width: 1100, height: 900, clip: '[role="dialog"] >> nth=-1' },
];
