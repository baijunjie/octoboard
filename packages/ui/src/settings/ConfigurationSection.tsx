import { Button } from "@heroui/react";
import React, { useState } from "react";

import { ExportConfigurationDialog } from "../configuration/ExportConfigurationDialog";
import { ImportConfigurationDialog } from "../configuration/ImportConfigurationDialog";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useCurrentConsole } from "../sidebar/sidebarView";
import { SettingRow } from "./SettingRow";

/** Moving configuration between computers: exporting the console the sidebar shows, or everything,
 * to a file, and importing a file exported elsewhere (see
 * `docs/product/configuration-export-and-import.md`). Each opens its own dialog, which saves or
 * opens the file through the platform; where the platform has no configuration-file capability
 * there is nothing to press and the section says so. */
export function ConfigurationSection(): React.ReactElement {
  const t = useT();
  const { configurationFile } = usePlatform();
  const inHand = useCurrentConsole();
  const [dialog, setDialog] = useState<"export" | "import">();

  if (!configurationFile) return <p className="text-sm text-muted">{t("settings.configuration.unavailable")}</p>;
  return (
    <>
      <SettingRow label={t("settings.configuration.export.label")} description={t("settings.configuration.export.description")}>
        <Button size="sm" variant="outline" preventFocusOnPress onPress={() => setDialog("export")}>
          {t("settings.configuration.export.button")}
        </Button>
      </SettingRow>
      <SettingRow label={t("settings.configuration.import.label")} description={t("settings.configuration.import.description")}>
        <Button size="sm" variant="outline" preventFocusOnPress onPress={() => setDialog("import")}>
          {t("settings.configuration.import.button")}
        </Button>
      </SettingRow>
      {dialog === "export" && <ExportConfigurationDialog console={inHand} onClose={() => setDialog(undefined)} />}
      {dialog === "import" && <ImportConfigurationDialog onClose={() => setDialog(undefined)} />}
    </>
  );
}
