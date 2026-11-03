import { useEffect } from "react";

import { format, type PlainMessageKey } from "../i18n/catalog";
import { useCurrentLanguage } from "../i18n/react";
import { en } from "../i18n/messages/en";
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
 * Best-effort: a menu left in its previous language or key is a cosmetic defect, not one worth
 * surfacing to the user.
 */
export function useNativeMenuLabels(): void {
  const { appMenu } = usePlatform();
  const language = useCurrentLanguage();
  const accelerator = toAccelerator(useBinding("openSettings"));
  // Serial, so two in-flight pushes cannot settle out of order and leave the menu in the earlier language.
  const push = useSerialPush();

  useEffect(() => {
    if (!appMenu) return;
    const labels = Object.fromEntries(MENU_LABEL_KEYS.map((key) => [key, format(language, key)]));
    push(() => appMenu.setLabels(labels, accelerator));
  }, [appMenu, language, accelerator, push]);
}
