// The direction of a block of the Markdown document, worked out here rather than left to `dir="auto"`.
// The engine's rule for `auto` looks at the container's own text and stops at any descendant that
// carries a `dir` of its own, so a list whose items are paragraphs, or a quote of paragraphs, has no
// text of its own to decide by; such a container cannot resolve, and an unresolved `auto` falls back
// to left to right, it does not inherit. WebKit also pays for it: a large document of such
// containers takes time quadratic in its size to lay out. Here a block takes the direction of the
// first strong character anywhere in its text, descendants included, and is given it explicitly.

export type Direction = "ltr" | "rtl";

/** The part of a syntax tree node that the direction reads; the library's nodes satisfy it. */
export interface DirectionNode {
  type: string;
  value?: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: DirectionNode[];
}

/** The directional marks and isolate controls the scan acts on, by code point: written out, they
 * are invisible in the source, and an editor or a paste can drop one without leaving a trace. */
const LEFT_TO_RIGHT_MARK = String.fromCodePoint(0x200e);
const RIGHT_TO_LEFT_MARK = String.fromCodePoint(0x200f);
const ARABIC_LETTER_MARK = String.fromCodePoint(0x061c);
/** Left-to-right, right-to-left and first-strong isolate. */
const ISOLATE_OPENERS = [0x2066, 0x2067, 0x2068].map((point) => String.fromCodePoint(point));
const POP_ISOLATE = String.fromCodePoint(0x2069);

/** Every character that can decide or shift the scan: a letter, a directional mark, an isolate
 * control. Anything else is neutral or weak and is never looked at. */
const RELEVANT = new RegExp(
  `[\\p{L}${LEFT_TO_RIGHT_MARK}${RIGHT_TO_LEFT_MARK}${ARABIC_LETTER_MARK}${ISOLATE_OPENERS.join("")}${POP_ISOLATE}]`,
  "gu",
);

/** The scripts written right to left that a document is likely to be in (rarer ones are left
 * out, and a letter of one takes left to right). Digits of these scripts are weak, not strong, so
 * only letters are tested against it. */
const RIGHT_TO_LEFT_LETTER =
  /[\p{Script=Hebrew}\p{Script=Arabic}\p{Script=Syriac}\p{Script=Thaana}\p{Script=Nko}\p{Script=Samaritan}\p{Script=Mandaic}\p{Script=Adlam}\p{Script=Hanifi_Rohingya}]/u;

/** What a scan of some text answers: the direction it found, or, when it found no strong character,
 * how many isolates are still open at its end (the next piece of text goes on inside them). */
type Scan = Direction | number;

function scanText(text: string, depth: number): Scan {
  RELEVANT.lastIndex = 0;
  for (let match = RELEVANT.exec(text); match; match = RELEVANT.exec(text)) {
    const character = match[0];
    if (ISOLATE_OPENERS.includes(character)) depth += 1;
    else if (character === POP_ISOLATE) depth = Math.max(0, depth - 1);
    else if (depth === 0) {
      if (character === LEFT_TO_RIGHT_MARK) return "ltr";
      if (character === RIGHT_TO_LEFT_MARK || character === ARABIC_LETTER_MARK) return "rtl";
      return RIGHT_TO_LEFT_LETTER.test(character) ? "rtl" : "ltr";
    }
  }
  return depth;
}

// Each node's scan from outside any isolate, kept so that a block and the blocks inside it share the
// work: the text is walked once however deep it is nested, and the cost stays linear in the document.
const scans = new WeakMap<DirectionNode, Scan>();

function scan(node: DirectionNode, depth: number): Scan {
  if (node.type === "text") return scanText(node.value ?? "", depth);
  if (node.type === "element") {
    // The content of an element with a direction of its own is that element's, not its parent's;
    // code is always drawn left to right.
    if (typeof node.properties?.dir === "string" || node.tagName === "code") return depth;
    // An image is drawn as its description.
    if (node.tagName === "img") return scanText(String(node.properties?.alt ?? ""), depth);
  }
  if (!node.children) return depth;
  if (depth === 0) {
    const known = scans.get(node);
    if (known !== undefined) return known;
  }
  let inside = depth;
  let found: Scan = depth;
  for (const child of node.children) {
    found = scan(child, inside);
    if (typeof found === "string") break;
    inside = found;
  }
  if (depth === 0) scans.set(node, found);
  return found;
}

/** The direction of the first strong character in the whole text of `node`, descendants included
 * (UAX #9 rules P2 and P3): neutral and weak characters are passed over, and so is anything inside
 * an isolate, inside an element that has a `dir` of its own, and inside code. Undefined when there is none. */
export function firstStrongDirection(node: DirectionNode): Direction | undefined {
  const found = scan(node, 0);
  return typeof found === "string" ? found : undefined;
}
