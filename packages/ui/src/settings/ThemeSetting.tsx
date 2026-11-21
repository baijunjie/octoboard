import { Monitor, Moon, Sun } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { useOctoboardTheme } from "../preferences/theme";
import { ToggleButton, ToggleButtonGroup, Tooltip } from "../primitives";
import { SettingRow } from "./SettingRow";

const OPTIONS = [
  { key: "light", label: "settings.theme.light", Icon: Sun },
  { key: "dark", label: "settings.theme.dark", Icon: Moon },
  { key: "system", label: "settings.theme.system", Icon: Monitor },
] as const;

/**
 * The Light / Dark / System choice. A `ToggleButtonGroup` in single-selection, no-empty-selection
 * mode is HeroUI's segmented control, the natural fit for a three-way choice where one option is
 * always current. Each choice is shown by its icon, named and given a tooltip by its label.
 *
 * A shortfall kept on purpose, by the user's decision: the selected button differs from the others by its fill colour
 * alone. Which one is selected is also carried by its position and by `aria-pressed`, and the fill difference meets
 * WCAG 1.4.11's 3:1 for a state (3.67:1 light, 4.58:1 dark, from the tokens). A two-state choice does not belong in
 * such a group: an `IconToggle` carries its state in its icon, with no shortfall to accept.
 */
export function ThemeSetting(): React.ReactElement {
  const t = useT();
  const { choice, setChoice } = useOctoboardTheme();
  return (
    <SettingRow searchId="theme" label={t("settings.theme.label")} description={t("settings.theme.description")}>
      <ToggleButtonGroup
        label={t("settings.theme.label")}
        selectedKey={choice}
        onSelectionChange={(key) => {
          if (key === "light" || key === "dark" || key === "system") setChoice(key);
        }}
      >
        {OPTIONS.map(({ key, label, Icon }) => (
          <Tooltip key={key} content={t(label)}>
            <ToggleButton id={key} size="sm" iconOnly label={t(label)} focusOnPress={false}>
              <Icon className="size-4" />
            </ToggleButton>
          </Tooltip>
        ))}
      </ToggleButtonGroup>
    </SettingRow>
  );
}
