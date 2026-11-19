import type { Event } from "../protocol";
import type { LineRange } from "./code/lineMarks";

/** A file in a project's directory, or in one of its worktrees, for the viewer to show, and the line
 * to show it at, with the ranges of that line's text to mark (a content search's matches on it). `read`, when given, is the reply of reading it already, which the viewer starts
 * from instead of showing itself loading. */
export interface ViewedProjectFile {
  project: string;
  worktree?: string;
  /** A wire path below the project's, or the worktree's, directory. */
  path: string;
  line?: number;
  marks?: readonly LineRange[];
  /** A heading of a Markdown document to scroll to, as a link to one asks. */
  fragment?: string;
  read?: Extract<Event, { type: "project_file" }>;
  launchable?: true;
}

/** A file a path in a terminal named that lies outside every project directory (in a temporary directory), shown from
 * the path as the terminal wrote it, which the daemon reads with `read_path_link`. `project` is the session's, if it
 * has one, which the daemon needs to tell a project kept under a temporary directory from scratch space. */
export interface ViewedLinkedFile {
  link: string;
  project?: string;
  line?: number;
  marks?: readonly LineRange[];
  launchable?: true;
}

/** What the viewer is asked to show over the window: a file of a project, or a file only a terminal's path link
 * names. `launchable` is the daemon's word that the system could launch the file (a path link's `viewer_only`),
 * which the viewer offers no Default application for. */
export type ViewedFile = ViewedProjectFile | ViewedLinkedFile;

export function isLinkedFile(file: ViewedFile): file is ViewedLinkedFile {
  return "link" in file;
}
