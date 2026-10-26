import type { Console, Project, Session, SessionBranchItem } from "../protocol";

/** How a new session is bound to a console session, by where the dialog was opened from: the user
 * `choose`s one of the console's console sessions or none (the full project list); the session is
 * `unbound` and nothing is offered (a project's focus mode); or it is `bound` to one console
 * session, shown as fixed rather than asked (that console session's focus mode). */
export type NewSessionBinding = { kind: "choose" } | { kind: "unbound" } | { kind: "bound"; to: Session };

/** A dialog the sidebar's menus and the archive view ask the app to open. */
export type DialogRequest =
  | { kind: "new-console" }
  /** Imports configuration from a file the user picks. */
  | { kind: "import-configuration" }
  | { kind: "edit-console"; console: Console }
  | { kind: "delete-console"; console: Console }
  | { kind: "new-project"; console: Console }
  | { kind: "edit-project"; project: Project }
  | { kind: "delete-project"; project: Project }
  | { kind: "restore-project-changes"; project: Project }
  | { kind: "new-console-session"; console: Console }
  | { kind: "new-session"; console: Console; project: Project; binding: NewSessionBinding }
  | { kind: "rename-project"; project: Project }
  | { kind: "rename-session"; session: Session }
  /** `branches` is an inspection of the bound branches already made for this archive. */
  | { kind: "archive-session"; session: Session; branches?: SessionBranchItem[] }
  /** Moves `session` to `account` (`null`, the default account), named `accountName` in the
   * dialog. */
  | { kind: "switch-account"; session: Session; account: string | null; accountName: string }
  /** Hands `session` off to a new session of another agent. */
  | { kind: "hand-off"; session: Session }
  /** Releases `session` from its owner. */
  | { kind: "unbind-session"; session: Session }
  | { kind: "delete-session"; session: Session }
  /** Every archived session of `project`, or every archived session bound to `consoleSession`, or
   * with neither every archived console session of `console`. */
  | { kind: "delete-archived"; console: Console; project?: Project; consoleSession?: Session; count: number };

/** The dialog requests that open a session, which a quick create answers without the dialog. */
export type NewSessionRequest = Extract<DialogRequest, { kind: "new-console-session" | "new-session" }>;
