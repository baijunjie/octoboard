import React from "react";

import { hasProviderName } from "../../agents/providers";
import { ProviderName } from "../../components/text/ProviderName";
import { formatRelativeTime, isJustNow } from "../../format/relativeTime";
import { useNow } from "../../format/useNow";
import { Message, useCurrentLanguage, useT } from "../../i18n/react";
import type { MemoryWriter } from "../../protocol";
import { useProviders } from "../../store";
import { shownWriter, writerAgent } from "./memoryRules";

/** When an entry was last updated and by whom, kept current as time passes: "Updated 5 minutes ago by Codex", naming
 * the agent whose session wrote it as the app names agents elsewhere (as a reference within the sentence), "… by an
 * agent" for an agent this client cannot name, or "… by you", and "Updated just now …" within the first minute, as
 * `UpdatedAge` says it. Each writer has a sentence of its own, so no language has a name glued into it; a writer of a
 * kind this client does not know is left out, with the "by". */
export function MemoryUpdated({ updatedAt, writer }: { updatedAt: number; writer: MemoryWriter }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const providers = useProviders();
  const shown = shownWriter(writer);
  const agentId = shown === "agent" ? writerAgent(writer) : undefined;
  const agent = agentId !== undefined && hasProviderName(providers, agentId) ? <ProviderName agent={agentId} /> : undefined;
  if (isJustNow(updatedAt, now)) {
    if (shown === "user") return <span>{t("memory.updatedNowByUser")}</span>;
    if (agent) return <span><Message id="memory.updatedNowByNamedAgent" params={{ agent }} /></span>;
    if (shown === "agent") return <span>{t("memory.updatedNowByAgent")}</span>;
    return <span>{t("memory.updatedNow")}</span>;
  }
  const time = formatRelativeTime(language, updatedAt, now);
  if (shown === "user") return <span>{t("memory.updatedByUser", { time })}</span>;
  if (agent) return <span><Message id="memory.updatedByNamedAgent" params={{ time, agent }} /></span>;
  if (shown === "agent") return <span>{t("memory.updatedByAgent", { time })}</span>;
  return <span>{t("memory.updated", { time })}</span>;
}
