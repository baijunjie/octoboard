/** The `localStorage` key of every persisted user preference, kept apart from the modules that
 * read them so that a key can be named without loading the module that reads it, which reads its
 * preference as it is evaluated. */
export const PREFERENCE_KEYS = {
  language: "octoboard.language",
  sidebarVisible: "octoboard.sidebarVisible",
  // The aside's keys keep the names they had while the pane only ever held the report panel, so a
  // choice made then still applies. The visibility key is no longer written: the open panels
  // (`panelStacks`) say whether the pane shows, and a hidden pane found here is carried over as
  // no panel open.
  asideVisible: "octoboard.reportVisible",
  sidebarWidth: "octoboard.sidebarWidth",
  asideWidth: "octoboard.reportWidth",
  panelStacks: "octoboard.panelStacks",
  activeShellTerminals: "octoboard.activeShellTerminals",
  sidebarConsole: "octoboard.sidebarConsole",
  projectFocusIncludeConsoleSessions: "octoboard.projectFocusIncludeConsoleSessions",
  sidebarFocus: "octoboard.sidebarFocus",
  projectBrowsers: "octoboard.projectBrowsers",
  diffLayout: "octoboard.diffLayout",
  wordWrap: "octoboard.wordWrap",
  changeLayout: "octoboard.changeLayout",
  markdownView: "octoboard.markdownView",
  markdownStyle: "octoboard.markdownStyle",
  runInBackground: "octoboard.runInBackground",
} as const;
