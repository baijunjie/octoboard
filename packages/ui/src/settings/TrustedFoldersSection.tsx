import { Button } from "@heroui/react";
import { FolderOpen } from "lucide-react";
import React from "react";

import { EmptyPanel } from "../components/EmptyPanel";
import { MarkedPath } from "../components/MarkedPath";
import { TitledControl } from "../components/TitledControl";
import { isolate } from "../i18n/isolate";
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
        <div className="border-t border-separator py-2">
          {/* `-ms-2` cancels the `ps-2` that `EmptyPanel`'s compact row carries for the sidebar rows
              it was built for, which here would stand its glyph 8px in from the section's own
              content edge. Change the two together. */}
          <div className="-ms-2">
            <EmptyPanel compact icon={FolderOpen} message={t("settings.trusted.empty")} />
          </div>
        </div>
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
                  aria-label={t("settings.trusted.removeLabel", { path: isolate(path) })}
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
