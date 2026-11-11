import { matchOffsets } from "./find";
import { FIND_CURRENT_HIGHLIGHT, FIND_MATCH_HIGHLIGHT } from "./findHighlight";
import { codeBlocksOf } from "../code/rendererDom";

/** What marks an element the find bar searches in: the viewer's text surfaces, a file's code, a
 * diff or a rendered document. Anything else in the viewer (the header, notices, images) is not. */
export const FIND_REGION = { "data-find-region": "" } as const;
const FIND_REGION_SELECTOR = `[${Object.keys(FIND_REGION)[0]}]`;

/** Marks a placeholder drawn inside a region ("Loading…"), which is no text of the file. */
export const FIND_SKIP = { "data-find-skip": "" } as const;
const FIND_SKIP_SELECTOR = `[${Object.keys(FIND_SKIP)[0]}]`;

/** What the text of a document does not offer to a reader: scripts and styles, drawn diagrams, the
 * formula markup read aloud rather than seen, text hidden visually, placeholders of the viewer's. */
const NOT_TEXT = `script, style, svg, .katex-mathml, .sr-only, ${FIND_SKIP_SELECTOR}`;

/** The elements a rendered document's text runs are cut at, so a match never joins the end of one
 * paragraph to the start of the next. */
const BLOCKS = "p, li, h1, h2, h3, h4, h5, h6, td, th, tr, pre, blockquote, dt, dd, figcaption, section, details, summary, div";

/** A run of text a match cannot leave: a line of code, or a block of a document. `starts[i]` is where
 * `nodes[i]` begins in `text`. */
interface TextBlock {
  nodes: Text[];
  starts: number[];
  text: string;
}

function textBlock(nodes: Text[]): TextBlock {
  const starts: number[] = [];
  let text = "";
  for (const node of nodes) {
    starts.push(text.length);
    text += node.data;
  }
  return { nodes, starts, text };
}

/** A walk over the elements and text under `node`, not entering an element that `skip` matches. */
function textWalker(node: Node, skip: string): TreeWalker {
  return node.ownerDocument!.createTreeWalker(node, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: (candidate) => (candidate.nodeType === Node.ELEMENT_NODE && (candidate as Element).matches(skip) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
}

/** The text nodes under `node` that are text a reader sees, in order. */
function textNodesIn(node: Node, skip = NOT_TEXT): Text[] {
  const walker = textWalker(node, skip);
  const found: Text[] = [];
  for (let current = walker.nextNode(); current; current = walker.nextNode()) {
    if (current.nodeType === Node.TEXT_NODE) found.push(current as Text);
  }
  return found;
}

/** The text blocks of one shadow root: the lines of its code. */
function codeBlocks(root: ShadowRoot): TextBlock[] {
  return codeBlocksOf(root).map((element) => textBlock(textNodesIn(element, `${NOT_TEXT}, [data-separator]`)));
}

/** The text blocks of a region's own document (light DOM), with the code of every shadow root in it
 * (a fenced block of a rendered document) at its place. */
function regionBlocks(region: Element): TextBlock[] {
  const blocks: TextBlock[] = [];
  let run: Text[] = [];
  let runBlock: Element | null | undefined;
  const flush = () => {
    if (run.length > 0) blocks.push(textBlock(run));
    run = [];
    runBlock = undefined;
  };
  const walker = textWalker(region, NOT_TEXT);
  for (let current = walker.nextNode(); current; current = walker.nextNode()) {
    if (current.nodeType === Node.ELEMENT_NODE) {
      const root = (current as Element).shadowRoot;
      if (root) {
        flush();
        blocks.push(...codeBlocks(root));
      }
      continue;
    }
    const block = current.parentElement?.closest(BLOCKS) ?? region;
    if (run.length > 0 && block !== runBlock) flush();
    runBlock = block;
    run.push(current as Text);
  }
  flush();
  return blocks;
}

/** The viewer's text surfaces under `container`, in order, and `container` itself when it is one. */
export function findRegions(container: ParentNode): HTMLElement[] {
  const within = Array.from(container.querySelectorAll<HTMLElement>(FIND_REGION_SELECTOR));
  return container instanceof HTMLElement && container.matches(FIND_REGION_SELECTOR) ? [container, ...within] : within;
}

/** The range covering `[start, end)` of a block's text. */
function rangeIn(block: TextBlock, start: number, end: number): Range | undefined {
  const at = (offset: number, isEnd: boolean) => {
    for (let i = block.nodes.length - 1; i >= 0; i--) {
      const from = block.starts[i];
      if (isEnd ? offset > from : offset >= from) return { node: block.nodes[i], offset: offset - from };
    }
    return undefined;
  };
  const first = at(start, false);
  const last = at(end, true);
  if (!first || !last) return undefined;
  const range = first.node.ownerDocument.createRange();
  range.setStart(first.node, first.offset);
  range.setEnd(last.node, last.offset);
  return range;
}

/** The matches of `pattern` in the text of `regions`, as ranges in reading order, and whether the
 * search stopped at `limit` with more to find. */
export function findMatches(regions: Element[], pattern: RegExp, limit: number): { ranges: Range[]; capped: boolean } {
  const ranges: Range[] = [];
  for (const region of regions) {
    for (const block of regionBlocks(region)) {
      for (const [start, end] of matchOffsets(pattern, block.text, limit - ranges.length + 1)) {
        if (ranges.length === limit) return { ranges, capped: true };
        const range = rangeIn(block, start, end);
        if (range) ranges.push(range);
      }
    }
  }
  return { ranges, capped: false };
}

/** Whether this browser can paint ranges without touching the DOM or the selection. */
export function canHighlight(): boolean {
  return typeof Highlight !== "undefined" && typeof CSS !== "undefined" && "highlights" in CSS;
}

/** Whether a range still lies on text in the page. A redraw takes out the text a stale range was made on:
 * the range then collapses onto what was around it, which may still be in the page. */
function isLive(range: Range | undefined): range is Range {
  return range !== undefined && range.startContainer.isConnected && !range.collapsed;
}

/** Whether a region still holds a placeholder (`FIND_SKIP`) where its text is to be drawn, so what
 * is searched in it now is not the text of the file. */
export function hasPlaceholder(regions: Element[]): boolean {
  return regions.some((region) => region.querySelector(FIND_SKIP_SELECTOR));
}

/** Paints the matches, and the current one over them. */
export function paintMatches(ranges: Range[], current: number): void {
  if (!canHighlight()) return;
  const match = new Highlight(...ranges.filter((range, index) => index !== current && isLive(range)));
  const now = new Highlight(...ranges.filter((range, index) => index === current && isLive(range)));
  now.priority = 1;
  CSS.highlights.set(FIND_MATCH_HIGHLIGHT, match);
  CSS.highlights.set(FIND_CURRENT_HIGHLIGHT, now);
}

/** Takes the find highlights off the page. */
export function clearMatches(): void {
  if (!canHighlight()) return;
  CSS.highlights.delete(FIND_MATCH_HIGHLIGHT);
  CSS.highlights.delete(FIND_CURRENT_HIGHLIGHT);
}

/** The element a node sits in, stepping out of a shadow root to its host. */
function parentAcross(node: Node): Element | null {
  const parent = node.parentNode;
  return parent instanceof ShadowRoot ? parent.host : parent instanceof Element ? parent : null;
}

/** Scrolls the match to the middle of each scroller around it, inside the region: of the part the
 * scroller leaves visible below its scroll padding, which is where the find bar's gap lies. The
 * match's own box is what is placed, so it works in a scroller whose text is one text node. */
export function revealMatch(range: Range): void {
  if (!isLive(range)) return;
  for (let scroller = parentAcross(range.startContainer); scroller; scroller = scroller.matches(FIND_REGION_SELECTOR) ? null : parentAcross(scroller)) {
    // Measured again at each scroller, as the one inside it has moved the match.
    const rect = range.getClientRects?.()[0] ?? range.getBoundingClientRect?.();
    if (!rect) return;
    const style = getComputedStyle(scroller);
    const box = scroller.getBoundingClientRect();
    if (/auto|scroll/.test(style.overflowY) && scroller.scrollHeight > scroller.clientHeight) {
      const pad = parseFloat(style.scrollPaddingTop) || 0;
      const middle = pad + (scroller.clientHeight - pad) / 2;
      scroller.scrollTop = Math.max(0, rect.top - box.top + scroller.scrollTop + rect.height / 2 - middle);
    }
    if (/auto|scroll/.test(style.overflowX) && scroller.scrollWidth > scroller.clientWidth && (rect.left < box.left || rect.right > box.right)) {
      scroller.scrollLeft += rect.left - box.left + rect.width / 2 - scroller.clientWidth / 2;
    }
  }
}
