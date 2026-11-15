import type { PlainMessageKey } from "../i18n/catalog";
import { isLanguage, LANGUAGE_NAMES } from "../i18n/languages";
import type { ClientSettingValue } from "./clientSettings";

/** The agent language tag a configuration document asks for, as the daemon reads it: its `agent_language`, or, in a
 * document of a build that held one language for the windows and the agents alike, its `interface_language`;
 * `undefined` when it carries neither. */
export function documentAgentLanguage(document: unknown): unknown {
  const settings = (document as { settings?: { agent_language?: unknown; interface_language?: unknown } } | null | undefined)?.settings;
  return settings?.agent_language ?? settings?.interface_language;
}

/**
 * The line of the agent language for the import cards: the daemon holds it and an import applies it through the daemon,
 * so it is not a client setting, but it is worded like one, in its own name with its own `lang`. `wanted` is the tag
 * the document asks for (or the import's reply says was applied), a line only when it is a language this build offers;
 * with `current`, the agent language the daemon holds now, the line says what would change (a tag this build does not
 * offer is no value to compare with). The line is a `ClientSettingValue` so the cards render it as the others.
 */
export function agentLanguageSettingValues(wanted: unknown, current?: unknown): ClientSettingValue[] {
  if (!isLanguage(wanted)) return [];
  const label: PlainMessageKey = "settings.agentLanguage.label";
  return [
    {
      field: "agent_language",
      label,
      value: LANGUAGE_NAMES[wanted],
      lang: wanted,
      ...(isLanguage(current) && { current: LANGUAGE_NAMES[current], currentLang: current }),
    },
  ];
}
