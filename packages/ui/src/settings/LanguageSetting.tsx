import React, { useRef, useState } from "react";

import { OptionSelect } from "../components/form/OptionSelect";
import { currentLanguage, setLanguage, t as translate } from "../i18n/language";
import { isLanguage, LANGUAGE_NAMES, LANGUAGES, type Language } from "../i18n/languages";
import { useStableHandler } from "../components/hooks/useLatest";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { createSettingsWriter, useDaemon } from "../store";
import { SettingRow } from "./SettingRow";
import { alertAppDataFailure } from "../components/alerts/raiseError";

const OPTIONS = LANGUAGES.map((language) => ({ value: language, label: LANGUAGE_NAMES[language], lang: language }));

/** The language of the UI: one of the offered languages, each named in itself so a user can find
 * theirs whatever the current language is. It is the one language every client shows and the agents
 * are told to reply in (`Settings.interface_language` in protocol.generated.ts): a pick takes effect
 * here at once and is sent to the daemon, which tells every other window (see `useInterfaceLanguage`).
 * A pick the daemon refuses is taken back. */
export function LanguageSetting(): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  // The language shown before the latest pick, which a refused write goes back to where the daemon holds none.
  const previous = useRef<Language>(language);
  const onFailure = useStableHandler((error: unknown) => {
    const held = store.getState().settings.interface_language;
    setLanguage(isLanguage(held) ? held : previous.current);
    // Worded after the rollback, in the language the window shows again, not the one the render saw.
    alertAppDataFailure(translate("settings.saveFailedTitle"))(error);
  });
  const [write] = useState(() => createSettingsWriter({ store, request, onFailure }));
  const pick = (next: Language) => {
    // A pick the writer would drop (the previous one is not yet in the store) is ignored whole, as the other controls
    // ignore a press that counts for nothing, so the window never shows a language the daemon was not sent.
    if (write.wouldDrop(["interface_language"])) return;
    previous.current = currentLanguage();
    setLanguage(next);
    void write({ interface_language: next }, ["interface_language"]);
  };
  return (
    <SettingRow fill searchId="language" label={t("settings.language.label")} description={<Message id="settings.language.sharedDescription" />}>
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
