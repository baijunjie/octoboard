// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { RENDER_BUDGETS } from "../code/budgets";
import { ViewerHost } from "../viewerTestSupport";
import { MarkdownDocument } from "./MarkdownDocument";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function render(text: string) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const openLink = vi.fn();
  const openFile = vi.fn();
  const renderCode = vi.fn(() => null);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(
    <ViewerHost>
      <MarkdownDocument text={text} path="docs/README.md" theme="light" renderCode={renderCode} onOpenLink={openLink} onOpenFile={openFile} />
    </ViewerHost>,
  ));
  return { container, openLink, openFile, renderCode };
}

it("renders centered HTML badges with dimensions and routes their links through the viewer", async () => {
  const { container, openLink, openFile } = await render(`
<p align="center"><a href="https://example.com/releases"><img src="https://example.com/badge.svg?a=1&amp;b=2" alt="Version" width="120" height="18" title="Release" /></a>
<a href="../LICENSE">License</a></p>
`);
  const paragraph = container.querySelector("p")!;
  expect(paragraph.style.textAlign).toBe("center");
  const image = paragraph.querySelector("img")!;
  expect(image.getAttribute("src")).toBe("https://example.com/badge.svg?a=1&b=2");
  expect([image.alt, image.width, image.height]).toEqual(["Version", 120, 18]);
  expect(image.hasAttribute("title")).toBe(false);
  expect(image.getAttribute("referrerpolicy")).toBe("no-referrer");
  const links = paragraph.querySelectorAll("a");
  await act(async () => links[0].click());
  expect(openLink).toHaveBeenCalledExactlyOnceWith("https://example.com/releases");
  act(() => links[1].click());
  expect(openFile).toHaveBeenCalledExactlyOnceWith("LICENSE", undefined);
});

it.each(["https://example.com/releases", "../LICENSE", "#note"])("keeps document links plain, including image links to %s with whitespace", async (href) => {
  const { container } = await render(`[Website](https://example.com)\n\n<a href="${href}">\n<img src="https://example.com/badge.svg" alt="Version">\n</a>\n\n<p id="note">Note</p>`);
  const links = container.querySelectorAll("a");
  expect(links).toHaveLength(2);
  expect(links[0].textContent).toBe("Website");
  expect(links[1].querySelector("img")).not.toBeNull();
  expect(container.querySelector("a svg")).toBeNull();
  expect(links[1].style.textDecoration).toBe("none");
});

it("preserves plain HTML preformatted text without invoking a code renderer", async () => {
  const { container, renderCode } = await render("<pre>first\n  second</pre>");
  expect(container.querySelector("pre")?.textContent).toBe("first\n  second");
  expect(renderCode).not.toHaveBeenCalled();
});

it("counts image descriptions in raw HTML formula sources against their character budget", async () => {
  const source = "x".repeat(RENDER_BUDGETS.documentFormulaChars + 1);
  const { container } = await render(`<code class="math-inline"><img alt="${source}"></code>`);
  expect(container.querySelector(".katex")).toBeNull();
  expect(container.textContent).toContain(source);
  expect(container.querySelector('[aria-label="Shown as source because the document is large."]')).not.toBeNull();
});

it("budgets HTML code by the first language class actually used to render it", async () => {
  const source = "x".repeat(RENDER_BUDGETS.documentFenceChars + 1);
  const { renderCode } = await render(`<pre><code class="language-ts language-mermaid"><img alt="${source}"></code></pre>`);
  expect(renderCode).toHaveBeenCalledExactlyOnceWith({ language: "ts", text: source, highlight: false });
});

it("removes executable HTML, unsafe attributes, app classes and secondary image sources", async () => {
  const { container } = await render(`
<script>alert(1)</script>
<style>body { display: none }</style>
<iframe src="https://example.com"></iframe>
<object data="https://example.com"></object>
<svg onload="alert(1)"><script>alert(1)</script></svg>
<p class="hidden" style="position:fixed" onclick="alert(1)" title="Native tooltip">
<a href="javascript:alert(1)">Unsafe link</a>
<picture><source srcset="https://example.com/extra.png"><img src="https://example.com/safe.png" srcset="https://example.com/extra.png 2x" onerror="alert(1)" alt="Safe image"></picture>
</p>
`);
  expect(container.querySelector("script, style, iframe, object, svg, source, [srcset], [onclick], [onerror], .hidden")).toBeNull();
  expect(container.querySelector('a[href^="javascript:"]')).toBeNull();
  expect(container.querySelector("p")?.hasAttribute("style")).toBe(false);
  expect(container.querySelector("p[title]")).toBeNull();
  expect(container.querySelector("img")?.getAttribute("src")).toBe("https://example.com/safe.png");
  expect(container.textContent).toContain("Unsafe link");
});

it.each(["javascript:alert(1)", "data:image/svg+xml,test", "file:///secret.png", "//example.com/a.png", "../../secret.png", "mailto:x@example.com"])("does not load the image address %s", async (address) => {
  const { container } = await render(`<img src="${address}" alt="Refused image">`);
  expect(container.querySelector("img")).toBeNull();
  expect(container.textContent).toContain("Refused image");
});

it("shares the image count between HTML and Markdown, without trusting authored budget flags", async () => {
  const html = Array.from({ length: RENDER_BUDGETS.documentImages }, (_, i) => `<img src="https://example.com/${i}.png" alt="${i}">`).join("");
  const { container } = await render(`${html}\n\n![Over budget](https://example.com/last.png)\n\n<img src="https://example.com/extra.png" alt="Extra" data-over-budget="false">`);
  expect(container.querySelectorAll("img")).toHaveLength(RENDER_BUDGETS.documentImages);
  expect(container.textContent).toContain("Over budget");
  expect(container.textContent).toContain("Extra");
});

it("keeps authored fragment links working without exposing unprefixed HTML ids", async () => {
  const { container } = await render('<p><a href="#note">Jump</a></p>\n<p id="note">Note</p>');
  const paragraph = container.querySelector<HTMLElement>("p[id]")!;
  expect(paragraph.id).toBe("user-content-note");
  expect(container.querySelector("a")?.getAttribute("href")).toBe(`#${paragraph.id}`);
});

it("shows the existing image failure notice when a remote image fails", async () => {
  const { container } = await render('<img src="https://example.com/missing.png" alt="Missing">');
  const image = container.querySelector("img")!;
  act(() => image.dispatchEvent(new Event("error")));
  expect(container.querySelector("img")).toBeNull();
  expect(container.textContent).toContain("Missing");
  expect(container.querySelector('[aria-label="This image could not be shown."]')).not.toBeNull();
});
