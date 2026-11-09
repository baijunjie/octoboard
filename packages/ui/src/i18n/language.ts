import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { format, type MessageArgs, type MessageKey } from "./catalog";
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
 * The UI's language. Once connected it is the one the daemon holds, which every client shares
 * (`useInterfaceLanguage`); what is kept here is only a cache of it, so that the start renders in
 * that language before it has connected. With no cache (the first launch, or cleared storage) it is
 * picked from the system's preferred languages. Without storage nothing is kept, so it is picked
 * from the system again on every load.
 */
const language = createPersistedPreference<Language>(
  PREFERENCE_KEYS.language,
  (raw) => (isLanguage(raw) ? raw : matchLanguage(systemLanguages())),
  (value) => value,
);
language.persist();

function applyToDocument(): void {
  document.documentElement.lang = language.get();
  document.documentElement.dir = textDirection(language.get());
}

applyToDocument();
// Subscribed before anything else can be, so the document already carries the new language when
// a later subscriber re-renders.
language.subscribe(applyToDocument);

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
