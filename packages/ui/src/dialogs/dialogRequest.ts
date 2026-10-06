import type { Console, Project, Session } from "../protocol";

/** A dialog the sidebar's menus and the archive view ask the app to open. */
export type DialogRequest =
  | { kind: "new-console" }
  | { kind: "edit-console"; console: Console }
  | { kind: "delete-console"; console: Console }
  | { kind: "new-project"; console: Console }
  | { kind: "edit-project"; project: Project }
  | { kind: "delete-project"; project: Project }
  | { kind: "new-session"; console: Console; project: Project }
  | { kind: "rename-project"; project: Project }
  | { kind: "rename-session"; session: Session }
  | { kind: "archive-session"; session: Session }
  | { kind: "delete-session"; session: Session }
  /** Every archived session of `project`, or with none every archived hub of `console`. */
  | { kind: "delete-archived"; console: Console; project?: Project; count: number };
