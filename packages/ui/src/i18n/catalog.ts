import { appConfig } from "../appConfig";
import { PROVIDER_PARAMS } from "../providers/registry";
import { FALLBACK_LANGUAGE, textDirection, type Language } from "./languages";
import { isolate } from "./isolate";
import { type DaemonHost, osName, platformParam, platformParamBase, PLATFORM_PARAM_NAMES } from "./platformTerms";
import { ar } from "./messages/ar";
import { de } from "./messages/de";
import { en } from "./messages/en";
import { es } from "./messages/es";
import { fr } from "./messages/fr";
import { hi } from "./messages/hi";
import { id } from "./messages/id";
import { it } from "./messages/it";
import { ja } from "./messages/ja";
import { ko } from "./messages/ko";
import { pt } from "./messages/pt";
import { ru } from "./messages/ru";
import { th } from "./messages/th";
import { tr } from "./messages/tr";
import { vi } from "./messages/vi";
import { zhHans } from "./messages/zh-Hans";
import { zhHant } from "./messages/zh-Hant";

/** The CLDR plural categories; which of them a language needs is what `Intl.PluralRules` reports. */
export type PluralCategory = "zero" | "one" | "two" | "few" | "many" | "other";

/** A message that depends on a count: one text per plural category, selected by the `count`
 * parameter. `other` is the one category every language has. */
export type PluralMessage = { readonly [C in PluralCategory]?: string } & { readonly other: string };

/** A catalog entry. `{name}` in the text is a placeholder filled from the parameter of that name,
 * except the global ones (`globalParam`), which every message can use and no caller passes. */
export type Message = string | PluralMessage;

export type MessageKey = keyof typeof en;

/** Whether the catalog has a message under `key`, for a key made at run time (a daemon code). */
export function isMessageKey(key: string): key is MessageKey {
  return Object.hasOwn(en, key);
}

/** The placeholders that are not a provider's: the app's own name (from the app config) and the products' names. */
const CORE_PARAMS = { appName: appConfig.name, git: "Git", tauri: "Tauri" } as const;

/** The placeholders filled in every message, so callers never pass them: the core ones and the providers' brand names
 * (`params` of their modules). */
const GLOBAL_PARAMS = { ...PROVIDER_PARAMS, ...CORE_PARAMS } as const;

/** `osName` and the platform words (`platformTerms.ts`) are global too, but come from the hosts announced: the shell's,
 * and for the words that name something on the daemon's machine, the daemon's. */
type GlobalParam =
  | keyof typeof GLOBAL_PARAMS
  | "osName"
  | "trash"
  | "systemSettings"
  | "hostSettings"
  | "settingsApp"
  | "menuBarIcon"
  | "developerMode";

/** The names of the placeholders that name a provider, which a message shows as references like `{agent}`. */
export const PROVIDER_PARAM_NAMES: readonly string[] = Object.keys(PROVIDER_PARAMS);

/** The names of the core placeholders, which no provider module may reuse: the products', the operating system's and
 * the platform words'. */
export const CORE_PARAM_NAMES: readonly string[] = [...Object.keys(CORE_PARAMS), "osName", ...PLATFORM_PARAM_NAMES];

/** The value of a global placeholder, or `undefined` when `name` is not one. The platform words are those of the hosts
 * announced (`announceHost`, `announceDaemonHost`), in `language`; a caller that rendered under a daemon host passes
 * it as `daemon`. */
export function globalParam(name: string, language: Language = FALLBACK_LANGUAGE, daemon?: DaemonHost): string | undefined {
  if (name === "osName") return osName();
  if (Object.hasOwn(GLOBAL_PARAMS, name)) return GLOBAL_PARAMS[name as keyof typeof GLOBAL_PARAMS];
  return platformParam(name, language, undefined, daemon);
}

/** The platform words that name a thing the user looks for on screen: a settings app (the client's or the daemon's) and
 * a setting in it. */
const REFERENCE_WORDS: ReadonlySet<string> = new Set(["systemSettings", "hostSettings", "settingsApp", "developerMode"]);

/** Whether a message shows the placeholder as a reference to a product, which rich text sets as a chip: a name of the
 * products, the system's, or one of `REFERENCE_WORDS`, in whichever form or capital a sentence asks for it. The other
 * platform words are plain words of the sentence. */
export function isReferenceParam(name: string): boolean {
  const word = platformParamBase(name);
  return Object.hasOwn(GLOBAL_PARAMS, name) || name === "osName" || (word !== undefined && REFERENCE_WORDS.has(word));
}

type Placeholders<S extends string> = S extends `${string}{${infer P}}${infer Rest}` ? Exclude<P, GlobalParam> | Placeholders<Rest> : never;

type PlaceholdersOf<M> = M extends string
  ? Placeholders<M>
  : M extends PluralMessage
    ? Placeholders<Extract<M[keyof M], string>> | "count"
    : never;

/** The parameters a message takes, read off its English text; `count` is the number that selects a
 * plural message's form. */
export type MessageParams<K extends MessageKey, V = string | number> = {
  [P in PlaceholdersOf<(typeof en)[K]>]: P extends "count" ? number : V;
};

/** The keys of the messages that take no parameters, for a module-level table that names a message
 * to look up later. */
export type PlainMessageKey = {
  [K in MessageKey]: [PlaceholdersOf<(typeof en)[K]>] extends [never] ? K : never;
}[MessageKey];

/** The arguments after the key: none for a message without placeholders, otherwise its parameters. */
export type MessageArgs<K extends MessageKey, V = string | number> =
  [PlaceholdersOf<(typeof en)[K]>] extends [never] ? [] : [MessageParams<K, V>];

/** The message lookup: the key, then the parameters its message takes, if any. */
export type Translate = <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => string;

/** A complete translation: every message, and each plural message with exactly the categories `C`
 * the language needs (`other` always included). */
export type Translation<C extends PluralCategory> = {
  readonly [K in MessageKey]: (typeof en)[K] extends string ? string : { readonly [P in C | "other"]: string };
};

/** The translated catalogs. A language that is not here, or a message a catalog lacks, renders in
 * English. A catalog is typed `Translation<...>` with the plural categories its language needs, so
 * a missing message or category fails type checking; `catalog.test.ts` checks the rest. */
export const CATALOGS: Partial<Record<Language, Partial<Record<MessageKey, Message>>>> = {
  "ar": ar,
  "de": de,
  en,
  "es": es,
  "fr": fr,
  "hi": hi,
  "id": id,
  "it": it,
  "ja": ja,
  "ko": ko,
  "pt": pt,
  "ru": ru,
  "th": th,
  "tr": tr,
  "vi": vi,
  "zh-Hans": zhHans,
  "zh-Hant": zhHant,
};

const pluralRules = new Map<Language, Intl.PluralRules>();

function selectPlural(language: Language, message: PluralMessage, count: number): string {
  let rules = pluralRules.get(language);
  if (!rules) {
    rules = new Intl.PluralRules(language);
    pluralRules.set(language, rules);
  }
  return message[rules.select(count)] ?? message.other;
}

/** The message's text in `language` with its plural form chosen, placeholders still in place. */
export function messageText(language: Language, key: MessageKey, params?: Record<string, unknown>): string {
  const translated = CATALOGS[language]?.[key];
  const resolvedLanguage = translated === undefined ? FALLBACK_LANGUAGE : language;
  const message: Message = translated ?? en[key];
  if (typeof message === "string") return message;
  const count = params?.count;
  return selectPlural(resolvedLanguage, message, typeof count === "number" ? count : Number.NaN);
}

/** Splits a message's text into literal runs and placeholder names, alternating, starting and
 * ending with a literal run (possibly empty). */
export function splitPlaceholders(text: string): string[] {
  return text.split(/\{(\w+)\}/);
}

/** The message in `language`, its placeholders (global ones included, the daemon's words from `daemon` when given)
 * filled; a number is formatted for the language. */
export function format(language: Language, key: MessageKey, params?: Record<string, string | number>, daemon?: DaemonHost): string {
  const parts = splitPlaceholders(messageText(language, key, params));
  return parts
    .map((part, index) => {
      if (index % 2 === 0) return part;
      const value = params?.[part] ?? globalParam(part, language, daemon);
      if (value === undefined) return `{${part}}`;
      const text = typeof value === "number" ? value.toLocaleString(language) : value;
      // Global brand names also enter plain native labels, without a caller to isolate them.
      return textDirection(language) === "rtl" && (Object.hasOwn(GLOBAL_PARAMS, part) || part === "osName") ? isolate(text) : text;
    })
    .join("");
}

/** The message lookup in `language`, for code that words something in a language it is given rather than the current
 * one; with `daemon`, the daemon's words are that host's rather than the one announced when a message is read. */
export function translatorFor(language: Language, daemon?: DaemonHost): Translate {
  return (key, ...args) => format(language, key, args[0], daemon);
}
