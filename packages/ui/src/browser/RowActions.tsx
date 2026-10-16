import { Copy, FolderOpen, SquareArrowOutUpRight } from "lucide-react";
import React, { createContext, useContext } from "react";

import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { RowControls } from "../components/RowControls";
import { useCopyPath } from "../components/useCopyPath";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { useDaemon } from "../store";

/** The live directory behind the displayed rows. A branch comparison has no live directory. */
export const RowPathScope = createContext<{ project: string; worktree?: string } | undefined>(undefined);

/** The row's button and right-click share the same actions and focus handling. */
export function RowActions({ path, name, directory = false, contextTargetRef }: {
  path: string;
  name: string;
  directory?: boolean;
  contextTargetRef?: React.RefObject<HTMLElement | null>;
}): React.ReactElement | null {
  const t = useT();
  const copyPath = useCopyPath();
  const scope = useContext(RowPathScope);
  const { request, toastError } = useDaemon();
  const items: ActionMenuEntry[] = [];
  if (scope) {
    const run = (type: "open_project_path" | "reveal_project_path") => {
      void request({ type, ...scope, path }).catch((error: Error) => toastError(error));
    };
    items.push(
      { label: t("browser.row.open"), icon: SquareArrowOutUpRight, onClick: () => run("open_project_path") },
      { label: t("browser.row.reveal"), icon: FolderOpen, onClick: () => run("reveal_project_path") },
    );
  }
  if (copyPath) items.push({ label: t("common.copyPath"), icon: Copy, onClick: () => copyPath(path) });
  if (items.length === 0) return null;
  return (
    <RowControls className="ms-auto">
      <ActionMenu
        label={t(directory ? "browser.directory.actions" : "browser.row.actions", { name: isolate(name) })}
        contextTargetRef={contextTargetRef}
        items={items}
      />
    </RowControls>
  );
}
