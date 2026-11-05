import type { HostFacts } from "../platform/host";
import type { AppMenuCapability } from "../platform";

/**
 * The native menu bar a desktop-app scenario has, which no menu is drawn for: only its subscriptions, so the page behaves
 * as it does where a shell has one (the Settings shortcut is the menu's, not the page's). A scenario that gives a `host`
 * has the menu its host says it has (`appMenu`: the Windows app has none, and the page handles that shortcut itself); one
 * that gives none keeps the menu every desktop-app scenario has always had, whatever the browser's own facts say. A
 * scenario that is not the desktop app has none.
 */
export function galleryAppMenu(desktopApp: boolean | undefined, host: Pick<HostFacts, "appMenu"> | undefined): AppMenuCapability | undefined {
  if (!desktopApp || (host !== undefined && !host.appMenu)) return undefined;
  return { onSettingsRequested: () => () => {}, onAboutRequested: () => () => {}, setLabels: async () => {} };
}
