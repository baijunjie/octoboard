import { createPersistedPreference, storageKeyFor } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { format, translatorFor, type MessageArgs, type MessageKey, type Translate } from "./catalog";
import { isLanguage, type Language, textDirection } from "./languages";
import { matchLanguage } from "./matchLanguage";

/** The system's preferred languages, most preferred first: as the desktop shell hands them over in
 * `?languages=` (WKWebView's `navigator.languages` holds only the first preference), or the
 * browser's own. */
function systemLanguages(): readonly string[] {
  const fromShell = new URLSearchParams(window.location.search).get("languages")?.split(",").filter(Boolean);
  if (fromShell?.length) return fromShell;
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

/**
 * The UI's language. Once connected it is the one the window's daemon holds, which every client of
 * that daemon shares (`useInterfaceLanguage`); what is kept here is only a cache of it, so that the
 * start renders in that language before it has connected. The cache is kept per daemon, so a window
 * of another daemon, which may hold another language, neither changes nor is changed by the others;
 * a daemon's window with none of its own yet starts from the local one's. With no cache (the first
 * launch, or cleared storage) it is picked from the system's preferred languages. Without storage
 * nothing is kept, so it is picked from the system again on every load.
 */
const language = createPersistedPreference<Language>(
  PREFERENCE_KEYS.language,
  (raw) => (isLanguage(raw) ? raw : matchLanguage(systemLanguages())),
  (value) => value,
  { scope: "daemon", inherit: true },
);
language.persist();

/**
 * The language of the local daemon's windows, which the menu bar icon's menu is worded in whichever daemon a window
 * shows. In the local window it is the window's own language; in a remote daemon's window, whose own language is kept
 * under its daemon, it is the local cache under the bare key, which that window only reads and follows when a local
 * window changes it.
 */
const localLanguage =
  storageKeyFor(PREFERENCE_KEYS.language, "daemon") === PREFERENCE_KEYS.language
    ? language
    : createPersistedPreference<Language>(
      PREFERENCE_KEYS.language,
      (raw) => (isLanguage(raw) ? raw : matchLanguage(systemLanguages())),
      (value) => value,
    );

/** The language of the local daemon's windows, re-rendering the caller when it changes (see `localLanguage`). */
export const useLocalLanguage = localLanguage.useValue;

function applyToDocument(): void {
  document.documentElement.lang = language.get();
  document.documentElement.dir = textDirection(language.get());
}

applyToDocument();
// Subscribed before anything else can be, so the document already carries the new language when
// a later subscriber re-renders.
language.subscribe(applyToDocument);

/** Has a window with no daemon of its own (the tray flyout), which shows the local daemon's language, follow a
 * change another window of the application makes to it. */
export function followLocalLanguage(): void {
  language.followStorage?.();
}

/** The message in the local daemon's windows' language, for what is worded in it whichever daemon the window shows. */
export function localTranslator(language: Language): Translate {
  return translatorFor(language);
}

/** The language the UI renders right now. */
export function currentLanguage(): Language {
  return language.get();
}

/** Makes `next` the UI's language, and keeps it. */
export function setLanguage(next: Language): void {
  language.set(next);
}

export const subscribeLanguage = language.subscribe;

/** The message in the current language, for code outside React. A component uses `useT` instead,
 * so that it re-renders when the language changes. */
export function t<K extends MessageKey>(key: K, ...args: MessageArgs<K>): string {
  return format(language.get(), key, args[0]);
}
