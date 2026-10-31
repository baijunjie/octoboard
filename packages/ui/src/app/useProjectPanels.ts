import { useStableHandler } from "../components/hooks/useLatest";
import type { HostPath } from "../components/host/useHostPathActions";
import type { AsideOwner } from "../layout/asideOwner";
import { openPanel, OWNER_KINDS, type OwnerKind, type PanelKind, type PanelStack, type PanelStacks } from "../panels/stackState";
import { setProjectBrowserMode } from "../project-browser/browserState";
import type { Project } from "../protocol";
import type { ProjectPanelKind } from "../sidebar/types";
import { useDaemon } from "../store";
import { startShellTerminal } from "../terminal/shell/shellTerminals";

/** Opening a project's panel from the sidebar, as its top-bar toggle opens it, and a shell terminal in one of its
 * folders. Both are one function for good, for the sidebar's rows and the project browser's row actions. */
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
  /** Puts a project's `kind` panel on screen: the project owns the aside, and the panel opens
   * docked, or floating in a narrow window (a maximized panel stays maximized), without selecting or
   * starting any session: the terminal beside it stays as it is. */
  const showProjectPanel = (project: string, kind: ProjectPanelKind) => {
    setOwnerByUser({ kind: "project", project });
    if (asideFloats) return openFloating("project", kind);
    setPanelStack("project", openPanel(panelStacks.project, kind, OWNER_KINDS.project));
    noteOpened("project", kind);
    // The sidebar's drawer in a narrow window covers what was asked for.
    if (sidebarOpen) closeSidebar();
  };

  /** Opens a project's panel as its top-bar toggle opens it: the Terminal panel starts a shell
   * only when the project has none, and the Processes panel lists the processes of all its sessions. */
  const openProjectPanel = (project: Project, kind: ProjectPanelKind) => {
    if (kind === "files") setProjectBrowserMode(project.id, "files");
    // Every session's processes, not those of the selected session the panel would follow.
    if (kind === "processes") showAllOf(project.id);
    showProjectPanel(project.id, kind);
    if (kind === "terminal") void startShellTerminal(daemon, project.id, { ifNone: true });
  };

  /** Shows a project's Terminal panel and starts a new shell terminal in the folder, which
   * becomes the shown tab and takes focus. */
  const openShellTerminal = ({ project, worktree, path }: HostPath) => {
    showProjectPanel(project, "terminal");
    void startShellTerminal(daemon, project, { place: { ...(worktree !== undefined && { worktree }), path }, focus: true });
  };

  const stableOpenProjectPanel = useStableHandler(openProjectPanel);
  const stableOpenShellTerminal = useStableHandler(openShellTerminal);
  return { stableOpenProjectPanel, stableOpenShellTerminal };
}
