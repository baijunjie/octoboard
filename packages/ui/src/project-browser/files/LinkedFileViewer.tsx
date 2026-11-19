import React, { useEffect } from "react";

import { FileViewer } from "../../viewer/FileViewer";
import { isLinkedFile, type ViewedFile, type ViewedLinkedFile, type ViewedProjectFile } from "../../viewer/viewedFile";
import { useFileReader } from "./useFileReader";
import { usePathLinkReader } from "./usePathLinkReader";

/**
 * The file viewer on a file a path in a session's terminal named, shown over the window with the project pane left as
 * it is: a file in a project's directory or one of its worktrees, read from there, or one in a temporary directory,
 * read through the daemon's path link rules. It reads the file itself, so it needs no project browser; a new `file`
 * (the same path at another line, too) shows it again. It has no Previous and Next, there being no list of files it was
 * opened from. A Markdown file's links to other files open in it too, in the project's own directory; in a worktree or
 * in a temporary directory they stay text, as the project's files they would be read from are not the ones beside it.
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
  return isLinkedFile(file) ? <PathLinkViewer file={file} onClose={onClose} /> : <ProjectFileViewer file={file} onClose={onClose} onOpenFile={onOpenFile} />;
}

function ProjectFileViewer({ file, onClose, onOpenFile }: { file: ViewedProjectFile; onClose: () => void; onOpenFile: (file: ViewedFile) => void }): React.ReactElement | null {
  const reader = useFileReader(file.project, file.worktree);
  useEffect(() => reader.open(file.path, file.read), [file]);
  if (!reader.subject) return null;
  const subject = file.launchable ? { ...reader.subject, launchable: true } : reader.subject;
  return (
    <FileViewer
      subject={subject}
      onClose={onClose}
      line={file.line}
      marks={file.marks}
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

/** A file outside every project directory has no project around it, so a relative link in it stays its text and a
 * relative image shows its alt text: no `location`. */
function PathLinkViewer({ file, onClose }: { file: ViewedLinkedFile; onClose: () => void }): React.ReactElement | null {
  const reader = usePathLinkReader(file.project);
  useEffect(() => reader.open(file.link), [file]);
  if (!reader.subject) return null;
  const subject = file.launchable ? { ...reader.subject, launchable: true } : reader.subject;
  return <FileViewer subject={subject} onClose={onClose} line={file.line} marks={file.marks} />;
}
