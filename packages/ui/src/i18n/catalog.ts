import { en } from "./messages/en";
import { zhHans } from "./messages/zh-Hans";
import { FALLBACK_LANGUAGE, type Language } from "./languages";

/** The CLDR plural categories; which of them a language needs is what `Intl.PluralRules` reports. */
export type PluralCategory = "zero" | "one" | "two" | "few" | "many" | "other";

/** A message that depends on a count: one text per plural category, selected by the `count`
 * parameter. `other` is the one category every language has. */
export type PluralMessage = { readonly [C in PluralCategory]?: string } & { readonly other: string };

/** A catalog entry. `{name}` in the text is a placeholder filled from the parameter of that name. */
export type Message = string | PluralMessage;

export type MessageKey = keyof typeof en;

/** Whether the catalog has a message under `key`, for a key made at run time (a daemon code). */
export function isMessageKey(key: string): key is MessageKey {
  return Object.hasOwn(en, key);
}

type Placeholders<S extends string> = S extends `${string}{${infer P}}${infer Rest}` ? P | Placeholders<Rest> : never;

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
  en,
  "zh-Hans": zhHans,
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

/** The message in `language`, its placeholders filled; a number is formatted for the language. */
export function format(language: Language, key: MessageKey, params?: Record<string, string | number>): string {
  const parts = splitPlaceholders(messageText(language, key, params));
  return parts
    .map((part, index) => {
      if (index % 2 === 0) return part;
      const value = params?.[part];
      if (value === undefined) return `{${part}}`;
      return typeof value === "number" ? value.toLocaleString(language) : value;
    })
    .join("");
}
