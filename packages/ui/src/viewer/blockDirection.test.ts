import { expect, it } from "vitest";

import { type DirectionNode, firstStrongDirection } from "./blockDirection";

// The directional controls the cases are written with, by code point: written out they are
// invisible, and a case would then read as though it held nothing special.
const [LRM, RLM, ALM, LRI, RLI, FSI, PDI] = [0x200e, 0x200f, 0x061c, 0x2066, 0x2067, 0x2068, 0x2069].map((point) =>
  String.fromCodePoint(point),
);

const text = (value: string): DirectionNode => ({ type: "text", value });
const element = (tagName: string, children: DirectionNode[], properties: Record<string, unknown> = {}): DirectionNode => ({
  type: "element",
  tagName,
  properties,
  children,
});

it.each([
  ["an Arabic first letter", "مرحبا hello", "rtl"],
  ["a Hebrew first letter", "שלום hello", "rtl"],
  ["a Latin first letter", "hello مرحبا", "ltr"],
  ["Han alone, which is strong left to right", "你好", "ltr"],
  ["digits and punctuation before the first strong letter", "1. (2) — مرحبا", "rtl"],
  ["Arabic-Indic digits, which are weak", "١٢٣ hello", "ltr"],
  ["a right-to-left mark", `${RLM}x`, "rtl"],
  ["an Arabic letter mark", `${ALM}x`, "rtl"],
  ["a left-to-right mark before an Arabic letter", `${LRM}مرحبا`, "ltr"],
  ["an empty text", "", undefined],
  ["no strong character", "123 - (4)", undefined],
  ["an isolated run, which is skipped", `${RLI}مرحبا${PDI} hello`, "ltr"],
  ["a nested isolate, skipped as a whole", `${LRI}a${RLI}b${PDI}c${PDI} مرحبا`, "rtl"],
  ["an isolate that never closes", `${FSI}hello`, undefined],
  ["a closer with nothing open", `${PDI}مرحبا`, "rtl"],
])("reads %s", (_, value, direction) => {
  expect(firstStrongDirection(text(value))).toBe(direction);
});

it.each([
  ["a strong letter only in a nested element", element("ul", [element("li", [element("p", [text("12"), element("em", [text("مرحبا")])])])]), "rtl"],
  ["the first strong letter across siblings", element("p", [text("1 "), element("strong", [text("a")]), text("مرحبا")]), "ltr"],
  ["an isolate that opens in one node and closes in another", element("p", [text(`${RLI}`), element("em", [text("مرحبا")]), text(`${PDI} a`)]), "ltr"],
  ["an element with a direction of its own, skipped", element("p", [element("span", [text("a")], { dir: "ltr" }), text("مرحبا")]), "rtl"],
  ["code, skipped", element("p", [element("code", [text("foo")]), text("مرحبا")]), "rtl"],
  ["an image's description", element("p", [element("img", [], { alt: "مرحبا" })]), "rtl"],
  ["nothing strong anywhere", element("ul", [element("li", [text("1")])]), undefined],
])("reads a tree with %s", (_, node, direction) => {
  expect(firstStrongDirection(node)).toBe(direction);
});
