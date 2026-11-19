// An ESLint rule for the UI's rule that a `lucide-react` icon is given no redundant `aria-hidden` (see "Accessible
// names" in docs/conventions/ui-standard.md). Lucide sets `aria-hidden="true"` itself only while the icon has no
// children and no `role`, `title` or other `aria-*` prop, so the attribute is reported, and removed by the fix, only
// where it adds nothing: its value is absent, `"true"` or `{true}`, the element has no children, no spread and no other
// `aria-*`, `role` or `title` attribute. Anywhere else it may change what assistive technology gets, and is left alone.
//
// An icon is known by resolving the element's name through scope to an import from `lucide-react`: a named import
// (aliases included) or the object of a namespace import (`<Icons.Power …>`). A local binding that shadows the import
// is not an icon. An icon reached through a prop or variable typed `LucideIcon` is not followed: nothing in the syntax
// says what it holds.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const SOURCE = "lucide-react";

/** Whether `name`, as seen from `node`, is bound by an import from `lucide-react` of the given kind. */
function importedFromLucide(context, node, name, kind) {
  for (let scope = context.sourceCode.getScope(node); scope; scope = scope.upper) {
    const variable = scope.set.get(name);
    if (!variable) continue;
    const def = variable.defs[0];
    return def?.type === "ImportBinding" && def.node.type === kind && def.parent.source.value === SOURCE;
  }
  return false;
}

/** The icon's name as written, when `element` (a JSX element name) is a `lucide-react` icon. */
function lucideIcon(context, node, element) {
  if (element.type === "JSXIdentifier") {
    return importedFromLucide(context, node, element.name, "ImportSpecifier") ? element.name : undefined;
  }
  if (element.type === "JSXMemberExpression" && element.object.type === "JSXIdentifier") {
    const namespace = element.object.name;
    return importedFromLucide(context, node, namespace, "ImportNamespaceSpecifier") ? `${namespace}.${element.property.name}` : undefined;
  }
  return undefined;
}

/** Whether the attribute's value says "hidden" for certain: absent, `"true"` or `{true}`. */
function surelyTrue(value) {
  if (value === null) return true;
  if (value.type === "Literal") return value.value === "true";
  return value.type === "JSXExpressionContainer" && value.expression.type === "Literal" && value.expression.value === true;
}

/** Whether lucide adds `aria-hidden` to this element itself: no children, no spread, no other `aria-*`, `role` or `title`. */
function lucideHidesIt(opening, attribute) {
  if (!opening.selfClosing && opening.parent.children.length > 0) return false;
  return opening.attributes.every((other) => other === attribute || (other.type === "JSXAttribute"
    && other.name.type === "JSXIdentifier" && !/^aria-|^role$|^title$/.test(other.name.name)));
}

/** @type {import("eslint").Rule.RuleModule} */
export const lucideAriaHidden = {
  meta: {
    type: "suggestion",
    docs: { description: "Disallow a redundant `aria-hidden` on lucide-react icons, which carry it themselves." },
    fixable: "code",
    schema: [],
    messages: {
      redundant: "`{{name}}` is a lucide-react icon, which carries `aria-hidden` itself; drop the attribute.",
    },
  },
  create(context) {
    return {
      JSXAttribute(node) {
        if (node.name.type !== "JSXIdentifier" || node.name.name !== "aria-hidden") return;
        const opening = node.parent;
        const name = lucideIcon(context, node, opening.name);
        if (name === undefined || !surelyTrue(node.value) || !lucideHidesIt(opening, node)) return;
        const before = context.sourceCode.getTokenBefore(node, { includeComments: true });
        context.report({
          node,
          messageId: "redundant",
          data: { name },
          fix: (fixer) => fixer.removeRange([before.range[1], node.range[1]]),
        });
      },
    };
  },
};

// ESLint's cache tells plugins apart only by `meta.name` and `meta.version`, so the version is this file's own content
// hash: any edit to the rule invalidates cached lint results.
const version = createHash("sha256").update(readFileSync(new URL(import.meta.url))).digest("hex").slice(0, 12);

export default { meta: { name: "octoboard-lucide", version }, rules: { "lucide-aria-hidden": lucideAriaHidden } };
