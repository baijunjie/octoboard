import type { PlainMessageKey } from "../i18n/catalog";
import { setLanguage } from "../i18n/language";
import { isLanguage, LANGUAGE_NAMES, type Language } from "../i18n/languages";
import type { ThemeChoice } from "../preferences/theme";
import { runInBackground } from "../lifecycle/runInBackground";
import { MARKDOWN_STYLE_NAMES, MARKDOWN_STYLES, markdownStyle, type MarkdownStyle } from "../viewer/markdown/markdownStyle";

/** The application settings only this client holds or reports, as a configuration document's `settings` carries them:
 * the daemon builds none of them but the language, stores none and passes them through an import's reply. Each is
 * absent when the document carries none, or one this client cannot stand behind (a value it does not know, or a
 * setting its platform does not have), which leaves this client's own as it is. */
export interface ClientSettings {
  appearance?: ThemeChoice;
  markdown_style?: MarkdownStyle;
  run_in_background?: boolean;
  interface_language?: Language;
}

/** What this client's platform has to offer the settings above. */
export interface ClientSettingsPlatform {
  /** Whether closing the window can leave the app running in the background (the desktop app). */
  backgroundRun: boolean;
}

const APPEARANCES: readonly ThemeChoice[] = ["light", "dark", "system"];

const APPEARANCE_LABEL: Record<ThemeChoice, PlainMessageKey> = {
  light: "settings.appearance.light",
  dark: "settings.appearance.dark",
  system: "settings.appearance.system",
};

/** The values this client adds to a document it saves with the application settings: the appearance, the Markdown style,
 * whether it runs in the background (only where the platform has that) and the language it shows, which is the one the
 * user chose and so replaces whatever the daemon held. */
export function heldClientSettings(
  current: { appearance: ThemeChoice; language: Language },
  platform: ClientSettingsPlatform,
): ClientSettings {
  return {
    appearance: current.appearance,
    markdown_style: markdownStyle.get(),
    ...(platform.backgroundRun && { run_in_background: runInBackground.get() }),
    interface_language: current.language,
  };
}

/** The client settings a document carries in its `settings` that this client can stand behind. */
export function documentClientSettings(document: unknown, platform: ClientSettingsPlatform): ClientSettings {
  return clientSettingsOf((document as { settings?: unknown } | null)?.settings, platform);
}

/** The client settings `settings` (a document's, or an import's reply) carries that this client can stand behind. */
export function clientSettingsOf(settings: unknown, platform: ClientSettingsPlatform): ClientSettings {
  if (typeof settings !== "object" || settings === null) return {};
  const { appearance, markdown_style, run_in_background, interface_language } = settings as Record<string, unknown>;
  return {
    ...(APPEARANCES.includes(appearance as ThemeChoice) && { appearance: appearance as ThemeChoice }),
    ...(MARKDOWN_STYLES.includes(markdown_style as MarkdownStyle) && { markdown_style: markdown_style as MarkdownStyle }),
    ...(platform.backgroundRun && typeof run_in_background === "boolean" && { run_in_background }),
    ...(isLanguage(interface_language) && { interface_language }),
  };
}

/** Applies `values` to this client; a value the document did not carry is left as it is. */
export function applyClientSettings(values: ClientSettings, setAppearance: (choice: ThemeChoice) => void): void {
  if (values.appearance !== undefined) setAppearance(values.appearance);
  if (values.markdown_style !== undefined) markdownStyle.set(values.markdown_style);
  if (values.run_in_background !== undefined) runInBackground.set(values.run_in_background);
  if (values.interface_language !== undefined) setLanguage(values.interface_language);
}

/** One client setting for the import cards: its settings-page name, and its value worded for it. `current`, the
 * value this client has now worded the same way, is present while reviewing. `lang` and `currentLang` are the language
 * of a value that is a name in a language of its own (a language's name, a Markdown style's). */
export interface ClientSettingValue {
  field: keyof ClientSettings;
  label: PlainMessageKey;
  value: string;
  lang?: string;
  current?: string;
  currentLang?: string;
}

/** The client settings `wanted` carries, in the order of the settings pages, worded as their settings pages word them.
 * With `current` (this client's own values), each carries the value this client has now. */
export function clientSettingValues(
  t: (key: PlainMessageKey) => string,
  wanted: ClientSettings,
  current?: ClientSettings,
): ClientSettingValue[] {
  // The language a value is named in, where it is one of its own: the language names are each in their language and
  // the style names in English, as the settings pages mark them.
  const words: { [F in keyof ClientSettings]-?: [PlainMessageKey, (value: NonNullable<ClientSettings[F]>) => string, ((value: NonNullable<ClientSettings[F]>) => string)?] } = {
    appearance: ["settings.appearance.label", (value) => t(APPEARANCE_LABEL[value])],
    interface_language: ["settings.language.label", (value) => LANGUAGE_NAMES[value], (value) => value],
    markdown_style: ["settings.markdownStyle.label", (value) => MARKDOWN_STYLE_NAMES[value], () => "en"],
    run_in_background: ["settings.background.label", (value) => t(value ? "configuration.import.value.on" : "configuration.import.value.off")],
  };
  return (Object.keys(words) as (keyof ClientSettings)[]).flatMap((field) => {
    const [label, word, language] = words[field] as [PlainMessageKey, (value: unknown) => string, ((value: unknown) => string)?];
    const value = wanted[field];
    if (value === undefined) return [];
    const now = current?.[field];
    return [
      {
        field,
        label,
        value: word(value),
        ...(language && { lang: language(value) }),
        ...(current && now !== undefined && { current: word(now), ...(language && { currentLang: language(now) }) }),
      },
    ];
  });
}
