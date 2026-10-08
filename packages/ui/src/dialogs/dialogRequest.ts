import type { Console, Project, Session } from "../protocol";

/** How a new session is bound to a console session, by where the dialog was opened from: the user
 * `choose`s one of the console's console sessions or none (the full project list); the session is
 * `unbound` and nothing is offered (a project's focus mode); or it is `bound` to one console
 * session, shown as fixed rather than asked (that console session's focus mode). */
export type NewSessionBinding = { kind: "choose" } | { kind: "unbound" } | { kind: "bound"; to: Session };

/** A dialog the sidebar's menus and the archive view ask the app to open. */
export type DialogRequest =
  | { kind: "new-console" }
  | { kind: "edit-console"; console: Console }
  | { kind: "delete-console"; console: Console }
  | { kind: "new-project"; console: Console }
  | { kind: "edit-project"; project: Project }
  | { kind: "delete-project"; project: Project }
  | { kind: "new-session"; console: Console; project: Project; binding: NewSessionBinding }
  | { kind: "rename-project"; project: Project }
  | { kind: "rename-session"; session: Session }
  | { kind: "archive-session"; session: Session }
  /** Moves `session` to `account` (`null`, the default account), named `accountName` in the
   * dialog. */
  | { kind: "switch-account"; session: Session; account: string | null; accountName: string }
  | { kind: "delete-session"; session: Session }
  /** Every archived session of `project`, or every archived session bound to `consoleSession`, or
   * with neither every archived console session of `console`. */
  | { kind: "delete-archived"; console: Console; project?: Project; consoleSession?: Session; count: number };
