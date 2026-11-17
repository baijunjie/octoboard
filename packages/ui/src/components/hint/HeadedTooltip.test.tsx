// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import { HeadedTooltip } from "./HeadedTooltip";

/** The heading's element, the one above the groove line. */
function headingOf(markup: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = markup;
  return host.querySelector("hr")!.previousElementSibling as HTMLElement;
}

it("lays out the heading, a groove line, then the children", () => {
  const host = document.createElement("div");
  host.innerHTML = renderToStaticMarkup(<HeadedTooltip heading="Main"><p>Body</p></HeadedTooltip>);
  const line = host.querySelector("hr")!;
  expect(line.previousElementSibling?.textContent).toBe("Main");
  expect(line.nextElementSibling?.textContent).toBe("Body");
});

it("sets the heading in medium weight by default", () => {
  expect(headingOf(renderToStaticMarkup(<HeadedTooltip heading="Main">Body</HeadedTooltip>)).classList.contains("font-medium")).toBe(true);
});

it("sets the heading in medium weight when emphasized", () => {
  expect(headingOf(renderToStaticMarkup(<HeadedTooltip heading="Main" emphasized>Body</HeadedTooltip>)).classList.contains("font-medium")).toBe(true);
});

it("leaves the heading in regular weight when not emphasized, still reading in its own direction", () => {
  const heading = headingOf(renderToStaticMarkup(<HeadedTooltip heading="Open in Claude app" emphasized={false}>Body</HeadedTooltip>));
  expect(heading.className).not.toMatch(/\bfont-(medium|semibold|bold)\b/);
  expect(heading.textContent).toBe("Open in Claude app");
  expect(heading.getAttribute("dir")).toBe("auto");
});
