// The drawing of a page's chart. This module and ECharts load only when a page shows a chart (`../ChartView.tsx`).
import React, { useEffect, useRef, useState } from "react";

import { BarChart, FunnelChart, GaugeChart, HeatmapChart, LineChart, PieChart, RadarChart, ScatterChart } from "echarts/charts";
import { DataZoomComponent, GridComponent, LegendComponent, RadarComponent, TitleComponent, TooltipComponent, VisualMapComponent } from "echarts/components";
import * as echarts from "echarts/core";
import { SVGRenderer } from "echarts/renderers";

import { CATEGORICAL, NARROW_WIDTH, SEQUENTIAL, chartOption, type ChartLook } from "./chartOption";

// Only what the allowed subset draws is installed: no toolbox, graphic, map or HTML-writing component exists here to be
// reached. The SVG renderer writes every string as the text of an SVG text node.
echarts.use([
  BarChart, FunnelChart, GaugeChart, HeatmapChart, LineChart, PieChart, RadarChart, ScatterChart,
  DataZoomComponent, GridComponent, LegendComponent, RadarComponent, TitleComponent, TooltipComponent, VisualMapComponent,
  SVGRenderer,
]);

/** Each appearance's colours where a token cannot be read (a test's document has no style sheet). */
const FALLBACK = {
  light: { text: "#0b0b0b", muted: "#52514e", line: "#d8d8dc", overlay: "#ffffff" },
  dark: { text: "#f0efec", muted: "#c3c2b7", line: "#3a3a3f", overlay: "#303033" },
} as const;

/** The tokens of the tooltip's surface: a floating surface, which in dark is `--floating` and its edge (the rule in
 * `style.css` sets them for a tooltip, which a chart's own drawing is not inside), and in light HeroUI's overlay. */
const TOOLTIP_TOKENS = {
  light: { background: "--overlay", edge: "--border" },
  dark: { background: "--floating", edge: "--floating-edge" },
} as const;

/** The width a drawing is given while its box has none yet (before layout, or in a test's document). */
const WIDTH_BEFORE_LAYOUT = 600;

/** The computed value of the colour `token` where `host` sits, or `fallback` where it has none. */
function tokenColour(host: HTMLElement, token: string, fallback: string): string {
  const probe = document.createElement("span");
  probe.style.display = "none";
  probe.style.color = `var(${token})`;
  host.appendChild(probe);
  const value = getComputedStyle(probe).color;
  probe.remove();
  return value && !value.startsWith("var(") ? value : fallback;
}

/** The chart colours of `appearance`, reading the UI's tokens where `host` sits. */
function lookAt(host: HTMLElement, appearance: "light" | "dark"): ChartLook {
  const fallback = FALLBACK[appearance];
  return {
    text: tokenColour(host, "--foreground", fallback.text),
    muted: tokenColour(host, "--app-muted", fallback.muted),
    line: tokenColour(host, "--separator", fallback.line),
    tooltipBackground: tokenColour(host, TOOLTIP_TOKENS[appearance].background, fallback.overlay),
    tooltipText: tokenColour(host, "--overlay-foreground", fallback.text),
    tooltipBorder: tokenColour(host, TOOLTIP_TOKENS[appearance].edge, fallback.line),
    categorical: CATEGORICAL[appearance],
    sequential: SEQUENTIAL[appearance],
    reducedMotion: typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  };
}

/**
 * Draws `option` with ECharts into a box `height` pixels tall, in the colours of `appearance`: drawn anew when either
 * changes, resized with its box, and disposed of when it goes. A failure to draw is thrown to the enclosing boundary.
 */
export function PageChart({ option, height, appearance }: {
  option: Record<string, unknown>;
  height: number;
  appearance: "light" | "dark";
}): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const [failure, setFailure] = useState<unknown>();
  if (failure !== undefined) throw failure;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let chart: echarts.ECharts | undefined;
    const look = lookAt(host, appearance);
    let width = host.clientWidth || WIDTH_BEFORE_LAYOUT;
    try {
      chart = echarts.init(host, null, { renderer: "svg", width, height });
      chart.setOption(chartOption(option, look, width));
    } catch (error) {
      chart?.dispose();
      setFailure(error ?? new Error("The chart could not be drawn"));
      return;
    }
    const drawn = chart;
    const observer = typeof ResizeObserver === "function"
      ? new ResizeObserver(() => {
        const next = host.clientWidth;
        if (next <= 0) return;
        // The layout differs below a width, so a crossing is laid out anew.
        if ((next < NARROW_WIDTH) !== (width < NARROW_WIDTH)) drawn.setOption(chartOption(option, look, next), true);
        width = next;
        drawn.resize({ width, height });
      })
      : undefined;
    observer?.observe(host);
    return () => {
      observer?.disconnect();
      drawn.dispose();
    };
  }, [option, height, appearance]);

  return <div ref={hostRef} data-page-chart="" style={{ height }} className="w-full" />;
}
