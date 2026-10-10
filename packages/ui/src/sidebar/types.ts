import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Console, Project, Session } from "../protocol";
import type { SettingsSectionId } from "../settings/SettingsDialog";

/** Which archive the archive view lists: a project's archived sessions, the archived sessions
 * under a console session (bound to it, or to an archived lead session bound to it), or with
 * neither `project` nor `consoleSession` the console's archived console sessions. A discriminated
 * union rather than two optional fields, so `project` and `consoleSession` being mutually
 * exclusive is enforced by the type rather than left to prose: a value naming both does not
 * type-check, and narrowing one in by `"project" in scope` (or the reverse) rules the other out
 * too. A console session's focus mode opens the `consoleSession` scope. */
export type ArchiveScope = { console: string } &
  (
    | { project: string; consoleSession?: never }
    | { consoleSession: string; project?: never }
    | { project?: never; consoleSession?: never }
  );

/** What focus mode is given over to: one project, or one console session. */
export type FocusTarget = { project: Project } | { consoleSession: Session };

/** The callbacks the sidebar triggers. Kept as one object, passed down by reference rather than
 * spread, so a child's prop list says exactly what data it narrows instead of inheriting whatever
 * the parent happened to have in scope under the same names. */
export interface SidebarHandlers {
  onSelectSession: (session: Session) => void;
  /** Enters a console session's focus mode and selects it, as one navigation. */
  onSwitchConsoleSession: (session: Session) => void;
  onOpenConsoleSession: (console: Console) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
  /** Enters the focus mode of a project or a console session, or with `undefined` leaves it. */
  onFocus: (target: FocusTarget | undefined) => void;
  onOpenArchive: (scope: ArchiveScope) => void;
  /** Opens the project's browser in the aside, starting nothing. */
  onBrowseProject: (project: Project) => void;
  /** Checks the project against its remote and fast-forwards it now, whatever the automatic sync
   * setting says. */
  onSyncProjectGit: (project: Project) => void;
  onSetPinned: (target: { project: Project } | { session: Session }, pinned: boolean) => void;
  /** Opens Settings at a section. */
  onOpenSettings: (section: SettingsSectionId) => void;
}
