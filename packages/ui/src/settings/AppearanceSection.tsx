import { ToggleButton, ToggleButtonGroup } from "@heroui/react";
import React from "react";

import { useOctoboardTheme } from "../theme";
import { SettingRow } from "./SettingRow";

const OPTIONS = [
  { key: "light", label: "Light" },
  { key: "dark", label: "Dark" },
  { key: "system", label: "System" },
] as const;

/**
 * The Light / Dark / System choice. A `ToggleButtonGroup` in single-selection, no-empty-selection
 * mode is HeroUI's segmented control, the natural fit for a three-way choice where one option is
 * always current.
 */
export function AppearanceSection(): React.ReactElement {
  const { choice, setChoice } = useOctoboardTheme();
  return (
    <SettingRow label="Appearance" description="Light, dark, or follow the operating system and switch with it.">
      <ToggleButtonGroup
        aria-label="Appearance"
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
    </SettingRow>
  );
}
