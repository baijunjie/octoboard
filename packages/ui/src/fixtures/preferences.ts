import type { Language } from "../i18n/languages";
import type { PanelKind } from "../layout/panelKind";
import type { OwnerKind } from "../panels/stackState";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { UpdateChannel } from "../platform";
import type { Overrides } from "../shortcuts/store";
import type { MarkdownStyle } from "../viewer/markdownStyle";

/** The user preferences a scenario starts from, each persisted in `localStorage` by the module the
 * key names. Anything not listed is unset, so the app's own default applies. */
export interface Preferences {
  /** The docked sidebar is shown (default) or hidden by the user. */
  sidebarVisible?: boolean;
  /** Keep the right pane docked on wide screens (default), or float its open panels together. */
  asideDocked?: boolean;
  /** Chosen docked widths in pixels. */
  sidebarWidth?: number;
  asideWidth?: number;
  /** The right pane's panels per owner (a project, a console session's report): which kinds of panel
   * are open (none open is no pane) and their shares of its height. An owner left out has every one of
   * its kinds open, the app's default. */
  panelStacks?: Partial<Record<OwnerKind, { open: PanelKind[]; shares: Record<string, number> }>>;
  /** The project sessions, by id, whose Report panel the user has closed; a session left out has it open. */
  closedReportPanels?: string[];
  /** The console the sidebar shows, and what is in focus mode. */
  sidebarConsole?: string;
  sidebarFocus?: `project:${string}` | `consoleSession:${string}`;
  /** Include console-bound sessions as cards in project focus; off by default. */
  projectFocusIncludeConsoleSessions?: boolean;
  /** How the Git mode's change lists show their changes. */
  changeLayout?: "flat" | "tree";
  /** What each project's browser is left at, by project id: the mode it opens in and whether the Git
   * mode lists what is committed by commit. */
  projectBrowsers?: Record<string, { mode: "files" | "git"; byCommit?: boolean }>;
  /** How a rendered Markdown document is styled; Tailwind, the original look, when unset. */
  markdownStyle?: MarkdownStyle;
  /** The layout diffs open in; unified when unset. */
  diffLayout?: "unified" | "split";
  /** Closing the window keeps the app running in the background; off by default. */
  runInBackground?: boolean;
  /** The release channel the application updates from; stable when unset. */
  updateChannel?: UpdateChannel;
  /** The keyboard shortcuts the user has changed, by command; the others keep their defaults. */
  keyboardShortcuts?: Overrides;
}

/** A scenario's preference as its owner stores it: a structured one (the panel stacks) as the JSON
 * its owner parses, anything else as its string. */
export function storedPreference(value: NonNullable<Preferences[keyof Preferences]>): string {
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}

/** Sets what the app reads from `storage` as its modules load to a page's starting point: the theme the page was
 * asked for, the language, and each preference of `preferences`. The page runs it before it loads the app, since the
 * modules that own these read them as they are evaluated. Dropping what an earlier page left is the caller's. */
export function writeStartingPreferences(
  storage: Pick<Storage, "setItem">,
  { language, theme, preferences = {} }: { language: Language; theme: "light" | "dark"; preferences?: Preferences },
): void {
  storage.setItem("heroui-theme", theme);
  storage.setItem(PREFERENCE_KEYS.language, language);
  for (const [name, value] of Object.entries(preferences)) {
    if (value !== undefined) storage.setItem(PREFERENCE_KEYS[name as keyof Preferences], storedPreference(value));
  }
}
