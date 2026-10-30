import type { ActionMenuEntry } from "../components/ActionMenu";
import type { DialogRequest, NewSessionRequest } from "../dialogs/dialogRequest";
import type { PanelKind } from "../panels/stackState";
import type { Console, ConsoleSessionColour, Project, Session } from "../protocol";
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

/** The panels of a project's right pane that a project menu can open. */
export type ProjectPanelKind = Extract<PanelKind, "files" | "processes" | "terminal">;

/** What focus mode is given over to: one project, or one console session. */
export type FocusTarget = { project: Project } | { consoleSession: Session };

/** The sidebar action callbacks and shared menu state. Kept as one object, passed down by reference rather than
 * spread, so a child's prop list says exactly what data it narrows instead of inheriting whatever
 * the parent happened to have in scope under the same names. */
export interface SidebarHandlers {
  /** The project that owns the right pane and the panels of it on screen, docked or floating. */
  visibleProjectPanels?: { project: string; kinds: readonly string[] };
  onSelectSession: (session: Session) => void;
  onReopenSession: (session: Session) => void;
  /** Enters a console session's focus mode and selects it, as one navigation. */
  onSwitchConsoleSession: (session: Session) => void;
  onOpenConsoleSession: (console: Console) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
  /** Opens the session the dialog `target` would open, without the dialog, on its defaults. */
  onQuickCreateSession: (target: NewSessionRequest) => void;
  /** Enters the focus mode of a project or a console session, or with `undefined` leaves it. */
  onFocus: (target: FocusTarget | undefined) => void;
  onOpenArchive: (scope: ArchiveScope) => void;
  /** Opens the project's panel of `kind` in the aside, as its top-bar toggle does; the Terminal
   * panel starts a shell only when the project has none. */
  onOpenProjectPanel: (project: Project, kind: ProjectPanelKind) => void;
  /** Opens the project directory in the system file manager. */
  onOpenProjectDirectory: (project: Project) => void;
  /** Opens the system terminal at the project's directory. */
  onOpenSystemTerminal: (project: Project) => void;
  /** Opens a validated repository web URL; absent without browser-link support. */
  onOpenRepository?: (url: string) => void;
  onCopyProjectName: (project: Project) => void;
  onCopySessionId: (session: Session) => void;
  /** Checks the project against its remote and fast-forwards it now, whatever the automatic pull
   * setting says. */
  onPullProjectBranch: (project: Project) => void;
  /** Loads the local branch choices for this project's Switch branch submenu. */
  onLoadProjectBranches: (project: Project) => Promise<ActionMenuEntry[]>;
  /** Pushes the project's current branch. */
  onPushProjectBranch: (project: Project) => void;
  /** Sets the colour of a console session. */
  onSetSessionColour: (session: Session, colour: ConsoleSessionColour) => void;
  onSetLocked: (session: Session, locked: boolean) => void;
  onSetPinned: (target: { project: Project } | { session: Session }, pinned: boolean) => void;
  /** Archives the session, asking first unless nothing is cut short. */
  onArchiveSession: (session: Session) => void;
  /** Opens Settings at a section. */
  onOpenSettings: (section: SettingsSectionId) => void;
}
