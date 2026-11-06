import React, { useState } from "react";

import { ExportConfigurationDialog } from "../configuration/ExportConfigurationDialog";
import { openDialogFromAnywhere } from "../dialogs/dialogLauncher";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useCurrentConsole } from "../store";
import { SettingRow } from "./SettingRow";
import { Button } from "../primitives";

/** Moving configuration between computers: exporting consoles and projects, the application settings
 * or the account structure to a file, and importing a file exported elsewhere (see
 * `docs/product/configuration-export-and-import-dialogs.md`). Export opens its dialog from here;
 * Import asks the window to open its own (see `dialogs/dialogLauncher.ts`), so an import under way outlives Settings.
 * Both save or open the file through the platform; where the platform has no configuration-file capability there is
 * nothing to press and the section says so. */
export function ConfigurationSection(): React.ReactElement {
  const t = useT();
  const { configurationFile } = usePlatform();
  const inHand = useCurrentConsole();
  const [exporting, setExporting] = useState(false);

  if (!configurationFile) return <p className="text-sm text-app-muted">{t("settings.configuration.unavailable")}</p>;
  return (
    <>
      <SettingRow compact label={t("settings.configuration.export.label")} description={t("settings.configuration.export.description")}>
        <Button size="sm" variant="outline" focusOnPress={false} onPress={() => setExporting(true)}>
          {t("settings.configuration.export.button")}
        </Button>
      </SettingRow>
      <SettingRow compact label={t("settings.configuration.import.label")} description={t("settings.configuration.import.description")}>
        {/* Unlike Export, it takes focus when pressed: the window's dialog returns focus to it when it closes. */}
        <Button size="sm" variant="outline" onPress={() => openDialogFromAnywhere({ kind: "import-configuration" })}>
          {t("settings.configuration.import.button")}
        </Button>
      </SettingRow>
      {exporting && <ExportConfigurationDialog console={inHand} onClose={() => setExporting(false)} />}
    </>
  );
}
