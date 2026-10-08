import { Button, EmptyState } from "@heroui/react";
import React from "react";

import { PathText } from "../components/PathText";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** The folders under which Octoboard answers Claude Code's trust prompt for every project, with a
 * way to stop each. Stopping leaves each project's own consent and every running session as it is
 * (see "Trusted folders" in `docs/product/launching-agents.md`). */
export function TrustedFoldersSection(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const directories = useDaemonStore((s) => s.trustedDirectories);
  useSectionRefocus([directories]);
  const remove = (path: string) =>
    void request({ type: "remove_trusted_directory", path }).catch((err) => toastError((err as Error).message));
  return (
    <>
      <p className="pb-2 text-sm text-muted">
        {t("settings.trusted.description")}
      </p>
      {directories.length === 0 ? (
        <EmptyState className="border-t border-separator px-0 py-4">
          {t("settings.trusted.empty")}
        </EmptyState>
      ) : (
        <div className="border-t border-separator">
          {directories.map((path) => (
            <SettingRow
              key={path}
              label={<PathText path={path} />}
              description={t("settings.trusted.rowDescription")}
            >
              <TitledControl title={t("settings.trusted.removeTooltip")}>
                <Button
                  size="sm"
                  variant="outline"
                  preventFocusOnPress
                  aria-label={t("settings.trusted.removeLabel", { path })}
                  onPress={() => remove(path)}
                >
                  {t("common.remove")}
                </Button>
              </TitledControl>
            </SettingRow>
          ))}
        </div>
      )}
    </>
  );
}
