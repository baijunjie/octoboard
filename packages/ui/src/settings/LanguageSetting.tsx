import React from "react";

import { OptionSelect } from "../components/form/OptionSelect";
import { setLanguage } from "../i18n/language";
import { isLanguage, LANGUAGE_NAMES, LANGUAGES, type Language } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import { useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
import { useSettingsWrite } from "./useSettingsWrite";

const OPTIONS = LANGUAGES.map((language) => ({ value: language, label: LANGUAGE_NAMES[language], lang: language }));

/** The language of the UI: one of the offered languages, each named in itself so a user can find theirs whatever the
 * current language is. It is the client's own and never sent to a daemon: a pick takes effect here at once and is kept
 * in this client's storage, which every window of the desktop application shares (see `i18n/language.ts`). */
export function InterfaceLanguageSetting(): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  return (
    <SettingRow fill searchId="language" label={t("settings.language.label")} description={t("settings.language.description")}>
      <OptionSelect inline label={t("settings.language.label")} options={OPTIONS} value={language} onChange={setLanguage} />
    </SettingRow>
  );
}

/** The language this window's daemon tells sessions to talk to the user in (`Settings.agent_language`), changed through
 * `update_settings` as every daemon setting is, so the window of a daemon elsewhere shows and changes that daemon's.
 * Until the daemon holds one this build offers, the row shows the interface language, which the daemon is given when
 * it holds none (see `useAgentLanguageSeed`). */
export function AgentLanguageSetting(): React.ReactElement {
  const t = useT();
  const write = useSettingsWrite();
  const held = useDaemonStore((s) => s.settings.agent_language);
  const shown = useCurrentLanguage();
  const pick = (next: Language) => write({ agent_language: next }, ["agent_language"]);
  return (
    <SettingRow fill searchId="agent-language" label={t("settings.agentLanguage.label")} description={t("settings.agentLanguage.description")}>
      <OptionSelect inline label={t("settings.agentLanguage.label")} options={OPTIONS} value={isLanguage(held) ? held : shown} onChange={pick} />
    </SettingRow>
  );
}
