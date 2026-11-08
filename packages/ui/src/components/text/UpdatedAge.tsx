import React from "react";

import { formatRelativeTime, isJustNow } from "../../format/relativeTime";
import { useNow } from "../../format/useNow";
import { useCurrentLanguage, useT } from "../../i18n/react";

/** The two sentences of one surface's "Updated …" line: each surface keeps keys of its own, since one key carries one
 * meaning on one page. */
export type UpdatedAgeCopy =
  | { now: "quota.updatedNow"; age: "quota.updated" }
  | { now: "settings.accounts.mcp.updatedNow"; age: "settings.accounts.mcp.updated" };

/** When what a surface shows was read (`at`, in milliseconds), as "Updated just now" or "Updated 5 minutes ago",
 * kept current as time passes. */
export function UpdatedAge({ at, copy }: { at: number; copy: UpdatedAgeCopy }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  return <span>{isJustNow(at, now) ? t(copy.now) : t(copy.age, { age: formatRelativeTime(language, at, now) })}</span>;
}
