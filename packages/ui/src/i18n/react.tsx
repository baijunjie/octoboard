import React, { useCallback, useSyncExternalStore } from "react";
import { I18nProvider } from "react-aria-components";

import { format, messageText, type MessageArgs, type MessageKey, splitPlaceholders, type Translate } from "./catalog";
import { currentLanguage, subscribeLanguage } from "./language";
import type { Language } from "./languages";

/** The language the UI renders, re-rendering the caller when it changes. */
export function useCurrentLanguage(): Language {
  return useSyncExternalStore(subscribeLanguage, currentLanguage);
}

/** The message lookup bound to the current language; the caller re-renders when it changes. */
export function useT(): Translate {
  const language = useCurrentLanguage();
  return useCallback<Translate>((key, ...args) => format(language, key, args[0]), [language]);
}

/**
 * A message whose placeholders are filled with React nodes, for text that carries markup (a
 * `<code>` span, a bold name) in the middle of a sentence, so the translation decides where the
 * markup goes.
 */
export function Message<K extends MessageKey>({
  id,
  params,
}: { id: K } & (MessageArgs<K> extends []
  ? { params?: never }
  : { params: MessageArgs<K, React.ReactNode>[0] })): React.ReactElement {
  const language = useCurrentLanguage();
  const values = params as Record<string, React.ReactNode> | undefined;
  const parts = splitPlaceholders(messageText(language, id, values));
  return (
    <>
      {parts.map((part, index) =>
        index % 2 === 0 ? part : <React.Fragment key={index}>{fill(values?.[part], part, language)}</React.Fragment>,
      )}
    </>
  );
}

function fill(value: React.ReactNode, name: string, language: Language): React.ReactNode {
  if (typeof value === "number") return value.toLocaleString(language);
  return value ?? `{${name}}`;
}

/** Hands the current language to react-aria and HeroUI, for their own built-in strings and
 * formatting. */
export function LanguageProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const language = useCurrentLanguage();
  return <I18nProvider locale={language}>{children}</I18nProvider>;
}
