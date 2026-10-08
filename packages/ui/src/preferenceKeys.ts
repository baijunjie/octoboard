/** The `localStorage` key of every persisted user preference, kept apart from the modules that
 * read them so that a key can be named without loading the module that reads it, which reads its
 * preference as it is evaluated. */
export const PREFERENCE_KEYS = {
  language: "octoboard.language",
  sidebarVisible: "octoboard.sidebarVisible",
  reportVisible: "octoboard.reportVisible",
  sidebarWidth: "octoboard.sidebarWidth",
  reportWidth: "octoboard.reportWidth",
  sidebarConsole: "octoboard.sidebarConsole",
  sidebarFocus: "octoboard.sidebarFocus",
} as const;
