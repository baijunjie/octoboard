import React from "react";

import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Session, SessionContext } from "../protocol";
import { IndicatorTooltip } from "./IndicatorTooltip";
import { StatusBadge } from "./StatusBadge";

export function hasVisibleContext(context: SessionContext | null | undefined): context is SessionContext {
  return (context?.tokens ?? 0) > 0;
}

/** The context size as its heading, then a table of the limit and how much of it is used when the agent gives one. */
function ContextTooltip({ heading, rows }: {
  heading: string;
  rows: { key: string; label: string; value: string }[];
}): React.ReactElement {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="font-medium">{heading}</div>
      {rows.length > 0 && (
        <>
          <hr className="groove-t" />
          <dl className="grid grid-cols-[auto_auto] gap-x-4 gap-y-0.5">
            {rows.map((row) => (
              <React.Fragment key={row.key}>
                <dt className="text-muted">{row.label}</dt>
                <dd className="text-end tabular-nums">{row.value}</dd>
              </React.Fragment>
            ))}
          </dl>
        </>
      )}
    </div>
  );
}

/** `plain` leaves the tooltip out, for a menu's transient surface, where no mark carries one; the
 * size then reaches assistive technology through its name alone. */
export function SessionContextLabel({ context, plain }: { context?: SessionContext | null; plain?: boolean }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  if (!hasVisibleContext(context)) return null;
  const number = new Intl.NumberFormat(language);
  const heading = t("session.context.heading", { count: context.tokens, tokens: isolate(number.format(context.tokens)) });
  const rows = [];
  if (context.window != null) {
    const limit = isolate(number.format(context.window));
    rows.push({
      key: "limit",
      label: t("session.context.limit"),
      value: context.used_percent == null
        ? limit
        : t("session.context.limitValue", { limit, percent: isolate(new Intl.NumberFormat(language, { style: "percent" }).format(context.used_percent / 100)) }),
    });
  }
  // The accessible name is one plain sentence; the tooltip lays the same facts out as a table.
  const details = joinPhrases(language, [heading, ...rows.map((row) => t("session.context.entry", { label: row.label, value: row.value }))]);
  const compact = new Intl.NumberFormat(language, { notation: "compact", maximumFractionDigits: 1 });
  // The compact size is a badge, so the metadata line stays as tall as the text beside it; the
  // exact count is in the name and the tooltip.
  const badge = (
    <span aria-hidden="true" className="contents">
      <StatusBadge separated={false}>{compact.format(context.tokens)}</StatusBadge>
    </span>
  );
  if (plain) return <span role="img" aria-label={details} className="flex shrink-0">{badge}</span>;
  return (
    <IndicatorTooltip tooltip={<ContextTooltip heading={heading} rows={rows} />} label={details} className="flex shrink-0">
      {badge}
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
