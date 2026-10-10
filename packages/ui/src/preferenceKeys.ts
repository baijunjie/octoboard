/** The `localStorage` key of every persisted user preference, kept apart from the modules that
 * read them so that a key can be named without loading the module that reads it, which reads its
 * preference as it is evaluated. */
export const PREFERENCE_KEYS = {
  language: "octoboard.language",
  sidebarVisible: "octoboard.sidebarVisible",
  // The aside's two keys keep the names they had while the pane only ever held the report panel, so
  // a choice made then still applies.
  asideVisible: "octoboard.reportVisible",
  sidebarWidth: "octoboard.sidebarWidth",
  asideWidth: "octoboard.reportWidth",
  sidebarConsole: "octoboard.sidebarConsole",
  sidebarFocus: "octoboard.sidebarFocus",
  projectBrowsers: "octoboard.projectBrowsers",
  diffLayout: "octoboard.diffLayout",
  wordWrap: "octoboard.wordWrap",
} as const;
