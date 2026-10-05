import type { Console, Project, Session } from "../protocol";

/** A dialog the sidebar's menus ask the app to open. */
export type DialogRequest =
  | { kind: "new-console" }
  | { kind: "edit-console"; console: Console }
  | { kind: "delete-console"; console: Console }
  | { kind: "new-project"; console: Console }
  | { kind: "edit-project"; project: Project }
  | { kind: "delete-project"; project: Project }
  | { kind: "new-session"; console: Console; project: Project }
  | { kind: "rename-session"; session: Session }
  | { kind: "archive-session"; session: Session };
