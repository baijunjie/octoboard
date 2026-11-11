// @vitest-environment jsdom
// In a file of its own: whether the chart renderer has been loaded is a fact about the module registry, which a test
// that draws a chart before this one leaves changed.
import { expect, it, vi } from "vitest";

import type { PageContent } from "../../protocol";
import { LAZY_IMPORT_TIMEOUT_MS, settleLazyImport } from "../../test/lazyImport";
import { renderPage } from "./pageTestSupport";

const loaded = vi.hoisted(() => vi.fn());
vi.mock("./chart/PageChart", async (importOriginal) => {
  loaded();
  return importOriginal();
});
vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function render(content: PageContent) {
  return (await renderPage(content)).host;
}

/** Waits for the chart, which loads on first use, to have drawn its SVG. */
const drawn = (host: ParentNode) => vi.waitFor(async () => {
  await settleLazyImport();
  expect(host.querySelector("[data-page-chart] svg")).not.toBeNull();
}, { timeout: LAZY_IMPORT_TIMEOUT_MS });

it("loads the chart renderer only for a page that carries a chart", async () => {
  await render({ body: [{ type: "markdown", text: "No chart here" }] });
  await settleLazyImport();
  expect(loaded).not.toHaveBeenCalled();

  const charted = await render({ body: [{ type: "chart", option: { series: [{ type: "pie", data: [{ name: "a", value: 1 }] }] } }] });
  await drawn(charted);
  expect(loaded).toHaveBeenCalledTimes(1);
});

