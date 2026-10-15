// `@tailwindcss/typography` in a selector form WebKit can file rules for, keeping the `not-prose`
// escape.
//
// The plugin's default (modern) form, `.prose :where(pre):not(:where(.not-prose, .not-prose *))`,
// is filed by WebKit under nothing: a rule is filed under the class, tag or id of its rightmost
// compound, and a `:where()` has none of the three, so each of the plugin's rules is tried against
// every element on every style recalculation. The legacy form (`.prose pre`) is filed under `pre`
// but has no escape, which the viewer needs around code, formulas and diagrams.
//
// So every rule here is `:where(.prose) pre:where(:not(.not-prose, .not-prose *))`: the rightmost
// compound keeps its tag, and the class and the escape weigh nothing. A rule therefore weighs what
// its tags, pseudo-classes and attributes add up to, as in the legacy form: a tag-only rule loses
// to any utility class on the element, while one with a pseudo-class or an attribute
// (`ol[type="A"]`, `> ul > li > p:first-child`) weighs 0,1,x and beats a lone utility. Where the
// modern form let the later of two equal rules win, the heavier one wins here, as the legacy form
// has always resolved them.
//
// Only the rules the plugin registers as components are rewritten. Its `prose-*` variants
// (`prose-a:underline`) come out in the legacy form, without the escape, and are not used.
import typography from "@tailwindcss/typography";

const ESCAPE = ":where(:not(.not-prose, .not-prose *))";

/** The selector list `selectors` below the class `className`: the class and the escape in `:where()`,
 * the escape on the last compound of each selector and before a trailing pseudo-element, which has
 * to stay last. The plugin's keys hold no parentheses; a release that adds some would be split or
 * suffixed wrongly, so the build fails rather than drop the rule in the browser. */
function scoped(className, selectors) {
  if (selectors.includes("(")) throw new Error(`typography.mjs cannot scope the selector "${selectors}"`);
  return selectors
    .split(",")
    .map((selector) => {
      const rule = selector.trim().replace(/(::[a-z-]+)?$/, `${ESCAPE}$1`);
      return `:where(${className}) ${rule}`;
    })
    .join(", ");
}

const isRule = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/** The plugin's `{ ".prose": { "h2": { ... }, ... } }` as one entry per rule, in the same order; the
 * class's own declarations (strings, and arrays of fallbacks) stay on the class, which is not a
 * descendant rule and needs neither the scope nor the escape. */
function scopeComponents(components) {
  return components.flatMap((component) =>
    Object.entries(component).flatMap(([className, rules]) => {
      const entries = [];
      let own;
      for (const [key, value] of Object.entries(rules)) {
        if (isRule(value)) {
          own = undefined;
          entries.push({ [scoped(className, key)]: value });
        } else {
          if (!own) entries.push({ [className]: (own = {}) });
          own[key] = value;
        }
      }
      return entries;
    }),
  );
}

const legacy = typography({ target: "legacy" });

export default {
  config: legacy.config,
  handler: (api) =>
    legacy.handler({ ...api, addComponents: (components, options) => api.addComponents(scopeComponents(components), options) }),
};
