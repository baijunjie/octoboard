import React from "react";

import { shortCommit } from "./shortCommit";

/** A commit's short id as a row's muted monospace label. It reads left to right whatever the
 * interface direction is, being a hash rather than prose. */
export function CommitIdLabel({ commit }: { commit: string }): React.ReactElement {
  return (
    <span dir="ltr" className="shrink-0 font-mono text-xs text-app-muted">
      {shortCommit(commit)}
    </span>
  );
}
