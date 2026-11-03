import React, { useId } from "react";

import { joinNodes } from "../../i18n/joinPhrases";
import { useCurrentLanguage } from "../../i18n/react";

/** The sentence `lead` that introduces agents named in a message, and under it the agents as a bulleted list, one to
 * an item, named by `lead`. Each name is isolated, so one written right to left keeps its own order without moving the
 * bullet. Each item but the last ends with the language's list separator for screen readers alone, so a name of
 * several words is not run into the next when the list is read as part of a sentence (an alert's description). It is
 * phrasing content throughout (spans with the list roles), so it can stand wherever the message part it
 * belongs to may, a paragraph or an Alert's description included. With no agents it is the sentence alone. */
export function AgentNameList({ lead, labels }: { lead: React.ReactNode; labels: string[] }): React.ReactElement {
  const id = useId();
  const language = useCurrentLanguage();
  // The language's own list, its names to items and each joining text kept, hidden, with the name before it.
  const items: { label: string; separator: string }[] = [];
  for (const part of joinNodes(language, labels, (label) => ({ label }))) {
    if (typeof part === "string") items[items.length - 1].separator += part;
    else items.push({ label: part.label, separator: "" });
  }
  return (
    <span className="flex min-w-0 flex-col gap-1">
      <span id={id}>{lead}</span>
      {labels.length > 0 && (
        <span role="list" aria-labelledby={id} className="block list-disc ps-5">
          {items.map((item, at) => (
            <span key={at} role="listitem" className="list-item">
              <bdi>{item.label}</bdi>
              {item.separator && <span className="sr-only">{item.separator}</span>}
            </span>
          ))}
        </span>
      )}
    </span>
  );
}
