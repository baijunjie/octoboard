import { FolderOpen } from "lucide-react";
import React, { useState } from "react";

import { Button } from "../components/controls/Button";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { InlineReference } from "../i18n/InlineReference";
import { MarkedPath } from "../components/text/MarkedPath";
import { useListMotion } from "../components/motion/useListMotion";
import { TitledControl } from "../components/hint/TitledControl";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { appDataRecovery } from "../errors/errorRecovery";
import { abbreviateHome } from "../format/pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
import { LeadRows, SettingsLead } from "./SettingsLead";
import { useSectionRefocus } from "./useSectionRefocus";

/** The folders under which Octoboard presses every agent's trust confirmation for every project —
 * the parent-directory form of the one permission all agents share — with a way to stop each.
 * Stopping asks first, and leaves each project's own permission, every running session and the trust
 * each agent has already recorded as they are (see "Trusted folders" in `docs/product/folder-trust.md`). */
export function TrustedFoldersSection(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const listRef = useListMotion<HTMLDivElement>();
  const directories = useDaemonStore((s) => s.trustedDirectories);
  const home = useDaemonStore((s) => s.homeDir);
  const [removing, setRemoving] = useState<string>();
  // Bumped when a removal has gone through: the Remove button that was pressed goes with its row and
  // the confirmation closes with it, so focus has to be sent back to the section's tab. Cancelling
  // removes no control, so it must not.
  const [removedCount, setRemovedCount] = useState(0);
  useSectionRefocus([directories, removedCount]);
  return (
    <>
      <SettingsLead>
        <Message id="settings.trusted.description" />
      </SettingsLead>
      <LeadRows>
        {directories.length === 0 ? (
          // `-ms-2` cancels the `ps-2` that `EmptyPanel`'s compact row carries for the sidebar rows it was built
          // for, which here would stand its glyph 8px in from the section's own content edge. Change the two together.
          <div className="-ms-2 py-2">
            <EmptyPanel compact icon={FolderOpen} message={t("settings.trusted.empty")} />
          </div>
        ) : (
          <div ref={listRef} className="relative">
            {directories.map((path) => (
              <SettingRow compact key={path} motionKey={path} label={<MarkedPath path={path} />}>
                <TitledControl title={t("settings.trusted.removeTooltip")}>
                  <Button
                    size="sm"
                    variant="danger-soft"
                    preventFocusOnPress
                    // The tooltip nearly repeats the accessible name; an empty description keeps it from being read twice.
                    aria-describedby=""
                    aria-label={t("settings.trusted.removeLabel", { path: isolate(path) })}
                    onPress={() => setRemoving(path)}
                  >
                    {t("common.remove")}
                  </Button>
                </TitledControl>
              </SettingRow>
            ))}
          </div>
        )}
      </LeadRows>
      {removing && (
        <ConfirmDialog
          title={t("settings.trusted.removeTitle")}
          errorTitle={t("settings.trusted.removeFailedTitle")}
          errorRecovery={appDataRecovery}
          subject={<Message id="settings.trusted.removeSubject" params={{ path: <InlineReference monospace wrap>{abbreviateHome(removing, home)}</InlineReference> }} />}
          message={t("settings.trusted.removeMessage")}
          confirmLabel={t("common.remove")}
          destructive
          resetKey={removing}
          onConfirm={async () => {
            await request({ type: "remove_trusted_directory", path: removing });
            setRemoving(undefined);
            setRemovedCount((count) => count + 1);
          }}
          onCancel={() => setRemoving(undefined)}
        />
      )}
    </>
  );
}
