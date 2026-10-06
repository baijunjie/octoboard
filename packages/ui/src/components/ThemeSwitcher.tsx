import { ToggleButton, ToggleButtonGroup } from "@heroui/react";
import React from "react";

import { useOctoboardTheme, type ThemeChoice } from "../theme";

const OPTIONS: { key: ThemeChoice; label: string }[] = [
  { key: "light", label: "Light" },
  { key: "dark", label: "Dark" },
  { key: "system", label: "System" },
];

/**
 * The Light / Dark / System switch, one row at the foot of the sidebar alongside
 * `NotificationsPrompt`. A `ToggleButtonGroup` in single-selection, no-empty-selection mode is
 * HeroUI's segmented control — the natural fit for a three-way choice where one option is always
 * current, rather than three independent `Button`s that would each need their own notion of
 * "currently selected".
 */
export function ThemeSwitcher(): React.ReactElement {
  const { choice, setChoice } = useOctoboardTheme();
  return (
    // The row's divider and padding belong to this wrapper, not to the group: a
    // `ToggleButtonGroup` sizes itself to its buttons, so a `border-t` on it stops where the
    // buttons do instead of spanning the sidebar, unlike the `NotificationsPrompt` and
    // `TrustedFolders` rows above it.
    <div className="shrink-0 border-t border-separator px-3 py-1">
      <ToggleButtonGroup
        aria-label="Theme"
        selectionMode="single"
        disallowEmptySelection
        selectedKeys={[choice]}
        onSelectionChange={(keys) => {
          const [key] = keys;
          if (key === "light" || key === "dark" || key === "system") setChoice(key);
        }}
      >
        {OPTIONS.map((option) => (
          <ToggleButton key={option.key} id={option.key} size="sm" preventFocusOnPress>
            {option.label}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </div>
  );
}
