import type { PageContent } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, pageOf, projectOf, sessionOf, snapshotState } from "../../fixtures/builders";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const hub = sessionOf("s-chart-hub", console_.id, undefined, "Build health", "idle", { colour: "teal" });

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** A repeatable stand-in for noise: the same `n` always gives the same number in [0, 1). */
const wobble = (n: number): number => {
  const x = Math.sin(n * 12.9898) * 43758.5453;
  return x - Math.floor(x);
};

/** Sixty consecutive days from 13 September, as "Oct 13"-style labels, for the zoomable line. */
const sixtyDays = Array.from({ length: 60 }, (_, i) => {
  const date = new Date(Date.UTC(2026, 8, 13 + i));
  return `${MONTHS[date.getUTCMonth()]} ${date.getUTCDate()}`;
});
const medianMs = sixtyDays.map((_, i) => Math.round(118 + 14 * Math.sin(i / 6) + 8 * wobble(i)));
const slowestMs = sixtyDays.map((_, i) => Math.round(310 + 60 * Math.sin(i / 9 + 1) + (i > 40 ? (i - 40) * 7 : 0) + 40 * wobble(i + 100)));

/** Failures by weekday (rows) and four-hour block (columns), as [column, row, count]. */
const HOURS = ["00", "04", "08", "12", "16", "20"];
const failures = DAYS.flatMap((_, row) => HOURS.map((_, column) => {
  const working = row < 5 && column >= 2 && column <= 4;
  return [column, row, Math.round((working ? 6 : 1) + 4 * wobble(row * 7 + column))];
}));

/** A report whose charts are drawn over the real width of the Report panel, between paragraphs of Markdown. */
const buildHealth: PageContent = {
  title: "Build health, last week",
  body: [
    {
      type: "markdown",
      text: "Builds were **mostly green**, but the flaky share grew on Thursday and Friday. Hover a bar for the day's numbers; click a name in the legend to hide that result.",
    },
    {
      type: "chart",
      summary: "Stacked bars of builds per weekday by result: passed, flaky and failed. Friday has the most failures.",
      height: 320,
      option: {
        title: { text: "Builds per day", left: 0 },
        legend: { top: 28, left: 0, data: ["Passed", "Flaky", "Failed"] },
        tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
        grid: { left: 56, right: 16, top: 72, bottom: 32 },
        xAxis: { type: "category", data: DAYS },
        yAxis: { type: "value", name: "Builds" },
        series: [
          { type: "bar", name: "Passed", stack: "builds", data: [42, 48, 51, 39, 44, 12, 9] },
          { type: "bar", name: "Flaky", stack: "builds", data: [3, 4, 2, 8, 9, 1, 0] },
          { type: "bar", name: "Failed", stack: "builds", data: [2, 1, 3, 5, 7, 1, 1] },
        ],
      },
    },
    {
      type: "markdown",
      text: "The median stayed flat, while the slowest 5% crept up over the last three weeks. Drag the slider under the chart, or its handles, to look at a shorter stretch.",
    },
    {
      type: "chart",
      summary: "Lines of the median and the slowest 5% of response times over sixty days; the slowest 5% rises in the last three weeks.",
      height: 360,
      option: {
        title: { text: "Response time", left: 0 },
        legend: { top: 28, left: 0 },
        tooltip: { trigger: "axis" },
        grid: { left: 56, right: 24, top: 72, bottom: 88 },
        xAxis: { type: "category", data: sixtyDays, boundaryGap: false },
        yAxis: { type: "value", name: "ms" },
        dataZoom: [{ type: "slider", xAxisIndex: 0, start: 35, end: 100 }],
        series: [
          { type: "line", name: "Median", smooth: true, showSymbol: false, data: medianMs },
          { type: "line", name: "Slowest 5%", smooth: true, showSymbol: false, data: slowestMs },
        ],
      },
    },
    {
      type: "markdown",
      text: "## Where the time goes\n\nA full build takes about **11 minutes**; the test stage is the largest share.",
    },
    {
      type: "chart",
      summary: "A ring of build minutes by stage: tests 5, compile 3, packaging 2, upload 1.",
      height: 300,
      option: {
        title: { text: "Minutes per stage", left: 0 },
        legend: { orient: "vertical", right: 0, top: "middle" },
        tooltip: { trigger: "item" },
        series: [{
          type: "pie",
          name: "Stage",
          radius: ["42%", "68%"],
          center: ["38%", "56%"],
          label: { show: false },
          emphasis: { focus: "self" },
          data: [
            { name: "Tests", value: 5 },
            { name: "Compile", value: 3 },
            { name: "Packaging", value: 2 },
            { name: "Upload", value: 1 },
          ],
        }],
      },
    },
    {
      type: "chart",
      summary: "A heatmap of failed builds by weekday and four-hour block: weekday daytime blocks have the most.",
      height: 360,
      option: {
        title: { text: "Failures by weekday and hour", left: 0 },
        tooltip: { trigger: "item" },
        grid: { left: 56, right: 16, top: 48, bottom: 100 },
        xAxis: { type: "category", data: HOURS, name: "Hour", nameLocation: "middle", nameGap: 28 },
        yAxis: { type: "category", data: DAYS, inverse: true },
        visualMap: { type: "continuous", min: 0, max: 10, calculable: true, orient: "horizontal", left: "center", bottom: 0 },
        series: [{ type: "heatmap", name: "Failures", data: failures }],
      },
    },
    {
      type: "markdown",
      text: "| Stage | Minutes | Share |\n|---|---:|---:|\n| Tests | 5 | 45% |\n| Compile | 3 | 27% |\n| Packaging | 2 | 18% |\n| Upload | 1 | 10% |",
    },
  ],
};

const state = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [hub],
  pages: { [hub.id]: [pageOf("pch-1", hub.id, buildHealth, 10)] },
});

export const reportChartScenarios: Scenario[] = [
  {
    id: "report-charts",
    group: "Report panel",
    category: "normal",
    title: "Page with charts between Markdown",
    description: "Four charts on one page at the panel's own width, each with Markdown around it: stacked bars, a line over sixty days, a ring and a heatmap. Hover for the tooltips (the bars' and the line's follow the pointer along the axis), click a legend name to hide a series, and drag the slider under the line to zoom. The colours follow the application's appearance: switch Light and Dark with the viewing controls to compare the series colours, the text and axis colours, the tooltip and the heatmap's blue scale.",
    state,
    steps: [
      async (ui) => {
        await ui.press(ui.session(hub.title));
        await ui.showPanel("report");
      },
    ],
  },
];
