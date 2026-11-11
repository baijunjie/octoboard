// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";
import * as echarts from "echarts/core";

import type { PageContent } from "../../protocol";
import { LAZY_IMPORT_TIMEOUT_MS, settleLazyImport } from "../../test/lazyImport";
import { renderPage } from "./pageTestSupport";

vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const HOSTILE = "<img src=x onerror=\"window.__chartPwned=1\">";

async function render(content: PageContent) {
  return (await renderPage(content)).host;
}

/** Waits for the chart, which loads on first use, to have drawn its SVG. */
const drawn = (host: ParentNode) => vi.waitFor(async () => {
  await settleLazyImport();
  expect(host.querySelector("[data-page-chart] svg")).not.toBeNull();
}, { timeout: LAZY_IMPORT_TIMEOUT_MS });

/** The texts the drawing shows. */
const texts = (host: ParentNode) => [...host.querySelectorAll("[data-page-chart] svg text")].map((node) => node.textContent ?? "");

it("puts every string of the option on the screen only as text, the tooltip's included", async () => {
  const host = await render({
    body: [{
      type: "chart",
      option: {
        title: { text: `Title ${HOSTILE}` },
        tooltip: { trigger: "item", formatter: HOSTILE, renderMode: "html" },
        legend: {},
        xAxis: { type: "category", data: [`Cat ${HOSTILE}`] },
        yAxis: { type: "value", name: `Axis ${HOSTILE}` },
        series: [{ type: "bar", name: `Series ${HOSTILE}`, data: [{ name: `Item ${HOSTILE}`, value: 3 }] }],
      },
    }],
  });
  await drawn(host);
  const chart = echarts.getInstanceByDom(host.querySelector<HTMLElement>("[data-page-chart]")!)!;
  await act(async () => chart.dispatchAction({ type: "showTip", seriesIndex: 0, dataIndex: 0 }));
  // The tooltip is drawn as text inside the chart once it shows: wait for it, not for the clock.
  await vi.waitFor(() => expect(texts(host).join("\n")).toContain(`Item ${HOSTILE}`), { timeout: LAZY_IMPORT_TIMEOUT_MS });

  const shown = texts(host).join("\n");
  for (const where of ["Title", "Cat", "Axis", "Series", "Item"]) expect(shown).toContain(`${where} ${HOSTILE}`);
  expect(host.querySelector("img, script, iframe, foreignObject, [onerror]")).toBeNull();
  // No tooltip, or anything else, is written into the window outside the drawing.
  const outside = [...host.querySelectorAll("[data-page-chart] *")].filter((element) =>
    !element.closest("svg") && [...element.childNodes].some((node) => node.nodeType === Node.TEXT_NODE && node.textContent!.trim() !== ""));
  expect(outside).toEqual([]);
  expect((window as { __chartPwned?: number }).__chartPwned).toBeUndefined();
});

it("is one image named by its summary, else its title, else the word for a chart", async () => {
  const pie = { series: [{ type: "pie", data: [{ name: "a", value: 1 }] }] };
  const host = await render({
    body: [
      { type: "chart", summary: "Builds rose", option: { ...pie, title: { text: "Builds" } } },
      { type: "chart", option: { ...pie, title: { text: "Builds" } } },
      { type: "chart", option: pie, height: 200 },
    ],
  });
  const names = [...host.querySelectorAll("[role=img]")].map((element) => element.getAttribute("aria-label"));
  expect(names).toEqual(["Builds rose", "Builds", "Chart"]);
  await drawn(host);
  expect(host.querySelectorAll<HTMLElement>("[data-page-chart]")[2]!.style.height).toBe("200px");
});
