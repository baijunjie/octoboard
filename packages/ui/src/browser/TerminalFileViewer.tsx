import React, { useEffect } from "react";

import { FileViewer } from "../viewer/FileViewer";
import type { Event } from "../protocol";
import { useFileReader } from "./useFileReader";

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

/**
 * The file viewer on a file a path in a session's terminal named, in a project's directory or one
 * of its worktrees, shown over the window with the project pane left as it is. It reads the file
 * itself, so it needs no project browser; a new `file` (the same path at another line, too) shows it
 * again. It has no Previous and Next, there being no list of files it was opened from. A Markdown
 * file's links to other files open in it too, in the project's own directory; in a worktree they
 * stay text, as the project's files they would be read from are not the worktree's.
 */
export function TerminalFileViewer({
  file,
  onClose,
  onOpenFile,
}: {
  file: ViewedFile;
  onClose: () => void;
  /** Shows another file of the same project and directory, as a link in a document asks. */
  onOpenFile: (file: ViewedFile) => void;
}): React.ReactElement | null {
  const reader = useFileReader(file.project, file.worktree);
  useEffect(() => reader.open(file.path, file.read), [file]);
  if (!reader.subject) return null;
  return (
    <FileViewer
      subject={reader.subject}
      onClose={onClose}
      line={file.line}
      location={
        file.worktree === undefined
          ? {
            project: file.project,
            anchor: file.fragment,
            onOpenFile: (path, fragment) => onOpenFile({ project: file.project, path, fragment }),
            // A link to a folder is not told from one to a file; it opens as a file and fails
            // as the viewer says.
            isDirectory: () => false,
          }
          : undefined
      }
    />
  );
}
