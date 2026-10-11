import React from "react";

import { useT } from "../i18n/react";
import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { SlotToggle } from "./slotToggle";

export type DiffLayout = "unified" | "split";

/** The layout diffs open in: the user's last choice in the viewer's toggle, kept across files and
 * restarts. Unified until they choose. */
export const diffLayout = createPersistedPreference<DiffLayout>(
  PREFERENCE_KEYS.diffLayout,
  (raw) => (raw === "split" ? "split" : "unified"),
  (value) => value,
);

/** The choice between a unified and a split diff. It is rendered by whatever draws the diff, so it
 * exists while a diff with a layout to choose does (a patch shown as plain text has none, and a
 * diff's goes with its renderer when that fails), and is moved into the header's layout slot
 * (`ControlsSlot`). */
export function LayoutToggle({
  layout,
  onLayoutChange,
}: {
  layout: DiffLayout;
  onLayoutChange: (layout: DiffLayout) => void;
}): React.ReactElement {
  const t = useT();
  return (
    <SlotToggle
      slot="layout"
      label={t("viewer.layout")}
      value={layout}
      options={[
        { id: "unified", label: t("viewer.layout.unified") },
        { id: "split", label: t("viewer.layout.split") },
      ]}
      onChange={onLayoutChange}
    />
  );
}
