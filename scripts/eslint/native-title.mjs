// An ESLint rule for the UI's rule that nothing in the window uses the native `title` attribute to show a tooltip (see
// "Tooltips and hints" in docs/conventions/ui-standard.md). A `title` JSX attribute is reported on a DOM
// element (a lowercase name) and on a component imported from the wrapper layer, `primitives`, whose components are
// HeroUI's controls. A component of the application's own may take a `title` prop of its own (a dialog's, an alert's), so
// those are not followed.
//
// A DOM element given a `title` for the one place that keeps it (a link inside a Markdown document) disables the rule
// there, with the reason.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

/** Whether `name`, as seen from `node`, is bound by an import from the wrapper layer's barrel. */
function importedFromPrimitives(context, node, name) {
  for (let scope = context.sourceCode.getScope(node); scope; scope = scope.upper) {
    const variable = scope.set.get(name);
    if (!variable) continue;
    const def = variable.defs[0];
    return def?.type === "ImportBinding" && /(?:^|\/)primitives$/.test(def.parent.source.value);
  }
  return false;
}

/** @type {import("eslint").Rule.RuleModule} */
export const nativeTitle = {
  meta: {
    type: "problem",
    docs: { description: "Disallow the native `title` attribute as a tooltip on a DOM element or a primitive." },
    schema: [],
    messages: {
      title: "`title` on `<{{name}}>` is the browser's tooltip, which draws outside the application's control. Use the application's own Tooltip, or name the control with `aria-label` (docs/conventions/ui-standard.md, \"Tooltips and hints\").",
    },
  },
  create(context) {
    return {
      JSXAttribute(node) {
        if (node.name.type !== "JSXIdentifier" || node.name.name !== "title") return;
        const element = node.parent.name;
        if (element.type !== "JSXIdentifier") return;
        const native = /^[a-z]/.test(element.name);
        if (!native && !importedFromPrimitives(context, node, element.name)) return;
        context.report({ node, messageId: "title", data: { name: element.name } });
      },
    };
  },
};

// ESLint's cache tells plugins apart only by `meta.name` and `meta.version`, so the version is this file's own content
// hash: any edit to the rule invalidates cached lint results.
const version = createHash("sha256").update(readFileSync(new URL(import.meta.url))).digest("hex").slice(0, 12);

export default { meta: { name: "octoboard-native-title", version }, rules: { "native-title": nativeTitle } };
