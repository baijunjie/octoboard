import { ToggleButton, ToggleButtonGroup } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import { useOctoboardTheme } from "../theme";
import { SettingRow } from "./SettingRow";

const OPTIONS = [
  { key: "light", label: "settings.appearance.light" },
  { key: "dark", label: "settings.appearance.dark" },
  { key: "system", label: "settings.appearance.system" },
] as const;

/**
 * The Light / Dark / System choice. A `ToggleButtonGroup` in single-selection, no-empty-selection
 * mode is HeroUI's segmented control, the natural fit for a three-way choice where one option is
 * always current.
 */
export function AppearanceSetting(): React.ReactElement {
  const t = useT();
  const { choice, setChoice } = useOctoboardTheme();
  return (
    <SettingRow label={t("settings.appearance.label")} description={t("settings.appearance.description")}>
      <ToggleButtonGroup
        aria-label={t("settings.appearance.label")}
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
            {t(option.label)}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </SettingRow>
  );
}
