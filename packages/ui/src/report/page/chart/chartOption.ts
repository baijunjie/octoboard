// What a page's chart hands ECharts: the agent's option with everything that could put a string into the window as
// HTML, load or open an address, or choose its own colours taken out, and the app's colours put in. The daemon refuses
// such an option already (`apps/daemon/src/protocol/page_content/chart.rs`, an allow-list); this is the renderer's own
// wall, a block-list of the keys that do those things at any depth, so that what reaches ECharts is safe whatever the
// stored page holds.

/** The colours of one appearance, from the UI's tokens where they can be read. */
export interface ChartLook {
  /** Primary text: titles, the legend, labels on the data. */
  text: string;
  /** Secondary text: axis labels and names, a subtitle. */
  muted: string;
  /** Axis lines and the grid's split lines. */
  line: string;
  tooltipBackground: string;
  tooltipText: string;
  tooltipBorder: string;
  /** The categorical palette, assigned to series (a pie's items) in order. */
  categorical: readonly string[];
  /** The sequential ramp a `visualMap` shades with, low to high. */
  sequential: readonly string[];
  /** Whether the user asked for reduced motion, which turns ECharts' animation off. */
  reducedMotion: boolean;
}

/**
 * The categorical palette, light and dark: the same eight hues, each stepped for its appearance's surface, checked
 * for colour-vision-deficiency separation between neighbours. A series' colour follows its order, never its rank.
 */
export const CATEGORICAL = {
  light: ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"],
  dark: ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"],
} as const;

/** The sequential ramp, one hue from near the surface to strong: light starts pale, dark starts deep. */
export const SEQUENTIAL = {
  light: ["#cde2fb", "#86b6ef", "#3987e5", "#1c5cab", "#0d366b"],
  dark: ["#184f95", "#256abf", "#3987e5", "#86b6ef", "#cde2fb"],
} as const;

/** The system font, which a drawing does not inherit. */
export const CHART_FONT = "ui-sans-serif, system-ui, sans-serif";

/**
 * Keys never passed to ECharts, at any depth. `formatter` (of every kind), `rich`, `renderMode` and `extraCssText`
 * are how ECharts writes a string as HTML; `link`, `sublink`, `toolbox` and `graphic` how it opens or loads an
 * address; colours, styles and fonts are the app's. The keys that would replace an object's prototype are dropped too.
 */
const DROPPED = new Set([
  "formatter", "valueFormatter", "rich", "renderMode", "extraCssText", "appendTo", "appendToBody", "className",
  "link", "sublink", "target", "subtarget", "toolbox", "graphic", "dataset", "geo", "map", "media", "timeline",
  "options", "baseOption", "aria", "color", "backgroundColor", "textStyle", "subtextStyle", "itemStyle", "lineStyle",
  "nameTextStyle", "borderColor", "shadowColor", "inRange", "outOfRange", "controller", "decal", "areaColor",
  "textBorderColor", "textShadowColor",
  "__proto__", "constructor", "prototype",
]);

// The room the header's parts take at the top of a drawing, in pixels, for the sizes `chartOption` gives their text.
const TITLE_HEIGHT = 30;
const SUBTEXT_HEIGHT = 22;
const LEGEND_HEIGHT = 28;
/** Between the header and the plotting area, and the more where a y axis names itself above its top. */
const GRID_GAP = 10;
const AXIS_NAME_ROOM = 30;
/** Below this width a drawing has no room beside its plot: a legend at its side moves under the title. */
export const NARROW_WIDTH = 480;

/** A string ECharts would read as an address to load: an image or path symbol. An object with an `image` key is a
 * pattern fill, which loads one too, so it goes whole. */
const ADDRESS = /^\s*(image|path):\/\//i;

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type JsonObject = { [key: string]: Json };

/** `value` without the dropped keys and address strings, at any depth: a fresh copy, so the page is not changed. */
export function withoutUnsafe(value: unknown): Json | undefined {
  if (value === null || typeof value === "boolean" || typeof value === "number") return value;
  if (typeof value === "string") return ADDRESS.test(value) ? undefined : value;
  if (Array.isArray(value)) return value.map(withoutUnsafe).filter((item): item is Json => item !== undefined);
  if (typeof value !== "object") return undefined;
  if ("image" in value) return undefined;
  const out: JsonObject = {};
  for (const [key, item] of Object.entries(value)) {
    if (DROPPED.has(key)) continue;
    const kept = withoutUnsafe(item);
    if (kept !== undefined) out[key] = kept;
  }
  return out;
}

function isObject(value: Json | undefined): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Applies `style` to each object of a component that is one object or an array of them. */
function each(value: Json | undefined, style: (item: JsonObject) => JsonObject): Json | undefined {
  if (Array.isArray(value)) return value.map((item) => (isObject(item) ? style(item) : item));
  return isObject(value) ? style(value) : value;
}

/** `item` with `key` merged over its own object (or set, where it had none). */
function withPart(item: JsonObject, key: string, part: JsonObject): JsonObject {
  const own = item[key];
  return { ...item, [key]: { ...(isObject(own) ? own : {}), ...part } };
}

/**
 * Stacks the title, the legend and the plotting area down the top of the drawing, so that none lies on another at any
 * width: the title at the top edge, a horizontal legend right under it (scrolling, not wrapping, where its names are
 * too many for the width), and the grid below both with room for the name of a y axis. Left and right stay the
 * option's own; what the option put on `top` or `bottom` of the title and legend gives way, and a grid's own `top`
 * only gives way when it is higher than the header needs. The labels of an axis are kept inside the grid's box.
 * Where `narrow`, a legend at the side (vertical) joins the stack too, and a pie or funnel is centred below the header,
 * so that the legend and the plot never share space.
 */
function withHeaderLayout(out: JsonObject, narrow: boolean): JsonObject {
  const { title, legend, grid } = out;
  let bottom = 0;
  if (isObject(title) && title.show !== false && (title.text || title.subtext)) {
    out.title = { ...title, top: 0, bottom: "auto" };
    bottom = title.subtext ? TITLE_HEIGHT + SUBTEXT_HEIGHT : TITLE_HEIGHT;
  }
  if (isObject(legend) && legend.show !== false && (narrow || legend.orient !== "vertical")) {
    const { right: _right, ...rest } = legend;
    out.legend = { ...rest, orient: "horizontal", left: legend.left ?? 0, type: "scroll", top: bottom, bottom: "auto" };
    bottom += LEGEND_HEIGHT;
    if (narrow) out.series = each(out.series, (item) => (item.type === "pie" || item.type === "funnel") ? { ...item, center: ["50%", "58%"] } : item) ?? null;
  }
  if (bottom === 0 || !("xAxis" in out || "yAxis" in out)) return out;
  const named = (axes: Json | undefined): boolean => [axes].flat().some((axis) => isObject(axis) && Boolean(axis.name));
  const top = bottom + (named(out.yAxis) ? AXIS_NAME_ROOM : GRID_GAP);
  out.grid = (each(grid ?? {}, (item) => ({
    ...item,
    top: typeof item.top === "number" ? Math.max(item.top, top) : top,
    containLabel: true,
  })) ?? null);
  return out;
}

/** The option ECharts is given for a page's chart, in the colours of `look`, for a drawing `width` pixels wide. */
export function chartOption(option: Record<string, unknown>, look: ChartLook, width = Infinity): JsonObject {
  const safe = withoutUnsafe(option);
  const base: JsonObject = isObject(safe) ? safe : {};
  const text = { color: look.text, fontFamily: CHART_FONT };
  const muted = { color: look.muted, fontFamily: CHART_FONT };
  const line = { lineStyle: { color: look.line } };
  const axis = (item: JsonObject, vertical: boolean): JsonObject => {
    let styled = withPart(item, "axisLabel", muted);
    styled = withPart(styled, "axisLine", line);
    styled = withPart(styled, "axisTick", line);
    styled = withPart(styled, "splitLine", line);
    // A y axis names itself over its own line: from the line towards the plot, so the name never runs off the edge.
    const name: JsonObject = vertical ? { ...muted, align: item.position === "right" ? "right" : "left" } : muted;
    return { ...styled, nameTextStyle: name };
  };
  const out: JsonObject = {
    ...base,
    color: [...look.categorical],
    backgroundColor: "transparent",
    textStyle: text,
    animation: !look.reducedMotion,
    aria: { enabled: false },
    tooltip: {
      ...(isObject(base.tooltip) ? base.tooltip : {}),
      // Drawn into the chart's own drawing, as text: an HTML tooltip would write its content into the window.
      renderMode: "richText",
      confine: true,
      backgroundColor: look.tooltipBackground,
      borderColor: look.tooltipBorder,
      textStyle: { color: look.tooltipText, fontFamily: CHART_FONT },
    },
  };
  const styled: Record<string, (item: JsonObject) => JsonObject> = {
    // Not heavier than a section's heading (semibold), which a chart sits under.
    title: (item) => ({ ...item, textStyle: { ...text, fontSize: 15, fontWeight: 600 }, subtextStyle: { ...muted, fontSize: 12 } }),
    legend: (item) => ({ ...item, textStyle: text, inactiveColor: look.line, pageTextStyle: muted }),
    xAxis: (item) => axis(item, false),
    yAxis: (item) => axis(item, true),
    radar: (item) => ({
      ...withPart(withPart(withPart(item, "axisName", muted), "axisLine", line), "splitLine", line),
      splitArea: { show: false },
    }),
    visualMap: (item) => ({ ...item, textStyle: muted, inRange: { color: [...look.sequential] } }),
    dataZoom: (item) => ({ ...item, textStyle: muted, borderColor: look.line }),
    series: (item) => {
      if (item.type === "line") return { ...item, lineStyle: { width: 2 } };
      if (item.type === "pie" || item.type === "funnel") return withPart(item, "label", { color: look.text });
      if (item.type === "gauge") return { ...withPart(item, "axisLabel", { color: look.muted }), detail: { color: look.text }, title: { color: look.muted } };
      return item;
    },
  };
  for (const [key, style] of Object.entries(styled)) {
    if (key in base) out[key] = each(base[key], style) ?? null;
  }
  return withHeaderLayout(out, width < NARROW_WIDTH);
}
