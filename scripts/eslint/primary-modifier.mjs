// An ESLint rule for the UI's rule that a Cmd/Ctrl test never accepts either key (see "Key handling" in
// docs/conventions/ui-accessibility.md, "Any Cmd/Ctrl test goes through `isPrimaryModifier`"). Which of the two is the
// primary modifier depends on the host, so `event.metaKey || event.ctrlKey` is right on neither Mac nor Windows alone:
// Ctrl-click on a Mac is a secondary click.
//
// What is reported is the expression that is nothing but the two keys: `a || b`, or its negation `!a && !b` ("neither"),
// where each of `a` and `b` is a `metaKey` or `ctrlKey` read (a member of any object, or a destructured name) and both
// keys appear. A chain that also tests something else (`event.key !== "Tab" || event.ctrlKey || event.metaKey`, or with
// `altKey`) is a guard that bails out on any modifier, not a test for the primary one, and is left alone.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const KEYS = new Set(["metaKey", "ctrlKey"]);

/** The key a node reads (`x.metaKey`, `metaKey`), or undefined. */
function keyRead(node) {
  if (node.type === "Identifier") return KEYS.has(node.name) ? node.name : undefined;
  if (node.type === "MemberExpression" && !node.computed && node.property.type === "Identifier") {
    return KEYS.has(node.property.name) ? node.property.name : undefined;
  }
  return undefined;
}

/** The operands of a chain of one logical operator, flattened. */
function operands(node, operator) {
  if (node.type === "LogicalExpression" && node.operator === operator) return [...operands(node.left, operator), ...operands(node.right, operator)];
  return [node];
}

/** The keys read by a chain of `operator` whose every operand is a read (`negated` false) or a `!` of one (true). */
function keysOfChain(node, operator, negated) {
  const keys = operands(node, operator).map((operand) => {
    if (!negated) return keyRead(operand);
    return operand.type === "UnaryExpression" && operand.operator === "!" ? keyRead(operand.argument) : undefined;
  });
  return keys.every((key) => key !== undefined) ? new Set(keys) : undefined;
}

/** @type {import("eslint").Rule.RuleModule} */
export const primaryModifier = {
  meta: {
    type: "problem",
    docs: { description: "Disallow a test that accepts either `metaKey` or `ctrlKey`." },
    schema: [],
    messages: {
      either: "Do not accept either of `metaKey` and `ctrlKey`: test the primary modifier with `isPrimaryModifier` from input/keys.ts, and show it by `host.modifier` (docs/conventions/ui-accessibility.md, \"Key handling\").",
    },
  },
  create(context) {
    const check = (node, operator, negated) => {
      // Only the outermost expression of a chain: its parts are the same chain.
      if (node.parent.type === "LogicalExpression" && node.parent.operator === operator) return;
      const keys = keysOfChain(node, operator, negated);
      if (keys?.size === 2) context.report({ node, messageId: "either" });
    };
    return {
      'LogicalExpression[operator="||"]': (node) => check(node, "||", false),
      'LogicalExpression[operator="&&"]': (node) => check(node, "&&", true),
    };
  },
};

// ESLint's cache tells plugins apart only by `meta.name` and `meta.version`, so the version is this file's own content
// hash: any edit to the rule invalidates cached lint results.
const version = createHash("sha256").update(readFileSync(new URL(import.meta.url))).digest("hex").slice(0, 12);

export default { meta: { name: "octoboard-primary-modifier", version }, rules: { "primary-modifier": primaryModifier } };
