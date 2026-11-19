import React from "react";

import { formatRelativeTime, isJustNow } from "../../format/relativeTime";
import { useNow } from "../../format/useNow";
import { useCurrentLanguage, useT } from "../../i18n/react";
import type { MemoryWriter } from "../../protocol";
import { shownWriter } from "./memoryRules";

/** When an entry was last updated and by whom, kept current as time passes: "Updated 5 minutes ago by an agent" or "…
 * by you", and "Updated just now …" within the first minute, as `UpdatedAge` says it. Each writer has a sentence of its
 * own, so no language has a name glued into it; a writer of a kind this client does not know is left out, with the
 * "by". */
export function MemoryUpdated({ updatedAt, writer }: { updatedAt: number; writer: MemoryWriter }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const shown = shownWriter(writer);
  if (isJustNow(updatedAt, now)) {
    if (shown === "user") return <span>{t("memory.updatedNowByUser")}</span>;
    if (shown === "agent") return <span>{t("memory.updatedNowByAgent")}</span>;
    return <span>{t("memory.updatedNow")}</span>;
  }
  const time = formatRelativeTime(language, updatedAt, now);
  if (shown === "user") return <span>{t("memory.updatedByUser", { time })}</span>;
  if (shown === "agent") return <span>{t("memory.updatedByAgent", { time })}</span>;
  return <span>{t("memory.updated", { time })}</span>;
}
