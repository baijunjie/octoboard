import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Console, Project, Session } from "../protocol";
import type { SettingsSectionId } from "../settings/SettingsDialog";

/** Which archive the archive view lists: a project's archived sessions, a console session's own
 * archived bound sessions, or with neither `project` nor `consoleSession` the console's archived
 * console sessions. A discriminated union rather than two optional fields, so `project` and
 * `consoleSession` being mutually exclusive is enforced by the type rather than left to prose: a
 * value naming both does not type-check, and narrowing one in by `"project" in scope` (or the
 * reverse) rules the other out too.
 *
 * TODO(docs/plans/20261008-console-sessions-and-agent-accounts/13-focus-modes.md): nothing opens
 * this scope with `consoleSession` set yet — a console session's focus mode is where milestone 13
 * reaches it. */
export type ArchiveScope = { console: string } & ({ project: string } | { consoleSession: string } | {});

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
  /** Opens Settings at a section. */
  onOpenSettings: (section: SettingsSectionId) => void;
}
