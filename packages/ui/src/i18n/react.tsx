import React, { useCallback, useSyncExternalStore } from "react";
import { I18nProvider } from "react-aria-components";

import { InlineReference } from "../components/InlineReference";

import { format, globalParam, messageText, type MessageArgs, type MessageKey, splitPlaceholders, type Translate } from "./catalog";
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
 *
 * Whitespace that stands alone between two placeholders ("{a} {b}") is not rendered as a text node
 * of its own: VoiceOver drops a whitespace-only node and runs the two sides together. It is moved
 * to the front of the text that follows when that is a string or another message's text; before any
 * other element it stays a node of its own.
 */
export function Message<K extends MessageKey>({
  id,
  params,
  lead,
}: {
  id: K;
  /** Text to put before this message's own, set when a parent hands its separator down. */
  lead?: string;
} & (MessageArgs<K> extends []
  ? { params?: never }
  : { params: MessageArgs<K, React.ReactNode>[0] })): React.ReactElement {
  const language = useCurrentLanguage();
  const values = params as Record<string, React.ReactNode> | undefined;
  const parts = splitPlaceholders(messageText(language, id, values));
  const nodes: React.ReactNode[] = [];
  let carry = lead ?? "";
  parts.forEach((part, index) => {
    if (index % 2 === 0) {
      if (index > 0 && index < parts.length - 1 && part !== "" && part.trim() === "") carry += part;
      else if (part !== "") {
        nodes.push(carry + part);
        carry = "";
      }
      return;
    }
    nodes.push(<React.Fragment key={index}>{withLead(fill(values?.[part] ?? richGlobalParam(part, language), part, language), carry)}</React.Fragment>);
    carry = "";
  });
  if (carry) nodes.push(carry);
  return <>{nodes}</>;
}

/** `node` with `lead` in front of its first text, in the same text node when it can be reached: a string,
 * or a nested `Message`. Any other element keeps `lead` as a text node of its own. */
function withLead(node: React.ReactNode, lead: string): React.ReactNode {
  if (!lead) return node;
  if (typeof node === "string") return lead + node;
  if (React.isValidElement<{ lead?: string }>(node) && node.type === Message) {
    return React.cloneElement(node, { lead: lead + (node.props.lead ?? "") });
  }
  return <>{lead}{node}</>;
}

function richGlobalParam(name: string, language: Language): React.ReactNode {
  const value = globalParam(name, language);
  return value === undefined ? undefined : <InlineReference dir="auto" wrap>{value}</InlineReference>;
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
