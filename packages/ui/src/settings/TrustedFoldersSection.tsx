import { Button, EmptyState } from "@heroui/react";
import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** The folders under which Octoboard answers Claude Code's trust prompt for every project, with a
 * way to stop each. Stopping leaves each project's own consent and every running session as it is
 * (see "Trusted folders" in `docs/product/launching-agents.md`). */
export function TrustedFoldersSection(): React.ReactElement {
  const { request, toastError } = useDaemon();
  const directories = useDaemonStore((s) => s.trustedDirectories);
  useSectionRefocus([directories]);
  const remove = (path: string) =>
    void request({ type: "remove_trusted_directory", path }).catch((err) => toastError((err as Error).message));
  return (
    <>
      <p className="pb-2 text-sm text-muted">
        Octoboard answers Claude Code's trust prompt without asking for every project under these folders, including
        projects added to them later. Claude Code then applies the permission rules and hooks in each project's own
        settings without asking either, so trust a folder only if you trust everything that ends up inside it.
      </p>
      {directories.length === 0 ? (
        <EmptyState className="border-t border-separator px-0 py-4">
          No folders are trusted. A folder is trusted from a session's trust prompt.
        </EmptyState>
      ) : (
        <div className="border-t border-separator">
          {directories.map((path) => (
            <SettingRow
              key={path}
              label={
                // Cut from the start when too long, so the folder's own name stays visible.
                <FadeOverflow clip="start" titleWhenClipped={path}>
                  {path}
                </FadeOverflow>
              }
              description="Projects under this folder are trusted."
            >
              <TitledControl title="Stop trusting this folder. Projects' own consents and running sessions are kept.">
                <Button
                  size="sm"
                  variant="outline"
                  preventFocusOnPress
                  aria-label={`Stop trusting ${path}`}
                  onPress={() => remove(path)}
                >
                  Remove
                </Button>
              </TitledControl>
            </SettingRow>
          ))}
        </div>
      )}
    </>
  );
}
