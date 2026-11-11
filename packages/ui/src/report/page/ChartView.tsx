// A page's chart: its box, accessible name and states, around the drawing, which loads with ECharts only the first
// time a page shows a chart.
import React, { Suspense } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { StatusMark } from "../../components/status/StatusMark";
import { retryableLazy } from "../../components/hooks/retryableLazy";
import { useT } from "../../i18n/react";
import { useOctoboardTheme } from "../../preferences/theme";
import { RendererBoundary } from "../../viewer/code/codeRenderer";
import type { ComponentProps } from "./registry";

const pageChart = retryableLazy(() => import("./chart/PageChart").then((module) => ({ default: module.PageChart })));
const PageChart = pageChart.Component;

/** The height of a chart that names none. */
export const DEFAULT_CHART_HEIGHT = 320;

/** The text of the option's (first) title, when it has one. */
function titleText(option: Record<string, unknown>): string | undefined {
  const title = Array.isArray(option.title) ? (option.title as unknown[])[0] : option.title;
  const text = typeof title === "object" && title !== null ? (title as { text?: unknown }).text : undefined;
  return typeof text === "string" && text.trim() !== "" ? text : undefined;
}

/**
 * A chart, drawn by ECharts in the user's appearance, with the hover tooltip, legend toggling and zoom ECharts gives
 * it. A screen reader reads it as one image named by its `summary` (or its title's text). While the library loads the
 * box keeps its height; where the option cannot be drawn, the box says so.
 */
export function ChartView({ component }: ComponentProps<"chart">): React.ReactElement {
  const t = useT();
  const { resolved } = useOctoboardTheme();
  const height = component.height ?? DEFAULT_CHART_HEIGHT;
  const name = component.summary ?? titleText(component.option) ?? t("report.chart.name");
  // The box keeps the chart's height, so what is below it does not move; the sentence is the mark's name and its text.
  const failed = (
    <div style={{ height }} className="flex items-center justify-center text-sm">
      <StatusMark tone="danger" label={t("report.chart.failed")} text={t("report.chart.failed")} />
    </div>
  );
  return (
    <RendererBoundary resetKey={JSON.stringify(component.option)} fallback={failed} retry={pageChart.retry}>
      <div role="img" aria-label={name} dir="auto" style={{ minHeight: height }}>
        <Suspense fallback={trackSuspenseFallback(<div style={{ height }} />)}>
          <PageChart option={component.option} height={height} appearance={resolved} />
        </Suspense>
      </div>
    </RendererBoundary>
  );
}
