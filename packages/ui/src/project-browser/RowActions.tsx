import { Copy, FolderOpen, MessageSquarePlus, SquareArrowOutUpRight, SquareTerminal, TextSearch } from "lucide-react";
import React, { createContext, useContext } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { useQuoteTarget } from "../components/host/quoteTarget";
import { ActionMenu } from "../components/menus/ActionMenu";
import { ActionableError } from "../errors/actionableError";
import { displayWirePath } from "../format/wirePath";
import type { ActionMenuEntry } from "../components/menus/menuEntries";
import { RowControls } from "../components/controls/RowControls";
import { useResolveAbsolutePath } from "../components/host/useResolveAbsolutePath";
import { useCopyPath } from "../components/host/useCopyPath";
import { type HostPath, useHostPathActions } from "../components/host/useHostPathActions";
import { useDaemon } from "../store";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { useHostPathsOrEmpty } from "../components/hooks/useHostPaths";
import { isLaunchableName } from "../launchable";
import { itemKeyOf, itemKeySelector } from "../primitives";
import { focusRowControl } from "./focusRow";

/** The live directory behind the displayed rows: the worktree a branch is checked out in, or the
 * project's directory when it is checked out nowhere. Historical rows act on today's files. */
export const RowPathScope = createContext<{ project: string; worktree?: string } | undefined>(undefined);

/** Opens the project's Terminal panel with a new shell terminal in the folder, which the window
 * provides; where none is provided a row offers no Terminal. */
export const OpenShellTerminal = createContext<((folder: HostPath) => void) | undefined>(undefined);

/** Opens Find in files limited to a folder of the project's directory, given its wire path; provided by the Files mode,
 * whose rows are the project's live files. Where none is provided a folder offers no Find in folder. */
export const FindInDirectory = createContext<((directory: string) => void) | undefined>(undefined);

/** Whether rows offer Add as context, which writes a row's path into the input of the session the window shows
 * (`QuoteTarget.addContext`); provided by the Files mode, whose rows are the project's live files. It is offered while
 * that session is one of the row's project. */
export const AddAsContextOffered = createContext(false);

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
  const resolveAbsolutePath = useResolveAbsolutePath();
  const scope = useContext(RowPathScope);
  const hostAction = useHostPathActions();
  const { isLocal } = useDaemon();
  const hostPaths = useHostPathsOrEmpty();
  const openTerminal = useContext(OpenShellTerminal);
  const findIn = useContext(FindInDirectory);
  const quoteTarget = useQuoteTarget();
  const contextTarget = useContext(AddAsContextOffered) && scope ? quoteTarget : undefined;
  // Read as the menu opens: whether the session the window shows is one of this project, and takes input now.
  const addAsContext = (): ActionMenuEntry[] => {
    if (!contextTarget || contextTarget.project() !== scope?.project) return [];
    const blocked = contextTarget.contextBlocked();
    return [{
      label: t("browser.addAsContext"), icon: MessageSquarePlus, disabled: blocked,
      reason: blocked ? { state: t("browser.addAsContextUnavailable") } : undefined,
      onClick: () => {
        // Offered when the menu opened: the session may have stopped taking input since.
        if (contextTarget.contextBlocked()) raiseError(new ActionableError(t("common.quoteFailedInput"), "contextInput"), { title: t("browser.addAsContextFailed") });
        // As the UI's own text shows it, as Copy relative path copies it.
        else contextTarget.addContext(displayWirePath(path));
      },
    }];
  };
  const leading = (): ActionMenuEntry[] => {
    const entries: ActionMenuEntry[] = [
      ...(directory && findIn ? [{ label: t("contentSearch.findInDirectory"), icon: TextSearch, onClick: () => findIn(path) }] : []),
      ...addAsContext(),
    ];
    return entries.length > 0 ? [...entries, "separator"] : [];
  };
  const items: ActionMenuEntry[] = [
    { label: t("common.copyRelativePath"), icon: Copy, onClick: () => copyPath(path) },
    ...(scope ? [{
      label: t("common.copyAbsolutePath"), icon: Copy, onClick: () => {},
      load: {
        key: JSON.stringify([scope.project, scope.worktree, path, t("common.copyAbsolutePath")]),
        run: () => resolveAbsolutePath({ ...scope, path }).then(
          (absolute) => ({ label: t("common.copyAbsolutePath"), icon: Copy, onClick: () => copyPath(absolute) }),
          () => ({ label: t("common.copyAbsolutePath"), icon: Copy, disabled: true, reason: { state: t("common.copyFailed") }, onClick: () => {} }),
        ),
      },
    }] : []),
  ];
  if (scope) {
    const run = (type: "open_project_path" | "reveal_project_path") => hostAction(type, { ...scope, path });
    const fileManager = { label: t("common.fileManager"), icon: FolderOpen, onClick: () => run("reveal_project_path") };
    // A folder's default application would be the file manager again, so a folder offers Terminal in its place.
    const terminal = openTerminal && { label: t("common.terminal"), icon: SquareTerminal, onClick: () => openTerminal({ ...scope, path }) };
    // The file manager and the default application are this machine's: not offered for a daemon elsewhere. A file the
    // system could launch has no default application, which the daemon would refuse.
    const onHost = isLocal !== false;
    const openIn: ActionMenuEntry[] = directory
      ? [...(onHost ? [fileManager] : []), ...(terminal ? [terminal] : [])]
      : onHost ? [fileManager, ...(isLaunchableName(name, hostPaths.style === "windows") ? [] : [{ label: t("common.defaultApplication"), icon: SquareArrowOutUpRight, onClick: () => run("open_project_path") }])] : [];
    if (openIn.length > 0) items.push("separator", { label: t("common.openIn"), icon: SquareArrowOutUpRight, items: openIn });
  }
  // The tree takes focus to the row as a menu closes, while a menu opened from the keyboard is to
  // give it back to its Actions button.
  const returnFocusToTrigger = (trigger: HTMLElement | null) => {
    const row = trigger?.closest<HTMLElement>(`[role=row]${itemKeySelector()}`);
    const key = itemKeyOf(row);
    if (trigger && row && key) focusRowControl(row.closest<HTMLElement>("[role=treegrid]"), key, trigger);
  };
  return (
    <RowControls reserve={reserve} className="ms-auto">
      <ActionMenu
        label={t(directory ? "browser.directory.actions" : "browser.row.actions", { name: isolate(name) })}
        contextTargetRef={contextTargetRef}
        onClosed={returnFocusToTrigger}
        items={contextTarget ? () => [...leading(), ...items] : [...leading(), ...items]}
      />
    </RowControls>
  );
}
