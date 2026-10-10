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
 * while a Markdown file is on screen in a form that has both views. */
export function MarkdownViewToggle({
  view,
  onViewChange,
}: {
  view: MarkdownView;
  onViewChange: (view: MarkdownView) => void;
}): React.ReactElement {
  const t = useT();
  return (
    <SlotToggle
      slot="markdownView"
      label={t("viewer.markdown")}
      value={view}
      options={[
        { id: "document", label: t("viewer.markdown.document") },
        { id: "source", label: t("viewer.markdown.source") },
      ]}
      onChange={onViewChange}
    />
  );
}
