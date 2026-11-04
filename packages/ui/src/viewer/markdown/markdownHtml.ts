import { defaultSchema, type Options } from "rehype-sanitize";

// Only the formula marker crosses the sanitizer; budgets are assigned afterwards, including
// those for raw HTML. Only renderer-specific classes survive.
export const markdownHtmlSchema: Options = {
  ...defaultSchema,
  tagNames: defaultSchema.tagNames?.filter((tag) => tag !== "source" && tag !== "picture"),
  strip: [...(defaultSchema.strip ?? []), "style", "iframe", "object", "embed", "svg", "math", "template"],
  attributes: {
    ...defaultSchema.attributes,
    "*": (defaultSchema.attributes?.["*"] ?? []).filter((attribute) => attribute !== "title"),
    code: [["className", /^language-./, "math-inline", "math-display"], "dataDisplay"],
  },
};

interface HtmlNode {
  type: string;
  properties?: Record<string, unknown>;
  children?: HtmlNode[];
}

/** The sanitizer prefixes ids and ARIA references to prevent DOM clobbering; href fragments
 * need the same mapping. Headings without authored ids get their GitHub ids afterwards. */
export function sanitizedFragments(prefix: string) {
  return function fragments() {
    return (tree: HtmlNode) => {
      const ids = new Set<string>();
      const walk = (node: HtmlNode, visit: (node: HtmlNode) => void) => {
        visit(node);
        node.children?.forEach((child) => walk(child, visit));
      };
      walk(tree, (node) => {
        if (typeof node.properties?.id === "string") ids.add(node.properties.id);
      });
      walk(tree, (node) => {
        const href = node.properties?.href;
        if (typeof href !== "string" || !href.startsWith("#")) return;
        const fragment = href.slice(1);
        let decoded = fragment;
        try {
          decoded = decodeURIComponent(fragment);
        } catch {
          // Invalid encoding stays literal.
        }
        const id = [fragment, decoded].map((value) => `${prefix}${value}`).find((value) => ids.has(value));
        if (id) node.properties!.href = `#${id}`;
      });
    };
  };
}
