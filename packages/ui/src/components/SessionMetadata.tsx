import { Meter, ProgressCircle } from "@heroui/react";
import React from "react";

import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Session, SessionContext } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { useNow } from "../useNow";
import { IndicatorTooltip } from "./IndicatorTooltip";
import { StatusBadge } from "./StatusBadge";

export function hasVisibleContext(context: SessionContext | null | undefined): context is SessionContext {
  return (context?.tokens ?? 0) > 0;
}

/** The tone class (`context-tone--*` in style.css) for how full the window is: filling up above
 * half, nearly full above 80%. */
function contextTone(percent: number): string {
  if (percent > 80) return "context-tone--danger";
  if (percent > 50) return "context-tone--warning";
  return "context-tone--accent";
}

/** The share of the window used as HeroUI's `ProgressCircle`, shrunk to sit before the size inside
 * its badge, its arc in the tone of how full the window is. */
function ContextRing({ label, percent }: { label: string; percent: number }): React.ReactElement {
  return (
    <ProgressCircle aria-label={label} value={percent} className={`context-ring shrink-0 ${contextTone(percent)}`}>
      <ProgressCircle.Track className="size-3">
        <ProgressCircle.TrackCircle />
        <ProgressCircle.FillCircle />
      </ProgressCircle.Track>
    </ProgressCircle>
  );
}

interface ContextUsage {
  percent: number;
  used: string;
  left: string;
}

/** The context window as a card: its name with the size used against the limit, a meter of the
 * share used, and that share with what is left beneath. Without a limit it is the name and the size
 * alone. The meter is hidden from assistive technology, since the indicator's name already says
 * the share. */
function ContextTooltip({ title, amount, usage }: { title: string; amount: string; usage?: ContextUsage }): React.ReactElement {
  return (
    <div className="flex min-w-56 flex-col gap-2 py-0.5">
      <div className="flex items-baseline justify-between gap-x-6">
        <span className="text-app-muted">{title}</span>
        <span dir="ltr" className="tabular-nums">{amount}</span>
      </div>
      {usage && (
        <>
          <div aria-hidden="true">
            <Meter aria-label={title} value={usage.percent} className={`gap-0 ${contextTone(usage.percent)}`}>
              <Meter.Track>
                <Meter.Fill />
              </Meter.Track>
            </Meter>
          </div>
          <div className="flex items-baseline justify-between gap-x-6 text-app-muted tabular-nums">
            <span>{usage.used}</span>
            <span>{usage.left}</span>
          </div>
        </>
      )}
    </div>
  );
}

/** What is known of a session's context, in words: one plain sentence with exact counts for an
 * accessible name, and the compact size against the limit with the share used and what is left for
 * display, where the agent gives a limit. */
function useContextFacts(context: SessionContext): { details: string; title: string; compactSize: string; amount: string; usage?: ContextUsage } {
  const t = useT();
  const language = useCurrentLanguage();
  const number = new Intl.NumberFormat(language);
  const compact = new Intl.NumberFormat(language, { notation: "compact", maximumFractionDigits: 1 });
  const phrases = [t("session.context.heading", { count: context.tokens, tokens: isolate(number.format(context.tokens)) })];
  const compactSize = compact.format(context.tokens);
  let amount = isolate(compactSize);
  let usage: ContextUsage | undefined;
  if (context.window != null) {
    const remaining = Math.max(0, context.window - context.tokens);
    // The daemon sends the agent's own share, a whole percent, with every limit; working it out is
    // only a fallback.
    const percent = context.used_percent ?? Math.round((context.tokens / context.window) * 100);
    const share = isolate(new Intl.NumberFormat(language, { style: "percent" }).format(percent / 100));
    amount = t("session.context.amount", { tokens: amount, limit: isolate(compact.format(context.window)) });
    usage = {
      percent,
      used: t("session.context.used", { percent: share }),
      left: t("session.context.left", { tokens: isolate(compact.format(remaining)) }),
    };
    phrases.push(
      t("session.context.entry", { label: t("session.context.limit"), value: isolate(number.format(context.window)) }),
      usage.used,
      t("session.context.left", { tokens: isolate(number.format(remaining)) }),
    );
  }
  return { details: joinPhrases(language, phrases), title: t("session.context.title"), compactSize, amount, usage };
}

/** The context size as a badge of the compact size, led by a ring of the share used when the agent
 * gives a limit; the exact counts are in the name and the tooltip's card. */
export function SessionContextLabel({
  context,
  touchPopover,
}: {
  context?: SessionContext | null;
  /** Whether a tap opens the tooltip's card in a popover; off inside a pressable row, whose tap it is. */
  touchPopover?: boolean;
}): React.ReactElement | null {
  if (!hasVisibleContext(context)) return null;
  return <ContextLabel context={context} touchPopover={touchPopover} />;
}

function ContextLabel({ context, touchPopover }: { context: SessionContext; touchPopover?: boolean }): React.ReactElement {
  const { details, title, compactSize, amount, usage } = useContextFacts(context);
  // The badge keeps the metadata line as tall as the text beside it, the ring included.
  const mark = (
    <span aria-hidden="true" className="contents">
      <StatusBadge separated={false} className={usage ? "inline-flex items-center gap-1 ps-1" : ""}>
        {usage && <ContextRing label={title} percent={usage.percent} />}
        {compactSize}
      </StatusBadge>
    </span>
  );
  return (
    <IndicatorTooltip tooltip={<ContextTooltip title={title} amount={amount} usage={usage} />} label={details} className="flex shrink-0 items-center" touchPopover={touchPopover}>
      {mark}
    </IndicatorTooltip>
  );
}

export function ArchivedSessionMetadata({ session }: { session: Pick<Session, "started_at" | "ended_at"> }): React.ReactElement {
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

/** A compact archive time for preview rows and menu items, named by its full local date. */
export function ArchivedSessionTime({ endedAt }: { endedAt: Session["ended_at"] }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  if (endedAt == null) return null;
  const date = new Intl.DateTimeFormat(language, {
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit",
  });
  return (
    <time
      dateTime={new Date(endedAt).toISOString()}
      aria-label={t("session.meta.archived", { time: isolate(date.format(endedAt)) })}
      className="shrink-0 text-xs text-app-muted whitespace-nowrap"
    >
      {formatRelativeTime(language, endedAt, now)}
    </time>
  );
}
