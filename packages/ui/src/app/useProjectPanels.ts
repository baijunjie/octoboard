import { useStableHandler } from "../components/hooks/useLatest";
import type { HostPath } from "../components/host/useHostPathActions";
import type { AsideOwner } from "../layout/asideOwner";
import { openPanel, OWNER_KINDS, type OwnerKind, type PanelStack, type PanelStacks } from "../panels/stackState";
import type { PanelKind } from "../layout/panelKind";
import { setProjectBrowserMode } from "../project-browser/browserState";
import type { Project } from "../protocol";
import type { ProjectPanelKind } from "../sidebar/types";
import { useDaemon } from "../store";
import { startShellTerminal } from "../terminal/shell/shellTerminals";

/** Opening a panel of the aside's owner (a project, or a console session's report), a project's as its top-bar toggle
 * opens it, and a shell terminal in one of a project's folders. Each is one function for good, for the sidebar's rows,
 * the project browser's row actions and the global process list. */
export function useProjectPanels({
  setOwnerByUser,
  asideFloats,
  openFloating,
  panelStacks,
  setPanelStack,
  noteOpened,
  sidebarOpen,
  closeSidebar,
  showAllOf,
}: {
  setOwnerByUser: (owner: AsideOwner | undefined) => void;
  asideFloats: boolean;
  openFloating: (owner: OwnerKind, kind: PanelKind) => void;
  panelStacks: PanelStacks;
  setPanelStack: (owner: OwnerKind, stack: PanelStack) => void;
  /** Notes `kind` as the panel `owner` opened last, for the floating panel to bring back. */
  noteOpened: (owner: OwnerKind, kind: PanelKind) => void;
  sidebarOpen: boolean;
  closeSidebar: () => void;
  /** Has the Processes panel list the processes of all of a project's sessions. */
  showAllOf: (projectId: string) => void;
}) {
  const daemon = useDaemon();
  /** Puts the `kind` panel of the aside's `owner` (a project, or a console session's report) on screen: the owner owns
   * the aside, and the panel opens docked, or floating in a narrow window (a maximized panel stays maximized),
   * without selecting or starting any session: the terminal beside it stays as it is. */
  const showPanel = (owner: AsideOwner, kind: PanelKind) => {
    setOwnerByUser(owner);
    if (asideFloats) return openFloating(owner.kind, kind);
    setPanelStack(owner.kind, openPanel(panelStacks[owner.kind], kind, OWNER_KINDS[owner.kind]));
    noteOpened(owner.kind, kind);
    // The sidebar's drawer in a narrow window covers what was asked for.
    if (sidebarOpen) closeSidebar();
  };

  /** Opens a project's panel as its top-bar toggle opens it: the Terminal panel starts a shell
   * only when the project has none, and the Processes panel lists the processes of all its sessions. */
  const openProjectPanel = (project: Project, kind: ProjectPanelKind) => {
    if (kind === "files") setProjectBrowserMode(project.id, "files");
    // Every session's processes, not those of the selected session the panel would follow.
    if (kind === "processes") showAllOf(project.id);
    showPanel({ kind: "project", project: project.id }, kind);
    if (kind === "terminal") void startShellTerminal(daemon, project.id, { ifNone: true });
  };

  /** Shows a project's Terminal panel and starts a new shell terminal in the folder, which
   * becomes the shown tab and takes focus. */
  const openShellTerminal = ({ project, worktree, path }: HostPath) => {
    showPanel({ kind: "project", project }, "terminal");
    void startShellTerminal(daemon, project, { place: { ...(worktree !== undefined && { worktree }), path }, focus: true });
  };

  const stableOpenProjectPanel = useStableHandler(openProjectPanel);
  const stableOpenShellTerminal = useStableHandler(openShellTerminal);
  const stableShowPanel = useStableHandler(showPanel);
  return { stableOpenProjectPanel, stableOpenShellTerminal, stableShowPanel };
}
