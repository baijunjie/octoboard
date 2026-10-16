import React from "react";

import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Session, SessionUsage } from "../protocol";
import { NEUTRAL_CHIP_COLOR_CLASS } from "./chipClasses";
import { IndicatorTooltip } from "./IndicatorTooltip";

export function hasVisibleUsage(usage: SessionUsage | null | undefined): usage is SessionUsage {
  return (usage?.total_tokens ?? 0) > 0;
}

/** The counts laid out as a table: the total as its heading, then one row per kind of token. */
function UsageTooltip({ total, rows, format }: {
  total: string;
  rows: { key: string; label: string; value: number }[];
  format: Intl.NumberFormat;
}): React.ReactElement {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="font-medium">{total}</div>
      <hr className="border-separator" />
      <dl className="grid grid-cols-[auto_auto] gap-x-4 gap-y-0.5">
        {rows.map((row) => (
          <React.Fragment key={row.key}>
            <dt className="text-muted">{row.label}</dt>
            <dd className="text-end tabular-nums">{format.format(row.value)}</dd>
          </React.Fragment>
        ))}
      </dl>
    </div>
  );
}

export function SessionUsageLabel({ usage }: { usage?: SessionUsage | null }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  if (!hasVisibleUsage(usage)) return null;
  const number = new Intl.NumberFormat(language);
  const tokens = (value: number) => isolate(number.format(value));
  const rows = [
    { key: "input", label: t("session.usage.input"), value: usage.input_tokens },
    { key: "output", label: t("session.usage.output"), value: usage.output_tokens },
    ...(usage.cached_input_tokens == null ? [] : [{ key: "cached", label: t("session.usage.cached"), value: usage.cached_input_tokens }]),
    ...(usage.cache_write_input_tokens == null ? [] : [{ key: "cacheWrite", label: t("session.usage.cacheWrite"), value: usage.cache_write_input_tokens }]),
  ];
  const total = t("session.usage.total", { tokens: tokens(usage.total_tokens) });
  // The accessible name is one plain sentence; the tooltip lays the same counts out as a table.
  const details = joinPhrases(language, [total, ...rows.map((row) => t("session.usage.entry", { label: row.label, tokens: tokens(row.value) }))]);
  const tooltip = <UsageTooltip total={total} rows={rows} format={number} />;
  const compact = new Intl.NumberFormat(language, { notation: "compact", maximumFractionDigits: 1 });
  // The chip's own `.sidebar-fills` keep its fill visible on the sidebar's `--panel-sidebar` and on
  // `--panel` (Focus cards, the archive): 1.19:1 to 1.24:1 resting, the same stacked on a hovered
  // or selected row. It is the smallest chip there is (HeroUI's `chip--sm` padding, its label's
  // included), one text line tall, so it sits in the metadata line without growing it.
  return (
    <IndicatorTooltip tooltip={tooltip} label={details} className={`sidebar-fills shrink-0 rounded-2xl px-1.5 text-xs leading-4 font-medium ${NEUTRAL_CHIP_COLOR_CLASS}`}>
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
