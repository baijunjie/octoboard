import React from "react";

import { OptionSelect } from "../dialogs/OptionSelect";
import { errorPresentation } from "../errorPresentation";
import { setLanguage } from "../i18n/language";
import { LANGUAGE_NAMES, LANGUAGES, type Language } from "../i18n/languages";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { useDaemon } from "../store";
import { SettingRow } from "./SettingRow";

const OPTIONS = LANGUAGES.map((language) => ({ value: language, label: LANGUAGE_NAMES[language], lang: language }));

/** The language of the UI: one of the offered languages, each named in itself so a user can find
 * theirs whatever the current language is. A pick is also reported to the daemon, which tells sessions
 * the user's language (see `Settings.interface_language` in protocol.ts). */
export function LanguageSetting(): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, toastError } = useDaemon();
  const pick = (next: Language) => {
    setLanguage(next);
    void request({ type: "update_settings", interface_language: next }).catch((err) => toastError(errorPresentation(err as Error)));
  };
  return (
    <SettingRow label={t("settings.language.label")} description={<Message id="settings.language.description" />}>
      <OptionSelect
        inline
        label={t("settings.language.label")}
        options={OPTIONS}
        value={language}
        onChange={pick}
      />
    </SettingRow>
  );
}
