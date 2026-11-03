import React, { useEffect } from "react";

import { FileViewer } from "../../viewer/FileViewer";
import type { ViewedFile } from "../../viewer/viewedFile";
import { useFileReader } from "./useFileReader";

/**
 * The file viewer on a file a path in a session's terminal named, in a project's directory or one
 * of its worktrees, shown over the window with the project pane left as it is. It reads the file
 * itself, so it needs no project browser; a new `file` (the same path at another line, too) shows it
 * again. It has no Previous and Next, there being no list of files it was opened from. A Markdown
 * file's links to other files open in it too, in the project's own directory; in a worktree they
 * stay text, as the project's files they would be read from are not the worktree's.
 */
export function LinkedFileViewer({
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
