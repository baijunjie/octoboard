// The components of a page that only show something: a callout, key-value pairs, a figure, code and a diff.
import { ArrowDown, ArrowRight, ArrowUp, CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react";
import React from "react";

import { useT } from "../../i18n/react";
import type { CalloutTone, StatSentiment, StatTrend } from "../../protocol";
import { useOctoboardTheme } from "../../preferences/theme";
import { FencedCode } from "../../viewer/markdown/FencedCode";
import { useCellText } from "./cells";
import { COPY_BUTTON_INSET, CopyBox } from "../../components/controls/CopyBox";
import { ScrollBox } from "../../components/scroll/ScrollBox";
import { BlockLabel, useCodeSurface, useCopyNames } from "./copyable";
import { HeadingLevelContext, useChildHeadingLevel } from "./headingLevel";
import { PageMarkdown } from "./PageMarkdown";
import type { ComponentProps } from "./registry";

const CALLOUT_TONES = {
  info: { Icon: Info, box: "border-(--tint-accent-edge) bg-(--tint-accent-bg)", icon: "text-accent-soft-foreground" },
  success: { Icon: CircleCheck, box: "border-(--tint-success-edge) bg-(--tint-success-bg)", icon: "text-success-soft-foreground" },
  warning: { Icon: TriangleAlert, box: "border-(--tint-warning-edge) bg-(--tint-warning-bg)", icon: "text-warning-soft-foreground" },
  danger: { Icon: CircleAlert, box: "border-(--tint-danger-edge) bg-(--tint-danger-bg)", icon: "text-danger-soft-foreground" },
} satisfies Record<CalloutTone, { Icon: React.ComponentType<{ className?: string; "aria-hidden"?: "true" }>; box: string; icon: string }>;

export function CalloutView({ component }: ComponentProps<"callout">): React.ReactElement {
  const t = useT();
  const tone = component.tone ?? "info";
  const { Icon, box, icon } = CALLOUT_TONES[tone];
  const { Tag, inner } = useChildHeadingLevel(component.title !== undefined);
  return (
    <div role="note" className={`flex gap-2.5 rounded-lg border p-3 ${box}`}>
      <Icon aria-hidden="true" className={`mt-0.5 size-4 shrink-0 ${icon}`} />
      <div className="min-w-0 flex-1">
        <span className="sr-only">{t(`report.callout.${tone}`)}</span>
        {component.title !== undefined && <Tag className="text-sm font-semibold align-match-parent" dir="auto">{component.title}</Tag>}
        <HeadingLevelContext.Provider value={inner}>
          <PageMarkdown text={component.text} />
        </HeadingLevelContext.Provider>
      </div>
    </div>
  );
}

export function KeyValuesView({ component }: ComponentProps<"key_values">): React.ReactElement {
  const cellText = useCellText();
  return (
    <dl className="grid grid-cols-[minmax(0,max-content)_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-sm">
      {component.items.map((item, index) => (
        <React.Fragment key={index}>
          <dt className="font-medium text-app-muted align-match-parent [overflow-wrap:anywhere]" dir="auto">{item.key}</dt>
          <dd className="align-match-parent [overflow-wrap:anywhere]" dir="auto">{cellText(item.value)}</dd>
        </React.Fragment>
      ))}
    </dl>
  );
}

const TREND_ICONS: Record<StatTrend, React.ComponentType<{ className?: string; "aria-hidden"?: "true" }>> = {
  up: ArrowUp,
  down: ArrowDown,
  flat: ArrowRight,
};

const SENTIMENT_CLASS: Record<StatSentiment, string> = {
  positive: "text-(--tint-success-fg)",
  negative: "text-(--tint-danger-fg)",
  neutral: "text-app-muted",
};

const TREND_TEXT = { up: "report.stat.trendUp", down: "report.stat.trendDown", flat: "report.stat.trendFlat" } as const;

export function StatView({ component }: ComponentProps<"stat">): React.ReactElement {
  const t = useT();
  const cellText = useCellText();
  const TrendIcon = component.trend && TREND_ICONS[component.trend];
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="text-xs text-app-muted align-match-parent [overflow-wrap:anywhere]" dir="auto">{component.label}</span>
      <span className="text-2xl font-semibold tabular-nums align-match-parent [overflow-wrap:anywhere]" dir="auto">{cellText(component.value)}</span>
      {(component.change !== undefined || component.trend !== undefined) && (
        <span className={`inline-flex items-center gap-1 text-sm ${SENTIMENT_CLASS[component.sentiment ?? "neutral"]}`}>
          {TrendIcon && <TrendIcon aria-hidden="true" className="size-4 shrink-0" />}
          {component.trend && <span className="sr-only">{t(TREND_TEXT[component.trend])}</span>}
          {component.change !== undefined && <span dir="auto">{component.change}</span>}
        </span>
      )}
    </div>
  );
}

export function CodeView({ component }: ComponentProps<"code">): React.ReactElement {
  const t = useT();
  const { resolved: theme } = useOctoboardTheme();
  const { surface } = useCodeSurface();
  const names = useCopyNames(component.label);
  return (
    <div className="flex flex-col gap-1">
      <BlockLabel label={component.label} />
      <CopyBox text={component.code} dir="ltr" surface={surface} {...names}>
        <FencedCode language={component.language} text={component.code} theme={theme} highlight scroll label={component.label ?? t("report.block.code")} />
      </CopyBox>
    </div>
  );
}

const ADDED_FILL = "bg-(--tint-success-bg)";
const REMOVED_FILL = "bg-(--tint-danger-bg)";

/** How a line of a unified diff is drawn: an added line, a removed line, or a hunk header; the file headers and the
 * context have no tint. */
function diffLineClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---")) return "text-app-muted";
  if (line.startsWith("+")) return `${ADDED_FILL} text-(--tint-success-fg)`;
  if (line.startsWith("-")) return `${REMOVED_FILL} text-(--tint-danger-fg)`;
  if (line.startsWith("@@")) return "text-(--tint-accent-fg)";
  return "";
}

/** The fill of a diff's first line, which its copy button's backing takes so it does not stand out of that line. */
function firstLineFill(line: string | undefined): string | undefined {
  if (line === undefined || line.startsWith("+++") || line.startsWith("---")) return undefined;
  return line.startsWith("+") ? ADDED_FILL : line.startsWith("-") ? REMOVED_FILL : undefined;
}

export function DiffView({ component }: ComponentProps<"diff">): React.ReactElement {
  const t = useT();
  const lines = component.diff.replace(/\n$/, "").split("\n");
  const { style, surface, boxClass } = useCodeSurface();
  const names = useCopyNames(component.label);
  return (
    <div className="flex flex-col gap-1">
      <BlockLabel label={component.label} />
      <CopyBox text={component.diff} dir="ltr" surface={surface} tint={firstLineFill(lines[0])} {...names}>
        <ScrollBox label={component.label ?? t("report.block.diff")} className={boxClass} style={style}>
          <pre dir="ltr" className="w-max min-w-full py-2 font-mono text-xs leading-5">
            {lines.map((line, index) => <div key={index} className={`ps-3 ${COPY_BUTTON_INSET} ${diffLineClass(line)}`}>{line === "" ? " " : line}</div>)}
          </pre>
        </ScrollBox>
      </CopyBox>
    </div>
  );
}
