import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { ThemeChoice } from "../preferences/theme";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import type { DocumentSessionOrder, SettingsSection } from "../protocol";
import { ACTIVE_PROJECT_SORTS, INACTIVE_PROJECT_SORTS, SESSION_SORTS, type SessionOrder } from "../sessions/order";
import { sessionOrderPreference } from "../store";
import { clampTerminalFontSize, terminalFontSize } from "../terminal/terminalFontSize";
import { MARKDOWN_STYLE_NAMES, MARKDOWN_STYLES, markdownStyle, type MarkdownStyle } from "../viewer/markdown/markdownStyle";

/** The application settings only this client holds, as a configuration document's `settings` carries them: the daemon
 * builds none of them, stores none and passes them through an import's reply. Each is absent when the document carries
 * none, or one this client cannot stand behind (a value it does not know, or a setting its platform does not have),
 * which leaves this client's own as it is. */
export interface ClientSettings {
  appearance?: ThemeChoice;
  markdown_style?: MarkdownStyle;
  terminal_font_size?: number;
  run_in_background?: boolean;
  show_menu_bar_icon?: boolean;
  /** The parts of the session order the document carries that this client knows. */
  session_order?: Partial<SessionOrder>;
}

/** The client settings that are one value worded on a line of their own; the session order is a setting of several
 * items, worded by `sessionOrderValues`. */
type ScalarClientField = Exclude<keyof ClientSettings, "session_order">;

/** The language, which the daemon holds and an import applies through it, is worded beside the client settings (see
 * `languageSettingValues`). */
type LanguageField = "interface_language";

/** What this client's platform has to offer the settings above. */
export interface ClientSettingsPlatform {
  /** Whether closing the window can leave the app running in the background (the desktop app). */
  backgroundRun: boolean;
  /** Whether the app has a menu bar icon (a notification area icon on Windows) to show or hide (the desktop app). */
  menuBarIcon: boolean;
}

/** What `platform`'s capabilities (each present only where the platform has it) say this client's platform offers. */
export function clientSettingsPlatform(platform: { backgroundRun?: unknown; statusItem?: unknown }): ClientSettingsPlatform {
  return { backgroundRun: platform.backgroundRun !== undefined, menuBarIcon: platform.statusItem !== undefined };
}

const APPEARANCES: readonly ThemeChoice[] = ["light", "dark", "system"];

const THEME_LABEL: Record<ThemeChoice, PlainMessageKey> = {
  light: "settings.theme.light",
  dark: "settings.theme.dark",
  system: "settings.theme.system",
};

/** The values this client adds to a document it saves, for the Settings `sections` the document carries: for General
 * the session order (all three parts), whether it runs in the background and whether it shows its menu bar icon (each of
 * the last two only where the platform has that); for Appearance the theme, the Markdown style and the terminal font size. */
export function heldClientSettings(
  current: { appearance: ThemeChoice },
  platform: ClientSettingsPlatform,
  sections: readonly SettingsSection[],
): Omit<ClientSettings, "session_order"> & { session_order?: DocumentSessionOrder } {
  const order = sessionOrderPreference.get();
  return {
    ...(sections.includes("general") && {
      session_order: { sessions: order.sessions, active_projects: order.activeProjects, inactive_projects: order.inactiveProjects },
      ...(platform.backgroundRun && { run_in_background: runInBackground.get() }),
      ...(platform.menuBarIcon && { show_menu_bar_icon: showMenuBarIcon.get() }),
    }),
    ...(sections.includes("appearance") && {
      appearance: current.appearance,
      markdown_style: markdownStyle.get(),
      terminal_font_size: terminalFontSize.get(),
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
  const { appearance, markdown_style, terminal_font_size, run_in_background, show_menu_bar_icon, session_order } = settings as Record<string, unknown>;
  const order = sessionOrderOf(session_order);
  const pixelSize = pixelSizeOf(terminal_font_size);
  return {
    ...(APPEARANCES.includes(appearance as ThemeChoice) && { appearance: appearance as ThemeChoice }),
    ...(MARKDOWN_STYLES.includes(markdown_style as MarkdownStyle) && { markdown_style: markdown_style as MarkdownStyle }),
    ...(pixelSize !== undefined && { terminal_font_size: clampTerminalFontSize(pixelSize) }),
    ...(platform.backgroundRun && typeof run_in_background === "boolean" && { run_in_background }),
    ...(platform.menuBarIcon && typeof show_menu_bar_icon === "boolean" && { show_menu_bar_icon }),
    ...(Object.keys(order).length > 0 && { session_order: order }),
  };
}

/** The largest pixel size the daemon keeps, a `u32`. */
const MAX_PIXEL_SIZE = 4294967295;

/** The whole pixel size a document's value stands for, read as the daemon reads one (`lenient_pixel_size`): a number
 * that is not negative rounds to the nearest pixel, and anything else (text, a negative or a non-finite number, one the
 * daemon cannot hold) is no size, so the review never promises a change the daemon drops. */
function pixelSizeOf(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= MAX_PIXEL_SIZE ? Math.round(value) : undefined;
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
  if (values.terminal_font_size !== undefined) terminalFontSize.set(values.terminal_font_size);
  if (values.run_in_background !== undefined) runInBackground.set(values.run_in_background);
  if (values.show_menu_bar_icon !== undefined) showMenuBarIcon.set(values.show_menu_bar_icon);
  if (values.session_order !== undefined) sessionOrderPreference.set({ ...sessionOrderPreference.get(), ...values.session_order });
}

/** One client setting for the import cards: its settings-page name, and its value worded for it. `current`, the value
 * this client has now worded the same way (the language the daemon holds now, for the language), is present while
 * reviewing. `lang` and `currentLang` are the language of a value that is a name in a language of its own (a
 * language's name, a Markdown style's). */
export interface ClientSettingValue {
  field: ScalarClientField | LanguageField;
  label: PlainMessageKey;
  value: string;
  lang?: string;
  current?: string;
  currentLang?: string;
}

/** The client settings `wanted` carries, in the order of the settings pages, worded as their settings pages word them.
 * With `current` (this client's own values), each carries the value this client has now. */
export function clientSettingValues(
  t: Translate,
  wanted: ClientSettings,
  current?: ClientSettings,
): ClientSettingValue[] {
  // The language a value is named in, where it is one of its own: the Markdown style names are in English, as the
  // settings pages mark them.
  const onOff = (value: boolean) => t(value ? "configuration.import.value.on" : "configuration.import.value.off");
  const words: { [F in ScalarClientField]-?: [PlainMessageKey, (value: NonNullable<ClientSettings[F]>) => string, ((value: NonNullable<ClientSettings[F]>) => string)?] } = {
    appearance: ["settings.theme.label", (value) => t(THEME_LABEL[value])],
    markdown_style: ["settings.markdownStyle.label", (value) => MARKDOWN_STYLE_NAMES[value], () => "en"],
    terminal_font_size: ["settings.terminalFontSize.label", (value) => t("configuration.import.value.pixels", { size: value })],
    run_in_background: ["settings.background.label", onOff],
    show_menu_bar_icon: ["settings.menuBarIcon.label", onOff],
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
