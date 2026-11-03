import type { Event } from "../protocol";

/** A file in a project's directory, or in one of its worktrees, for the viewer to show, and the line
 * to show it at. `read`, when given, is the reply of reading it already, which the viewer starts
 * from instead of showing itself loading. */
export interface ViewedFile {
  project: string;
  worktree?: string;
  /** A wire path below the project's, or the worktree's, directory. */
  path: string;
  line?: number;
  /** A heading of a Markdown document to scroll to, as a link to one asks. */
  fragment?: string;
  read?: Extract<Event, { type: "project_file" }>;
}
