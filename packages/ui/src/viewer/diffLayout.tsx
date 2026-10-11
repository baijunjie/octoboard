import { Columns2, Rows2 } from "lucide-react";
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
 * (`ControlsSlot`). Split is the pressed state. */
export function LayoutToggle({
  layout,
  onLayoutChange,
}: {
  layout: DiffLayout;
  onLayoutChange: (layout: DiffLayout) => void;
}): React.ReactElement {
  const t = useT();
  const split = layout === "split";
  return (
    <SlotToggle
      slot="layout"
      label={t(split ? "viewer.layout.unified" : "viewer.layout.split")}
      isSelected={split}
      onChange={(selected) => onLayoutChange(selected ? "split" : "unified")}
    >
      {split ? <Columns2 aria-hidden="true" className="size-4" /> : <Rows2 aria-hidden="true" className="size-4" />}
    </SlotToggle>
  );
}
