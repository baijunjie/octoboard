import { ToggleButton } from "@heroui/react";
import React, { useCallback, useContext, useEffect, useState } from "react";

import { useT } from "../i18n/react";
import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ControlsSlot } from "./controlsSlot";

/** Whether long lines wrap, in files and diffs alike: the user's last choice in the viewer's
 * toggle, kept across files and restarts. Unwrapped until they choose, whatever the file is. */
export const wordWrap = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.wordWrap,
  (raw) => raw === "true",
  (value) => String(value),
);

/** Keeps the header's wrap choice for as long as code that can wrap is on screen; it draws nothing
 * itself. Whatever draws code renders one, and however many are mounted (a change in sections, an
 * image change whose sides both fell back to text) the header still shows a single choice, which
 * stays until the last of them goes. */
export function WrapToggle(): null {
  const claimWrap = useContext(ControlsSlot)?.claimWrap;
  useEffect(() => claimWrap?.(), [claimWrap]);
  return null;
}

/** The claims on the header's wrap choice, for the viewer that owns the header: `claimWrap` for the
 * controls context and whether any is held. */
export function useWrapClaims(): { claimWrap: () => () => void; claimed: boolean } {
  const [claims, setClaims] = useState(0);
  const claimWrap = useCallback(() => {
    setClaims((n) => n + 1);
    return () => setClaims((n) => n - 1);
  }, []);
  return { claimWrap, claimed: claims > 0 };
}

/** The header's wrap choice itself, drawn while some `WrapToggle` is mounted. */
export function WrapToggleHost(): React.ReactElement {
  const t = useT();
  const wrap = wordWrap.useValue();
  return (
    <ToggleButton size="sm" isSelected={wrap} onChange={(selected) => wordWrap.set(selected)}>
      {t("viewer.wrap")}
    </ToggleButton>
  );
}
