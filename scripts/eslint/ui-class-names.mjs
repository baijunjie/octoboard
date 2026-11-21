// ESLint rules for the UI's rules about Tailwind class names. A class name is not known by where it is written (a
// `className` attribute, a constant, a helper's argument), so each rule reads every string literal and template-literal
// chunk in the file as whitespace-separated class tokens and reports a token that is the forbidden utility. A token is
// reduced to its utility first: variants (`hover:`, `rtl:`, `[&>svg]:`), the important marks and a negative sign are
// dropped, and an arbitrary value (`[3px]`, `(--pane-inset)`) is blanked, so `sm:-ml-[3px]!` is judged as `ml-`. A chunk
// of a template literal ends mid-token wherever an expression follows, which the prefix patterns tolerate.
//
// A string that is not a class list but happens to hold such a token (an element id, a region name) is a false positive
// to disable where it stands, with the reason. The rules report without a fix: replacing a utility is a design choice.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

/**
 * A class token split into its `variants` as written (`hover`, `group-focus-visible/item`, `[&>svg]`) and the `utility`
 * they apply, without its important marks and negative sign. An arbitrary value or variant is blanked first, so its
 * colons and brackets do not split the token.
 */
function parse(token) {
  const parts = token.replace(/\[[^\]]*\]/g, "[]").replace(/\([^)]*\)/g, "()").split(":");
  return { variants: parts.slice(0, -1), utility: parts[parts.length - 1].replace(/^[!-]+/, "").replace(/!$/, "") };
}

/**
 * A rule reporting each class token `matches` accepts, with `messageId` carrying the `name` of the token as written.
 * `matches` is given the token's parts (`utility`, `variants`) and the utilities of every token in the chunk. `also`
 * reports a chunk of text as a whole, for what is not a token (a selector), when no token was reported.
 */
function classRule({ description, message, matches, also }) {
  return {
    meta: {
      type: "problem",
      docs: { description },
      schema: [],
      messages: { forbidden: message },
    },
    create(context) {
      const check = (node, text) => {
        const tokens = text.split(/\s+/).filter((token) => token !== "");
        const parsed = tokens.map(parse);
        const utilities = parsed.map(({ utility }) => utility);
        let reported = false;
        for (const [index, token] of tokens.entries()) {
          if (!matches(parsed[index], utilities)) continue;
          context.report({ node, messageId: "forbidden", data: { name: token } });
          reported = true;
        }
        if (!reported && also?.test(text)) context.report({ node, messageId: "forbidden", data: { name: also.exec(text)[0] } });
      };
      return {
        Literal(node) {
          if (typeof node.value === "string") check(node, node.value);
        },
        TemplateElement(node) {
          check(node, node.value.cooked ?? node.value.raw);
        },
      };
    },
  };
}

export const noFocusVisible = classRule({
  description: "Disallow styling focus with CSS `:focus-visible`.",
  message: "`{{name}}` styles focus with CSS `:focus-visible`, which the app's WKWebView does not match after a click. Draw the ring from react-aria's modality (`useFocusVisibleProps`, `data-focus-visible`; docs/conventions/ui-accessibility.md, \"Focus\").",
  matches: ({ variants }) => variants.some((variant) => /^(?:group-|peer-|has-|in-|not-)?focus-visible(?:\/.+)?$/.test(variant)),
  also: /:focus-visible\b/,
});

export const noEllipsis = classRule({
  description: "Disallow ellipsis and line-clamp utilities on text.",
  message: "`{{name}}` cuts text with an ellipsis or a line clamp. Single-line text that may be clipped uses FadeOverflow (docs/conventions/ui-standard.md, \"Clipped text\").",
  matches: ({ utility }) => /^(?:truncate|text-ellipsis|line-clamp-(?!none$).+)$/.test(utility),
});

export const noPhysicalDirection = classRule({
  description: "Disallow physical left/right utilities in favour of logical ones.",
  message: "`{{name}}` is a physical direction, which does not mirror under a right-to-left language. Use the logical utility (`ms-`, `me-`, `ps-`, `pe-`, `start-`, `end-`, `text-start`, `text-end`, `rounded-s-`, `rounded-e-`, `border-s`, `border-e`), or give a hand-placed decoration its `rtl:` twin and disable the rule there (docs/conventions/ui-accessibility.md, \"Text direction\").",
  // A utility under `rtl:` or `ltr:` is the twin that a physical decoration is given.
  matches: ({ utility, variants }) => !variants.some((variant) => variant === "rtl" || variant === "ltr")
    && (/^(?:(?:scroll-)?[mp][lr]|left|right)-/.test(utility)
      || /^(?:float|clear)-(?:left|right)$/.test(utility)
      || /^text-(?:left|right)$/.test(utility)
      || /^rounded-(?:l|r|tl|tr|bl|br)(?:-|$)/.test(utility)
      || /^border-(?:l|r)(?:-|$)/.test(utility)),
});

export const noBorderDivider = classRule({
  description: "Disallow `border-separator` on one side's border and `divide-*` as a divider between a pane's parts.",
  message: "`{{name}}` draws a divider with the HeroUI separator colour on a single side's border, or with Tailwind's divide utilities. A line dividing a pane's parts or rows is a groove (`groove-b` and siblings); a flat card or a table divides with `hairline-b` (docs/conventions/ui-components.md, \"A line that divides a pane's parts or rows\"; docs/conventions/ui-standard.md, \"Cards, grooves and hairlines\").",
  // `--separator` stays for HeroUI components and composite frames, whose border goes round the whole box: only a
  // single side's border carrying it is a divider. A zero-width side or `divide-*-0` draws nothing.
  matches: ({ utility }, utilities) => /^divide-(?![xy]-0$)/.test(utility)
    || (/^border-separator(?:\/.+)?$/.test(utility) && utilities.some((other) => /^border-[tbxyselr](?:-[1-9]\d*)?$/.test(other))),
});

// ESLint's cache tells plugins apart only by `meta.name` and `meta.version`, so the version is this file's own content
// hash: any edit to the rules invalidates cached lint results.
const version = createHash("sha256").update(readFileSync(new URL(import.meta.url))).digest("hex").slice(0, 12);

export default {
  meta: { name: "octoboard-ui-class-names", version },
  rules: {
    "no-focus-visible": noFocusVisible,
    "no-ellipsis": noEllipsis,
    "no-physical-direction": noPhysicalDirection,
    "no-border-divider": noBorderDivider,
  },
};
