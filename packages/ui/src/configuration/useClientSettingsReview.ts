import { clientSettingsPlatform, clientSettingValues, documentClientSettings, type ClientSettingValue } from "./clientSettings";
import { sessionOrderValues, type SettingValue } from "./importValues";
import { useCurrentLanguage, useT } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { keepAwake, keepAwakeWhen } from "../lifecycle/keepAwake";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { usePlatform } from "../platform/react";
import { useOctoboardTheme } from "../preferences/theme";
import { sessionOrderPreference } from "../store";
import { markdownStyle } from "../viewer/markdown/markdownStyle";

/**
 * What the settings only this client holds would do here, for the review of `document` (absent outside the review): one
 * entry of `values` for each such setting the document carries, with the value this client has, and the session order's
 * items that would change (`sorting`, `undefined` when it carries none). `platformSettings` says which of the
 * platform-dependent ones this client has at all, and `setAppearance` applies the appearance, which applying the result
 * needs as well.
 */
export function useClientSettingsReview(document: unknown) {
  const t = useT();
  const platform = usePlatform();
  const { choice: appearance, setChoice: setAppearance } = useOctoboardTheme();
  const language = useCurrentLanguage();
  const markdown = markdownStyle.useValue();
  const inBackground = runInBackground.useValue();
  const menuBarIconShown = showMenuBarIcon.useValue();
  const keepingAwake = keepAwake.useValue();
  const keepAwakeWhenValue = keepAwakeWhen.useValue();
  const sessionOrder = sessionOrderPreference.useValue();
  const platformSettings = clientSettingsPlatform(platform);
  const documentSettings = document === undefined ? {} : documentClientSettings(document, platformSettings);
  const sorting: SettingValue[] | undefined = documentSettings.session_order && sessionOrderValues(t, documentSettings.session_order, sessionOrder);
  const values: ClientSettingValue[] = document === undefined
    ? []
    : clientSettingValues(t, documentSettings, {
      appearance,
      markdown_style: markdown,
      ...(platformSettings.backgroundRun && { run_in_background: inBackground }),
      ...(platformSettings.menuBarIcon && { show_menu_bar_icon: menuBarIconShown }),
      ...(platformSettings.keepAwake && { keep_awake: keepingAwake, keep_awake_when: keepAwakeWhenValue }),
      interface_language: language,
    });
  return { platformSettings, setAppearance, sorting, values };
}
