// Screenshots of the collaboration features. Entry fields are described in `entries.mjs`.
/** @type {import("./manifest.d.mts").ScreenshotEntry[]} */
export default [
  { id: "console-sessions-sidebar", scenario: "console-sessions-section", width: 1100, height: 640, clip: { selector: "[data-region=\"sidebar\"]:visible >> nth=0", padding: 0 } },
  { id: "report-interactive", scenario: "report-two-console-sessions", width: 1280, height: 800, clip: { selector: "#panel-report", padding: 8 } },
  { id: "native-session-terminal", scenario: "terminal-native-session", width: 1100, height: 640, scale: 1 },
  { id: "project-launch-settings", scenario: "dialog-edit-project-launch-settings-unset", width: 1100, height: 1100, clip: "[role=\"dialog\"]" },
  { id: "keep-to-own-project-setting", scenario: "settings-agent-permissions", width: 1100, height: 800 },
  { id: "session-status-glyphs", scenario: "session-status-glyphs", width: 1100, height: 760, clip: { selector: "[data-region=\"sidebar\"]:visible >> nth=0", padding: 0 } },
  { id: "focus-mode-sessions", scenario: "focus-project", width: 1100, height: 760, clip: { selector: "[data-region=\"sidebar\"]:visible >> nth=0", padding: 0 } },
  { id: "processes-of-session", scenario: "processes-session-plain", width: 1280, height: 800, clip: { selector: "#panel-processes", padding: 8 } },
  { id: "processes-on-rail", scenario: "processes-rail-overview", width: 1100, height: 760 },
  { id: "processes-stop-confirm", scenario: "processes-stop-confirmation-plain", width: 900, height: 700, clip: "[role=alertdialog]" },
  { id: "git-changes-list", scenario: "git-changes-by-commit-plain", width: 1280, height: 800, clip: { selector: "#panel-files", padding: 0 } },
  { id: "git-change-diff", scenario: "git-change-viewer", width: 1280, height: 800 },
  { id: "files-tree-viewer", scenario: "files-viewer", width: 1280, height: 800 },
  { id: "files-tree", scenario: "files-tree-plain", width: 1280, height: 800, clip: { selector: "#panel-files", padding: 0 } },
  { id: "prevent-sleep-setting", scenario: "settings-general-browser-prevent-sleep", width: 1100, height: 800 },
];
