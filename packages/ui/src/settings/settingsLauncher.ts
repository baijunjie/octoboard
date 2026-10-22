import type { SettingsSectionId } from "./SettingsDialog";

/** How a control anywhere in the page opens the Settings dialog, which the window owns: on `section`, with its
 * Add agent dialog in front when `addAgent` is set, or, for `account` (an account ID), on that account's agent tab
 * with its sharing switch scrolled to and focused. A module of its own rather than a context because an error
 * alert is rendered beside the window, not inside it. */
export type OpenSettings = (section: SettingsSectionId, options?: { addAgent?: boolean; account?: string }) => void;

let launcher: OpenSettings | undefined;

/** Registers the window's way of opening Settings, and returns what unregisters it. */
export function registerSettingsLauncher(open: OpenSettings): () => void {
  launcher = open;
  return () => {
    if (launcher === open) launcher = undefined;
  };
}

/** Opens Settings; does nothing while no window has registered. */
export const openSettingsFromAnywhere: OpenSettings = (section, options) => launcher?.(section, options);
