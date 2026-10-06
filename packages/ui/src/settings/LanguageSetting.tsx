import React from "react";

import { OptionSelect } from "../dialogs/OptionSelect";
import { setLanguage } from "../i18n/language";
import { LANGUAGE_NAMES, LANGUAGES } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import { SettingRow } from "./SettingRow";

const OPTIONS = LANGUAGES.map((language) => ({ value: language, label: LANGUAGE_NAMES[language], lang: language }));

/** The language of the UI: one of the offered languages, each named in itself so a user can find
 * theirs whatever the current language is. */
export function LanguageSetting(): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  return (
    <SettingRow label={t("settings.language.label")} description={t("settings.language.description")}>
      <OptionSelect
        inline
        label={t("settings.language.label")}
        options={OPTIONS}
        value={language}
        onChange={setLanguage}
      />
    </SettingRow>
  );
}
