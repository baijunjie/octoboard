import { FileQuestion, FileSymlink } from "lucide-react";
import type { ReactElement } from "react";

import { FileTypeIcon } from "../components/FileTypeIcon";
import type { BrowseEntry } from "../protocol";

export function FileIcon({ entry, path }: { entry: BrowseEntry | undefined; path: string }): ReactElement {
  if (entry?.kind === "file") return <FileTypeIcon path={path} />;
  const Icon = !entry || entry.kind === "other"
    ? FileQuestion
    : entry.kind === "symlink"
      ? FileSymlink
      : FileQuestion;
  return <Icon aria-hidden="true" className="size-4 shrink-0 text-app-muted" />;
}
