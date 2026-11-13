import { Monitor, Moon, Sun } from "lucide-react";
import React from "react";

import { TitledControl } from "../components/hint/TitledControl";
import { useT } from "../i18n/react";
import { useOctoboardTheme } from "../preferences/theme";
import { ToggleButton, ToggleButtonGroup } from "../primitives";
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
          <TitledControl key={key} title={t(label)}>
            <ToggleButton id={key} size="sm" iconOnly label={t(label)} focusOnPress={false}>
              <Icon aria-hidden="true" className="size-4" />
            </ToggleButton>
          </TitledControl>
        ))}
      </ToggleButtonGroup>
    </SettingRow>
  );
}
