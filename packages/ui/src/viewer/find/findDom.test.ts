// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";

import { findPattern, DEFAULT_FIND_OPTIONS } from "./find";
import { findMatches, findRegions, hasPlaceholder, revealMatch } from "./findDom";

afterEach(() => {
  document.body.innerHTML = "";
});

function search(query: string, regions: Element[], limit = 100): { texts: string[]; capped: boolean } {
  const pattern = findPattern(query, DEFAULT_FIND_OPTIONS);
  if (pattern.kind !== "pattern") throw new Error("no pattern");
  const { ranges, capped } = findMatches(regions, pattern.pattern, limit);
  return { texts: ranges.map((range) => range.toString()), capped };
}

function region(html: string): HTMLElement {
  document.body.innerHTML = `<div data-find-region>${html}</div>`;
  return document.querySelector<HTMLElement>("[data-find-region]")!;
}

it("finds the regions marked as text to search, and nothing else in the viewer", () => {
  document.body.innerHTML = '<p>header</p><div data-find-region>code</div><img alt="x">';
  expect(findRegions(document.body)).toHaveLength(1);
});

it("returns the container itself first when it is a region, then the regions inside it", () => {
  document.body.innerHTML = '<div id="outer" data-find-region><div id="inner" data-find-region>code</div></div><p>header</p>';
  const outer = document.getElementById("outer")!;
  expect(findRegions(outer)).toEqual([outer, document.getElementById("inner")]);
});

it("returns only the container when it is a region with none inside, and nothing for a plain one", () => {
  document.body.innerHTML = '<pre id="only" data-find-region>text</pre><div id="plain"><span>x</span></div>';
  const only = document.getElementById("only")!;
  expect(findRegions(only)).toEqual([only]);
  expect(findRegions(document.getElementById("plain")!)).toEqual([]);
});

it("returns each region once when the container is a region and is searched", () => {
  const found = region("<pre>one</pre>");
  expect(findRegions(found)).toEqual([found]);
  expect(search("one", findRegions(found)).texts).toEqual(["one"]);
});

it("matches across the tokens of a highlighted line, but not from one line to the next", () => {
  const host = region("<div></div>").firstElementChild!;
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = `
    <div data-gutter><div data-column-number>1 foo</div></div>
    <div data-content>
      <div data-line="1"><span>const </span><span>foo</span><span> = 1;</span>\n</div>
      <div data-line="2"><span>bar</span>\n</div>
      <div data-separator><div data-unmodified-lines>3 foo lines</div></div>
    </div>`;
  expect(search("t foo =", [host.parentElement!]).texts).toEqual(["t foo ="]);
  expect(search("foo", [host.parentElement!]).texts).toEqual(["foo"]);
  expect(search("1;bar", [host.parentElement!]).texts).toEqual([]);
});

it("ranges over the right nodes when a match starts and ends at token borders", () => {
  const host = region("<div></div>").firstElementChild!;
  host.attachShadow({ mode: "open" }).innerHTML = '<div data-content><div data-line="1"><span>ab</span><span>cd</span><span>ef</span></div></div>';
  const [range] = findMatches([host.parentElement!], /cd/g, 10).ranges;
  expect(range.startContainer.textContent).toBe("cd");
  expect(range.endContainer.textContent).toBe("cd");
  expect(range.toString()).toBe("cd");
});

it("keeps a match inside one block of a rendered document, and skips what a reader does not see", () => {
  const found = region(
    '<p>one <em>two</em> three</p><p>four</p><span class="sr-only">hidden</span><svg><text>drawn</text></svg><span class="katex-mathml">spoken</span>',
  );
  expect(search("two three", [found]).texts).toEqual(["two three"]);
  expect(search("three four", [found]).texts).toEqual([]);
  for (const skipped of ["hidden", "drawn", "spoken"]) expect(search(skipped, [found]).texts).toEqual([]);
});

it("finds the code of a fenced block where it stands in a document", () => {
  const found = region("<p>before</p><div></div><p>after</p>");
  found.children[1].attachShadow({ mode: "open" }).innerHTML = '<div data-content><div data-line="1">fenced</div></div>';
  expect(search("e", [found]).texts.length).toBe(5);
  expect(search("fenced", [found]).texts).toEqual(["fenced"]);
});

it("does not search a placeholder of the viewer's drawn in a region", () => {
  const found = region("<div data-find-skip><div>Loading…</div></div><pre>text</pre>");
  expect(search("Loading", [found]).texts).toEqual([]);
  expect(search("text", [found]).texts).toEqual(["text"]);
});

it("searches plain text across its lines", () => {
  const found = region("<pre>one\ntwo\none</pre>");
  expect(search("one", [found]).texts).toEqual(["one", "one"]);
});

it("reports that it stopped at the limit", () => {
  const found = region("<pre>a a a a</pre>");
  expect(search("a", [found], 3)).toEqual({ texts: ["a", "a", "a"], capped: true });
  expect(search("a", [found], 4)).toEqual({ texts: ["a", "a", "a", "a"], capped: false });
});

it("scrolls plain text, one text node in its scroller, by the match's own box", () => {
  const scroller = region("one\ntwo\nthree");
  scroller.style.overflowY = "auto";
  Object.defineProperties(scroller, { clientHeight: { value: 100 }, scrollHeight: { value: 5000 } });
  const [range] = findMatches([scroller], /three/g, 10).ranges;
  // The third line, 40px down the text, 20px tall.
  range.getClientRects = () => [{ top: 40 - scroller.scrollTop, bottom: 60 - scroller.scrollTop, left: 0, right: 30, width: 30, height: 20 }] as unknown as DOMRectList;
  // Already where the window's middle is, from the top: 40 + 10 - 50.
  revealMatch(range);
  expect(scroller.scrollTop).toBe(0);
  range.getClientRects = () => [{ top: 400 - scroller.scrollTop, bottom: 420 - scroller.scrollTop, left: 0, right: 30, width: 30, height: 20 }] as unknown as DOMRectList;
  revealMatch(range);
  expect(scroller.scrollTop).toBe(360);
});

it("does not scroll to a match whose text a redraw has taken out, though the range lives on in what was around it", () => {
  const scroller = region("<pre><b>gone</b></pre>");
  scroller.style.overflowY = "auto";
  Object.defineProperties(scroller, { clientHeight: { value: 100 }, scrollHeight: { value: 5000 } });
  const [range] = findMatches([scroller], /gone/g, 10).ranges;
  range.getClientRects = () => [{ top: 900, bottom: 920, left: 0, right: 30, width: 30, height: 20 }] as unknown as DOMRectList;
  // The range collapses onto the connected `pre`, so it is no longer on any text.
  scroller.querySelector("pre")!.innerHTML = "";
  expect(range.startContainer.isConnected).toBe(true);
  revealMatch(range);
  expect(scroller.scrollTop).toBe(0);
});

it("knows a region that still shows a placeholder", () => {
  const found = region("<div data-find-skip>Loading…</div>");
  expect(hasPlaceholder([found])).toBe(true);
  expect(hasPlaceholder([region("<pre>text</pre>")])).toBe(false);
});
