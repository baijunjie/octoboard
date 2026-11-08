import type { PlainMessageKey } from "../i18n/catalog";
import { setLanguage } from "../i18n/language";
import { isLanguage, LANGUAGE_NAMES, type Language } from "../i18n/languages";
import type { ThemeChoice } from "../preferences/theme";
import { runInBackground } from "../lifecycle/runInBackground";
import { isKeepAwakeWhen, keepAwake, KEEP_AWAKE_WHEN_LABEL, keepAwakeWhen, type KeepAwakeWhen } from "../lifecycle/keepAwake";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import type { DocumentSessionOrder, SettingsSection } from "../protocol";
import { ACTIVE_PROJECT_SORTS, INACTIVE_PROJECT_SORTS, SESSION_SORTS, type SessionOrder } from "../sessions/order";
import { sessionOrderPreference } from "../store";
import { MARKDOWN_STYLE_NAMES, MARKDOWN_STYLES, markdownStyle, type MarkdownStyle } from "../viewer/markdown/markdownStyle";

/** The application settings only this client holds or reports, as a configuration document's `settings` carries them:
 * the daemon builds none of them but the language, stores none and passes them through an import's reply. Each is
 * absent when the document carries none, or one this client cannot stand behind (a value it does not know, or a
 * setting its platform does not have), which leaves this client's own as it is. */
export interface ClientSettings {
  appearance?: ThemeChoice;
  markdown_style?: MarkdownStyle;
  run_in_background?: boolean;
  show_menu_bar_icon?: boolean;
  keep_awake?: boolean;
  keep_awake_when?: KeepAwakeWhen;
  interface_language?: Language;
  /** The parts of the session order the document carries that this client knows. */
  session_order?: Partial<SessionOrder>;
}

/** The client settings that are one value worded on a line of their own; the session order is a setting of several
 * items, worded by `sessionOrderValues`. */
type ScalarClientField = Exclude<keyof ClientSettings, "session_order">;

/** What this client's platform has to offer the settings above. */
export interface ClientSettingsPlatform {
  /** Whether closing the window can leave the app running in the background (the desktop app). */
  backgroundRun: boolean;
  /** Whether the app has a menu bar icon (a notification area icon on Windows) to show or hide (the desktop app). */
  menuBarIcon: boolean;
  /** Whether the app can hold the computer and its display awake (the desktop app). */
  keepAwake: boolean;
}

/** What `platform`'s capabilities (each present only where the platform has it) say this client's platform offers. */
export function clientSettingsPlatform(platform: { backgroundRun?: unknown; statusItem?: unknown; keepAwake?: unknown }): ClientSettingsPlatform {
  return { backgroundRun: platform.backgroundRun !== undefined, menuBarIcon: platform.statusItem !== undefined, keepAwake: platform.keepAwake !== undefined };
}

const APPEARANCES: readonly ThemeChoice[] = ["light", "dark", "system"];

const APPEARANCE_LABEL: Record<ThemeChoice, PlainMessageKey> = {
  light: "settings.appearance.light",
  dark: "settings.appearance.dark",
  system: "settings.appearance.system",
};

/** The values this client adds to a document it saves, for the Settings `sections` the document carries: for General the
 * appearance, the Markdown style, the session order (all three parts) and the language it shows, which is the one the
 * user chose and so replaces whatever the daemon held; for App behavior whether it runs in the background, whether it
 * shows its menu bar icon, whether it keeps the computer awake and when (each only where the platform has that). */
export function heldClientSettings(
  current: { appearance: ThemeChoice; language: Language },
  platform: ClientSettingsPlatform,
  sections: readonly SettingsSection[],
): Omit<ClientSettings, "session_order"> & { session_order?: DocumentSessionOrder } {
  const order = sessionOrderPreference.get();
  return {
    ...(sections.includes("general") && {
      appearance: current.appearance,
      markdown_style: markdownStyle.get(),
      session_order: { sessions: order.sessions, active_projects: order.activeProjects, inactive_projects: order.inactiveProjects },
      interface_language: current.language,
    }),
    ...(sections.includes("app_behavior") && {
      ...(platform.backgroundRun && { run_in_background: runInBackground.get() }),
      ...(platform.menuBarIcon && { show_menu_bar_icon: showMenuBarIcon.get() }),
      ...(platform.keepAwake && { keep_awake: keepAwake.get(), keep_awake_when: keepAwakeWhen.get() }),
    }),
  };
}

/** The client settings a document carries in its `settings` that this client can stand behind. */
export function documentClientSettings(document: unknown, platform: ClientSettingsPlatform): ClientSettings {
  return clientSettingsOf((document as { settings?: unknown } | null)?.settings, platform);
}

/** The client settings `settings` (a document's, or an import's reply) carries that this client can stand behind. */
export function clientSettingsOf(settings: unknown, platform: ClientSettingsPlatform): ClientSettings {
  if (typeof settings !== "object" || settings === null) return {};
  const { appearance, markdown_style, run_in_background, show_menu_bar_icon, keep_awake, keep_awake_when, interface_language, session_order } = settings as Record<string, unknown>;
  const order = sessionOrderOf(session_order);
  return {
    ...(APPEARANCES.includes(appearance as ThemeChoice) && { appearance: appearance as ThemeChoice }),
    ...(MARKDOWN_STYLES.includes(markdown_style as MarkdownStyle) && { markdown_style: markdown_style as MarkdownStyle }),
    ...(platform.backgroundRun && typeof run_in_background === "boolean" && { run_in_background }),
    ...(platform.menuBarIcon && typeof show_menu_bar_icon === "boolean" && { show_menu_bar_icon }),
    ...(platform.keepAwake && typeof keep_awake === "boolean" && { keep_awake }),
    ...(platform.keepAwake && isKeepAwakeWhen(keep_awake_when) && { keep_awake_when }),
    ...(isLanguage(interface_language) && { interface_language }),
    ...(Object.keys(order).length > 0 && { session_order: order }),
  };
}

/** The parts of a document's `session_order` whose value this client knows. */
function sessionOrderOf(value: unknown): Partial<SessionOrder> {
  if (typeof value !== "object" || value === null) return {};
  const { sessions, active_projects, inactive_projects } = value as Record<string, unknown>;
  const sessionsSort = SESSION_SORTS.find((sort) => sort === sessions);
  const activeProjects = ACTIVE_PROJECT_SORTS.find((sort) => sort === active_projects);
  const inactiveProjects = INACTIVE_PROJECT_SORTS.find((sort) => sort === inactive_projects);
  return {
    ...(sessionsSort && { sessions: sessionsSort }),
    ...(activeProjects && { activeProjects }),
    ...(inactiveProjects && { inactiveProjects }),
  };
}

/** Applies `values` to this client; a value the document did not carry is left as it is, and so is a part of the session
 * order it did not carry. */
export function applyClientSettings(values: ClientSettings, setAppearance: (choice: ThemeChoice) => void): void {
  if (values.appearance !== undefined) setAppearance(values.appearance);
  if (values.markdown_style !== undefined) markdownStyle.set(values.markdown_style);
  if (values.run_in_background !== undefined) runInBackground.set(values.run_in_background);
  if (values.show_menu_bar_icon !== undefined) showMenuBarIcon.set(values.show_menu_bar_icon);
  if (values.keep_awake !== undefined) keepAwake.set(values.keep_awake);
  if (values.keep_awake_when !== undefined) keepAwakeWhen.set(values.keep_awake_when);
  if (values.interface_language !== undefined) setLanguage(values.interface_language);
  if (values.session_order !== undefined) sessionOrderPreference.set({ ...sessionOrderPreference.get(), ...values.session_order });
}

/** One client setting for the import cards: its settings-page name, and its value worded for it. `current`, the
 * value this client has now worded the same way, is present while reviewing. `lang` and `currentLang` are the language
 * of a value that is a name in a language of its own (a language's name, a Markdown style's). */
export interface ClientSettingValue {
  field: ScalarClientField;
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
  const onOff = (value: boolean) => t(value ? "configuration.import.value.on" : "configuration.import.value.off");
  const words: { [F in ScalarClientField]-?: [PlainMessageKey, (value: NonNullable<ClientSettings[F]>) => string, ((value: NonNullable<ClientSettings[F]>) => string)?] } = {
    appearance: ["settings.appearance.label", (value) => t(APPEARANCE_LABEL[value])],
    interface_language: ["settings.language.label", (value) => LANGUAGE_NAMES[value], (value) => value],
    markdown_style: ["settings.markdownStyle.label", (value) => MARKDOWN_STYLE_NAMES[value], () => "en"],
    run_in_background: ["settings.background.label", onOff],
    show_menu_bar_icon: ["settings.menuBarIcon.label", onOff],
    keep_awake: ["settings.keepAwake.label", onOff],
    keep_awake_when: ["settings.keepAwake.when.label", (value) => t(KEEP_AWAKE_WHEN_LABEL[value])],
  };
  return (Object.keys(words) as ScalarClientField[]).flatMap((field) => {
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
