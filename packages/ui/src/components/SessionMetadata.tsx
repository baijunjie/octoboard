import React from "react";

import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Session, SessionUsage } from "../protocol";
import { IndicatorTooltip } from "./IndicatorTooltip";

export function hasVisibleUsage(usage: SessionUsage | null | undefined): usage is SessionUsage {
  return (usage?.total_tokens ?? 0) > 0;
}

export function SessionUsageLabel({ usage }: { usage?: SessionUsage | null }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  if (!hasVisibleUsage(usage)) return null;
  const number = new Intl.NumberFormat(language);
  const tokens = (value: number) => isolate(number.format(value));
  const details = joinPhrases(language, [
    t("session.usage.total", { tokens: tokens(usage.total_tokens) }),
    t("session.usage.input", { tokens: tokens(usage.input_tokens) }),
    t("session.usage.output", { tokens: tokens(usage.output_tokens) }),
    ...(usage.cached_input_tokens == null ? [] : [t("session.usage.cached", { tokens: tokens(usage.cached_input_tokens) })]),
    ...(usage.cache_write_input_tokens == null ? [] : [t("session.usage.cacheWrite", { tokens: tokens(usage.cache_write_input_tokens) })]),
  ]);
  const compact = new Intl.NumberFormat(language, { notation: "compact", maximumFractionDigits: 1 });
  return (
    <IndicatorTooltip tooltip={details} label={details} className="shrink-0 text-xs text-app-muted">
      <span aria-hidden="true">{compact.format(usage.total_tokens)}</span>
    </IndicatorTooltip>
  );
}

export function ArchivedSessionMetadata({ session }: { session: Session }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const date = new Intl.DateTimeFormat(language, {
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit",
  });
  return (
    <div className="flex shrink-0 flex-col items-end gap-0.5 text-end text-xs text-app-muted">
      {session.ended_at != null && <time dateTime={new Date(session.ended_at).toISOString()}>{t("session.meta.archived", { time: isolate(date.format(session.ended_at)) })}</time>}
      <time dateTime={new Date(session.started_at).toISOString()}>{t("session.meta.created", { time: isolate(date.format(session.started_at)) })}</time>
    </div>
  );
}
