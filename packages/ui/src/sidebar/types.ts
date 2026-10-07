import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Console, Project, Session } from "../protocol";

/** Which archive the archive view lists: a project's archived sessions, or with no `project` the
 * console's archived console sessions. */
export interface ArchiveScope {
  console: string;
  project?: string;
}

/** The callbacks the sidebar triggers. Kept as one object, passed down by reference rather than
 * spread, so a child's prop list says exactly what data it narrows instead of inheriting whatever
 * the parent happened to have in scope under the same names. */
export interface SidebarHandlers {
  onSelectSession: (session: Session) => void;
  onOpenConsoleSession: (console: Console) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
  onSelectConsole: (consoleId: string) => void;
  /** Enters a project's focus mode, or with `undefined` leaves it. */
  onFocusProject: (projectId: string | undefined) => void;
  onOpenArchive: (scope: ArchiveScope) => void;
  onSetPinned: (target: { project: Project } | { session: Session }, pinned: boolean) => void;
}
