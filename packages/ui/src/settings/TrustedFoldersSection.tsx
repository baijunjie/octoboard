import { Button, EmptyState } from "@heroui/react";
import React from "react";

import { MarkedPath } from "../components/MarkedPath";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** The folders under which Octoboard presses every agent's trust confirmation for every project —
 * the parent-directory form of the one permission all agents share — with a way to stop each.
 * Stopping leaves each project's own permission, every running session and the trust each agent
 * has already recorded as they are (see "Trusted folders" in `docs/product/folder-trust.md`). */
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
              label={<MarkedPath path={path} />}
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
