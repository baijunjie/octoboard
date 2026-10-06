import { useEffect, useRef } from "react";

import { format, type PlainMessageKey } from "../i18n/catalog";
import { useCurrentLanguage } from "../i18n/react";
import { en } from "../i18n/messages/en";
import { usePlatform } from "../platform/react";

/** The labels the shell's menu takes, every `menu.*` message; the shell reads each back under the
 * same key. */
const MENU_LABEL_KEYS = Object.keys(en).filter((key) => key.startsWith("menu.")) as PlainMessageKey[];

/**
 * Pushes the menu's labels, in the language the UI renders, to the native menu bar on mount and on
 * every language change. Best-effort: a menu left in its previous language is a cosmetic defect,
 * not one worth surfacing to the user.
 */
export function useNativeMenuLabels(): void {
  const { appMenu } = usePlatform();
  const language = useCurrentLanguage();
  // Chained for the same reason as in `useNativeWindowTheme`: two in-flight pushes must not settle
  // out of order and leave the menu in the earlier language.
  const pendingRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!appMenu) return;
    const labels = Object.fromEntries(MENU_LABEL_KEYS.map((key) => [key, format(language, key)]));
    pendingRef.current = pendingRef.current.then(() => appMenu.setLabels(labels).catch(() => {}));
  }, [appMenu, language]);
}
