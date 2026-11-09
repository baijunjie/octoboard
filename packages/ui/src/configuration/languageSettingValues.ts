import type { PlainMessageKey } from "../i18n/catalog";
import { isLanguage, LANGUAGE_NAMES } from "../i18n/languages";
import type { ClientSettingValue } from "./clientSettings";

/** The language tag a configuration document carries in its settings, `undefined` when it carries none. */
export function documentLanguage(document: unknown): unknown {
  return (document as { settings?: { interface_language?: unknown } } | null | undefined)?.settings?.interface_language;
}

/**
 * The line of the language for the import cards: the daemon holds it and an import applies it through the daemon, so
 * it is not a client setting, but it is worded like one, in its own name with its own `lang`. `wanted` is the tag the
 * document (or the import's reply) carries, a line only when it is a language this build offers; with `current`, the
 * language the daemon holds now, the line says what would change (a tag this build does not offer is no value to
 * compare with). The line is a `ClientSettingValue` so the cards render it as the others.
 */
export function languageSettingValues(wanted: unknown, current?: unknown): ClientSettingValue[] {
  if (!isLanguage(wanted)) return [];
  const label: PlainMessageKey = "settings.language.label";
  return [
    {
      field: "interface_language",
      label,
      value: LANGUAGE_NAMES[wanted],
      lang: wanted,
      ...(isLanguage(current) && { current: LANGUAGE_NAMES[current], currentLang: current }),
    },
  ];
}
