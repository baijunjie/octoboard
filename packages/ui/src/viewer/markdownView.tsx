import { CodeXml, FileText } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { SlotToggle } from "./slotToggle";

export type MarkdownView = "document" | "source";

/** How a Markdown file opens: the user's last choice in the viewer's toggle, kept across files and
 * restarts. The document until they choose. */
export const markdownView = createPersistedPreference<MarkdownView>(
  PREFERENCE_KEYS.markdownView,
  (raw) => (raw === "source" ? "source" : "document"),
  (value) => value,
);

/** The choice between a Markdown file's source and its rendered document, moved into the header's
 * Markdown view slot (`ControlsSlot`). It is drawn by whatever draws the file, so it exists only
 * while a Markdown file is on screen in a form that has both views. Source is the pressed state. */
export function MarkdownViewToggle({
  view,
  onViewChange,
}: {
  view: MarkdownView;
  onViewChange: (view: MarkdownView) => void;
}): React.ReactElement {
  const t = useT();
  const source = view === "source";
  return (
    <SlotToggle
      slot="markdownView"
      label={t(source ? "viewer.markdown.document" : "viewer.markdown.source")}
      isSelected={source}
      onChange={(selected) => onViewChange(selected ? "source" : "document")}
    >
      {source ? <CodeXml aria-hidden="true" className="size-4" /> : <FileText aria-hidden="true" className="size-4" />}
    </SlotToggle>
  );
}
