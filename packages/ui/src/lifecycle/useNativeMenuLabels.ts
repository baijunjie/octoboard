import { useEffect } from "react";

import { format, type PlainMessageKey } from "../i18n/catalog";
import { useCurrentLanguage } from "../i18n/react";
import { en } from "../i18n/messages/en";
import { windowKind } from "../platform";
import { usePlatform } from "../platform/react";
import { toAccelerator } from "../shortcuts/binding";
import { useBinding } from "../shortcuts/store";
import { useSerialPush } from "../components/hooks/useSerialPush";

/** The labels the shell's menu takes, every `menu.*` message; the shell reads each back under the
 * same key. */
const MENU_LABEL_KEYS = Object.keys(en).filter((key) => key.startsWith("menu.")) as PlainMessageKey[];

/**
 * Pushes the menu's labels, in the language the UI renders, and the key of its Settings item, as
 * the user has bound it, to the native menu bar on mount and on every language or binding change.
 * Only the local daemon's window does (`windowKind`). Best-effort: a menu left in its previous language or key is a
 * cosmetic defect, not one worth surfacing to the user.
 */
export function useNativeMenuLabels(): void {
  const { appMenu, jumpList, host } = usePlatform();
  const language = useCurrentLanguage();
  const accelerator = toAccelerator(useBinding("openSettings"), host);
  // Serial, so two in-flight pushes cannot settle out of order and leave the menu in the earlier language.
  const push = useSerialPush();
  // The native menus are the application's, in the language of the local daemon's window: a remote daemon's window and
  // the window of the tray flyout, which has no daemon and shows the local language, leave them as they are.
  const pushesLabels = windowKind() === "local";

  useEffect(() => {
    if (!appMenu || !pushesLabels) return;
    const labels = Object.fromEntries(MENU_LABEL_KEYS.map((key) => [key, format(language, key)]));
    push(() => appMenu.setLabels(labels, accelerator));
  }, [appMenu, pushesLabels, language, accelerator, push]);

  // The jump list's and the windows' system menus' lines, sent under the app menu's keys. Windows writes its menus in
  // sentence case, where the macOS app menu takes Title Case, and the words for the local window are the PC's.
  useEffect(() => {
    if (!jumpList || !pushesLabels) return;
    const labels = {
      "menu.remote": format(language, "menu.remote"),
      "menu.remoteDevices": format(language, "jumpList.remoteDevices"),
      "menu.noRemoteDaemons": format(language, "jumpList.noRemoteDaemons"),
      "menu.openMainWindow": format(language, "jumpList.openMainWindow"),
    };
    push(() => jumpList.setLabels(labels));
  }, [jumpList, pushesLabels, language, push]);
}
