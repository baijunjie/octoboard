import { ToggleButton, ToggleButtonGroup } from "@heroui/react";
import React, { useContext } from "react";
import { createPortal } from "react-dom";

import { useT } from "../i18n/react";
import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ControlsSlot } from "./controlsSlot";

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
 * single diff's goes with its renderer when that fails), and is moved into the header's layout
 * slot (`ControlsSlot`). The exception is a change in sections, whose one toggle `ChangeView`
 * draws outside the renderer boundaries, so it stays even if every section's renderer fails. */
export function LayoutToggle({
  layout,
  onLayoutChange,
}: {
  layout: DiffLayout;
  onLayoutChange: (layout: DiffLayout) => void;
}): React.ReactElement | null {
  const t = useT();
  const slot = useContext(ControlsSlot)?.layout;
  if (!slot) return null;
  return createPortal(
    <ToggleButtonGroup
      aria-label={t("viewer.layout")}
      size="sm"
      selectionMode="single"
      disallowEmptySelection
      selectedKeys={[layout]}
      onSelectionChange={(keys) => {
        const [picked] = [...keys];
        if (picked === "unified" || picked === "split") onLayoutChange(picked);
      }}
    >
      <ToggleButton id="unified">{t("viewer.layout.unified")}</ToggleButton>
      <ToggleButton id="split">{t("viewer.layout.split")}</ToggleButton>
    </ToggleButtonGroup>,
    slot,
  );
}
