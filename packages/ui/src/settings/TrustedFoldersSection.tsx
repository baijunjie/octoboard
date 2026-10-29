import { FolderOpen } from "lucide-react";
import React, { useState } from "react";

import { Button } from "../components/Button";
import { EmptyPanel } from "../components/EmptyPanel";
import { InlineReference } from "../components/InlineReference";
import { MarkedPath } from "../components/MarkedPath";
import { useListMotion } from "../components/useListMotion";
import { TitledControl } from "../components/TitledControl";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
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
      <p className="pb-2 text-sm text-app-muted">
        <Message id="settings.trusted.description" />
      </p>
      {directories.length === 0 ? (
        <div className="groove-t py-2">
          {/* `-ms-2` cancels the `ps-2` that `EmptyPanel`'s compact row carries for the sidebar rows
              it was built for, which here would stand its glyph 8px in from the section's own
              content edge. Change the two together. */}
          <div className="-ms-2">
            <EmptyPanel compact icon={FolderOpen} message={t("settings.trusted.empty")} />
          </div>
        </div>
      ) : (
        <div ref={listRef} className="relative groove-t">
          {directories.map((path) => (
            <SettingRow key={path} motionKey={path} label={<MarkedPath path={path} />}>
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
      {removing && (
        <ConfirmDialog
          title={t("settings.trusted.removeTitle")}
          errorTitle={t("settings.trusted.removeFailedTitle")}
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
