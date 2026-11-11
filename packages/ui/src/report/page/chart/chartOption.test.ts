import { describe, expect, it } from "vitest";

import { CATEGORICAL, NARROW_WIDTH, SEQUENTIAL, chartOption, withoutUnsafe, type ChartLook } from "./chartOption";

const LOOK: ChartLook = {
  text: "rgb(1, 1, 1)", muted: "rgb(2, 2, 2)", line: "rgb(3, 3, 3)",
  tooltipBackground: "rgb(4, 4, 4)", tooltipText: "rgb(5, 5, 5)", tooltipBorder: "rgb(6, 6, 6)",
  categorical: CATEGORICAL.dark, sequential: SEQUENTIAL.dark, reducedMotion: false,
};

describe("withoutUnsafe", () => {
  it("drops, at any depth, every key that writes HTML, opens or loads an address, or styles, and address strings", () => {
    const option = {
      tooltip: { trigger: "axis", formatter: "<img src=x onerror=alert(1)>", renderMode: "html", extraCssText: "x", valueFormatter: "v" },
      title: { text: "T", link: "https://example.com", sublink: "https://example.com", textStyle: { color: "red" } },
      toolbox: { feature: { dataView: {} } },
      graphic: [{ type: "image" }],
      color: ["#f00"],
      backgroundColor: "#000",
      series: [{ type: "line", name: "s", symbol: "image://https://example.com/x.png", label: { show: true, formatter: "{b}", rich: {} }, itemStyle: { color: "red" }, data: [1, "-", null] }],
    };
    expect(withoutUnsafe(option)).toEqual({
      tooltip: { trigger: "axis" },
      title: { text: "T" },
      series: [{ type: "line", name: "s", label: { show: true }, data: [1, "-", null] }],
    });
  });

  it("drops a whole object that carries an image (a pattern fill loads one) and the colours of a label's text", () => {
    const option = {
      series: [{ type: "bar", data: [{ value: 1, itemStyle: { color: { image: "https://example.com/p.png", repeat: "repeat" } } }, { value: 2, tag: { image: "x" }, kept: true }], label: { show: true, textBorderColor: "red", textShadowColor: "blue" }, areaStyle: { color: "red", opacity: 0.4 } }],
    };
    expect(withoutUnsafe(option)).toEqual({
      series: [{ type: "bar", data: [{ value: 1 }, { value: 2, kept: true }], label: { show: true }, areaStyle: { opacity: 0.4 } }],
    });
  });

  it("does not let a key replace an object's prototype, and leaves the page's own option unchanged", () => {
    const option = JSON.parse('{"legend": {"__proto__": {"polluted": true}, "show": true}}') as Record<string, unknown>;
    const copy = withoutUnsafe(option) as { legend: Record<string, unknown>; series: unknown };
    expect(copy.legend).toEqual({ show: true });
    expect(Object.getPrototypeOf(copy.legend)).toBe(Object.prototype);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect(option.legend).toHaveProperty("show", true);
  });
});

describe("chartOption", () => {
  it("draws the tooltip as text inside the chart, whatever the option asked for", () => {
    const out = chartOption({ tooltip: { trigger: "item", renderMode: "html", formatter: "<b>x</b>" }, series: [] }, LOOK);
    expect(out.tooltip).toMatchObject({ trigger: "item", renderMode: "richText", confine: true, backgroundColor: LOOK.tooltipBackground });
    expect(out.tooltip).not.toHaveProperty("formatter");
    expect(out.aria).toEqual({ enabled: false });
  });

  it("puts in the appearance's palette and text colours, never the option's", () => {
    const out = chartOption({
      color: ["#f00"],
      title: { text: "T" },
      legend: {},
      xAxis: [{ type: "category" }, { type: "value", axisLabel: { rotate: 30 } }],
      yAxis: { type: "value" },
      visualMap: { min: 0, max: 1, inRange: { color: ["#000"] } },
      series: [{ type: "line", data: [1] }, { type: "pie", data: [], label: { show: true } }],
    }, LOOK);
    expect(out.color).toEqual(CATEGORICAL.dark);
    expect(out.backgroundColor).toBe("transparent");
    expect(out.title).toMatchObject({ text: "T", textStyle: { color: LOOK.text }, subtextStyle: { color: LOOK.muted } });
    expect(out.legend).toMatchObject({ textStyle: { color: LOOK.text } });
    expect(out.xAxis).toMatchObject([
      { type: "category", axisLabel: { color: LOOK.muted }, axisLine: { lineStyle: { color: LOOK.line } } },
      { type: "value", axisLabel: { rotate: 30, color: LOOK.muted } },
    ]);
    expect(out.yAxis).toMatchObject({ splitLine: { lineStyle: { color: LOOK.line } } });
    expect(out.visualMap).toMatchObject({ inRange: { color: SEQUENTIAL.dark } });
    expect(out.series).toMatchObject([{ type: "line", lineStyle: { width: 2 } }, { type: "pie", label: { show: true, color: LOOK.text } }]);
  });

  it("adds no component the option did not have, and turns animation off for reduced motion", () => {
    const out = chartOption({ series: [{ type: "pie", data: [] }] }, { ...LOOK, reducedMotion: true });
    expect(out).not.toHaveProperty("xAxis");
    expect(out).not.toHaveProperty("legend");
    expect(out.animation).toBe(false);
  });

  it("stacks the title, the legend and the grid down the top, whatever positions the option gave", () => {
    const out = chartOption({
      title: { text: "T", top: 200, bottom: 0 },
      legend: { top: 4, type: "plain" },
      grid: { left: 56, top: 10 },
      yAxis: { type: "value", name: "ms" },
      xAxis: { type: "category" },
      series: [],
    }, LOOK) as { title: { top: number }; legend: { top: number; type: string }; grid: { top: number; left: number; containLabel: boolean }; yAxis: { nameTextStyle: { align: string } } };
    expect(out.title.top).toBe(0);
    expect(out.legend).toMatchObject({ type: "scroll", top: out.title.top + 30 });
    expect(out.grid).toMatchObject({ left: 56, containLabel: true });
    expect(out.grid.top).toBeGreaterThanOrEqual(out.legend.top + 28 + 30);
    expect(out.yAxis.nameTextStyle.align).toBe("left");
  });

  it("keeps a grid that sits lower than the header needs, and lays out nothing without a header or axes", () => {
    const low = chartOption({ title: { text: "T" }, xAxis: {}, yAxis: {}, grid: { top: 200 }, series: [] }, LOOK);
    expect(low.grid).toMatchObject({ top: 200 });
    const pie = chartOption({ title: { text: "T" }, series: [{ type: "pie", data: [] }] }, LOOK);
    expect(pie).not.toHaveProperty("grid");
    const bare = chartOption({ xAxis: {}, yAxis: {}, series: [] }, LOOK);
    expect(bare).not.toHaveProperty("grid");
  });

  it("moves a legend at the side under the title where the drawing is narrow, and centres a pie below it", () => {
    const option = { title: { text: "T" }, legend: { orient: "vertical", right: 0, top: "middle" }, series: [{ type: "pie", center: ["38%", "56%"], data: [] }] };
    const wide = chartOption(option, LOOK, 900);
    expect(wide.legend).toMatchObject({ orient: "vertical", right: 0, top: "middle" });
    expect(wide.series).toMatchObject([{ center: ["38%", "56%"] }]);
    const narrow = chartOption(option, LOOK, 320) as { legend: Record<string, unknown>; series: unknown };
    expect(narrow.legend).toMatchObject({ orient: "horizontal", type: "scroll", left: 0, top: 30 });
    expect(narrow.legend).not.toHaveProperty("right");
    expect(narrow.series).toMatchObject([{ center: ["50%", "58%"] }]);
  });
  type Laid = { title?: { top: number }; legend: Record<string, unknown> & { top: number }; grid?: { top: number }; series: { center?: string[] }[] };

  it("changes the layout below 480 pixels and not at it", () => {
    expect(NARROW_WIDTH).toBe(480);
    const option = { title: { text: "T" }, legend: { orient: "vertical", right: 0 }, series: [{ type: "pie", data: [] }] };
    expect(chartOption(option, LOOK, 480).legend).toMatchObject({ orient: "vertical", right: 0 });
    expect(chartOption(option, LOOK, 479).legend).toMatchObject({ orient: "horizontal" });
  });

  it("centres a funnel below the header of a narrow drawing as it does a pie, and leaves the other series where they are", () => {
    const option = {
      title: { text: "T" },
      legend: { orient: "vertical", right: 0 },
      xAxis: {}, yAxis: {},
      series: [{ type: "funnel", data: [] }, { type: "pie", data: [] }, { type: "bar", data: [] }],
    };
    const narrow = chartOption(option, LOOK, 300) as unknown as Laid;
    expect(narrow.series[0].center).toEqual(["50%", "58%"]);
    expect(narrow.series[1].center).toEqual(["50%", "58%"]);
    expect(narrow.series[2]).not.toHaveProperty("center");
    const wide = chartOption(option, LOOK, 900) as unknown as Laid;
    expect(wide.series[0]).not.toHaveProperty("center");
  });

  it("never lets the narrow stack overlap: the legend lies below the title and its subtitle, and the grid below the legend", () => {
    const option = {
      title: { text: "T", subtext: "sub" },
      legend: { orient: "vertical", right: 0, top: "middle" },
      xAxis: { type: "category" }, yAxis: { type: "value" },
      series: [{ type: "bar", data: [1] }],
    };
    const plain = chartOption({ ...option, title: { text: "T" } }, LOOK, 320) as unknown as Laid;
    const withSub = chartOption(option, LOOK, 320) as unknown as Laid;
    expect(withSub.title?.top).toBe(0);
    // A subtitle takes room of its own, which the legend and the grid make way for.
    expect(withSub.legend.top).toBeGreaterThan(plain.legend.top);
    expect(withSub.grid!.top).toBeGreaterThan(withSub.legend.top);
    expect(plain.grid!.top).toBeGreaterThan(plain.legend.top);
    expect(plain.legend.top).toBeGreaterThan(plain.title!.top);
  });

  it("puts a legend with no title at the top edge, with the grid under it", () => {
    const out = chartOption({ legend: { top: "bottom" }, xAxis: {}, yAxis: {}, series: [] }, LOOK) as unknown as Laid;
    expect(out.legend.top).toBe(0);
    expect(out.grid!.top).toBeGreaterThan(0);
  });

  it("leaves a legend the page hides, and a title the page hides, out of the stack", () => {
    const out = chartOption({ title: { text: "T", show: false }, legend: { show: false }, xAxis: {}, yAxis: {}, series: [] }, LOOK);
    expect(out).not.toHaveProperty("grid");
    expect(out.legend).toMatchObject({ show: false });
  });
});
