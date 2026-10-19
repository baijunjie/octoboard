import { Copy, FolderOpen, SquareArrowOutUpRight } from "lucide-react";
import React, { createContext, useContext } from "react";

import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { RowControls } from "../components/RowControls";
import { useCopyPath } from "../components/useCopyPath";
import { useHostPathActions } from "../components/useHostPathActions";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";

/** The live directory behind the displayed rows: the project's, or the worktree a branch is checked
 * out in. A committed change has none. */
export const RowPathScope = createContext<{ project: string; worktree?: string } | undefined>(undefined);

/** The row's button and right-click share the same actions and focus handling. */
export function RowActions({ path, name, directory = false, reserve = false, contextTargetRef }: {
  path: string;
  name: string;
  directory?: boolean;
  /** Keep the button's room while it is hidden, for a row that has content laid out before it. */
  reserve?: boolean;
  contextTargetRef?: React.RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const copyPath = useCopyPath();
  const scope = useContext(RowPathScope);
  const hostAction = useHostPathActions();
  const items: ActionMenuEntry[] = [{ label: t("common.copyPath"), icon: Copy, onClick: () => copyPath(path) }];
  if (scope) {
    const run = (type: "open_project_path" | "reveal_project_path") => hostAction(type, { ...scope, path });
    items.push(
      {
        label: t("common.openIn"), icon: SquareArrowOutUpRight,
        items: [
          { label: t("common.fileManager"), icon: FolderOpen, onClick: () => run("reveal_project_path") },
          { label: t("common.defaultApplication"), icon: SquareArrowOutUpRight, onClick: () => run("open_project_path") },
        ],
      },
    );
  }
  return (
    <RowControls reserve={reserve} className="ms-auto">
      <ActionMenu
        label={t(directory ? "browser.directory.actions" : "browser.row.actions", { name: isolate(name) })}
        contextTargetRef={contextTargetRef}
        items={items}
      />
    </RowControls>
  );
}
