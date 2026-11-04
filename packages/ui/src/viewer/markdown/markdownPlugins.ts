import GithubSlugger, { slug as githubSlug } from "github-slugger";

import { classNamesOf, codeChild, fenceOf, textOf as hastText } from "./documentNodes";
import { RENDER_BUDGETS } from "../code/budgets";
import { resolveProjectAddress } from "./projectAddress";

/** The Markdown syntax tree as far as these plugins read it. */
interface MdNode {
  type: string;
  lang?: string | null;
  url?: string | null;
  value?: string;
  position?: { start: { offset?: number | null }; end: { offset?: number | null } };
  children?: MdNode[];
  data?: { hProperties?: Record<string, unknown> };
}

interface HastNode {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: HastNode[];
}

function mark(node: MdNode, properties: Record<string, unknown>): void {
  node.data = { ...node.data, hProperties: { ...node.data?.hProperties, ...properties } };
}

function sliceOf(source: string, node: MdNode): string | undefined {
  const start = node.position?.start.offset;
  const end = node.position?.end.offset;
  if (typeof start !== "number" || typeof end !== "number") return undefined;
  return source.slice(start, end);
}

/**
 * Whether an inline `$...$` span is one GitHub draws as a formula. The parser pairs dollars the
 * way it pairs code spans, so `$20 and $30` becomes one span whose closing dollar is preceded by a
 * space; GitHub leaves that as text. A closing dollar followed by a digit is text too. `$$` is a
 * formula either way. The decision needs the source slice, because an escape is only visible
 * before parsing and a text walk afterwards cannot see it.
 */
function keepInlineMath(source: string, node: MdNode): boolean {
  const raw = sliceOf(source, node);
  if (raw === undefined) return true;
  if (raw.startsWith("$$")) return true;
  const inner = raw.slice(1, -1);
  if (/^\s/.test(inner) || /\s$/.test(inner)) return false;
  const end = node.position?.end.offset;
  const after = typeof end === "number" ? source[end] : undefined;
  return after === undefined || after < "0" || after > "9";
}

function revertInlineDollars(node: MdNode, source: string): void {
  const children = node.children;
  if (!children) return;
  for (let i = 0; i < children.length; i += 1) {
    const child = children[i];
    if (!child) continue;
    if (child.type === "inlineMath" && !keepInlineMath(source, child)) {
      children[i] = { type: "text", value: sliceOf(source, child) ?? child.value ?? "" };
      continue;
    }
    revertInlineDollars(child, source);
  }
}

/** Keeps GitHub's literal-dollar rules before converting the Markdown tree to HTML. */
export function prepareMarkdown(source: string) {
  return function prepare() {
    return (tree: MdNode) => {
      revertInlineDollars(tree, source);
      const visit = (node: MdNode) => {
        if (node.type === "inlineMath" && (sliceOf(source, node)?.startsWith("$$") ?? false)) {
          mark(node, { dataDisplay: true });
        }
        node.children?.forEach(visit);
      };
      visit(tree);
    };
  };
}

/**
 * Counts renderings after HTML has been parsed and sanitized, so raw HTML and Markdown share
 * the same limits. An oversized item does not consume a place a later small item could use.
 */
export function limitRenderings(documentPath: string) {
  return () => (tree: HastNode) => {
    let diagrams = 0;
    let formulas = 0;
    let images = 0;
    let fences = 0;
    let fenceChars = 0;
    const visit = (node: HastNode, parent?: HastNode) => {
      if (node.type === "element") {
        const props = node.properties ??= {};
        if (node.tagName === "code" && parent?.tagName !== "pre" && classNamesOf(node).includes("math-inline")) {
          const length = hastText(node).replace(/\n$/, "").length;
          if (length > RENDER_BUDGETS.documentFormulaChars || formulas >= RENDER_BUDGETS.documentFormulas) props.dataOverBudget = true;
          else formulas += 1;
        }
        if (node.tagName === "pre") {
          const code = codeChild(node);
          if (code) {
            const properties = code.properties ??= {};
            const fence = fenceOf(node);
            const length = fence.text.length;
            if (classNamesOf(code).includes("math-display")) {
              if (length > RENDER_BUDGETS.documentFormulaChars || formulas >= RENDER_BUDGETS.documentFormulas) properties.dataOverBudget = true;
              else formulas += 1;
            } else if (fence.language === "mermaid") {
              if (length > RENDER_BUDGETS.documentDiagramChars || diagrams >= RENDER_BUDGETS.documentDiagrams) properties.dataOverBudget = true;
              else diagrams += 1;
            } else if (fences >= RENDER_BUDGETS.documentFences || fenceChars + length > RENDER_BUDGETS.documentFenceChars) {
              properties.dataPlain = true;
            } else {
              fences += 1;
              fenceChars += length;
            }
          }
        }
        if (node.tagName === "img") {
          const address = resolveProjectAddress(documentPath, typeof props.src === "string" ? props.src : undefined);
          if (address.kind === "file" || (address.kind === "external" && /^https?:/.test(address.href))) {
            if (images >= RENDER_BUDGETS.documentImages) props.dataOverBudget = true;
            else images += 1;
          }
        }
      }
      node.children?.forEach((child) => visit(child, node));
    };
    visit(tree);
  };
}

/**
 * Gives each heading a GitHub-style id, after footnotes exist, so a heading never takes an id a
 * footnote already has (`user-content-fn-…`, `footnote-label`). A heading that already has an id
 * keeps it: the footnote heading is one, and overwriting it would break the description the
 * footnotes point at. A repeated heading text stays distinct. A heading whose text slugs to
 * nothing — punctuation, an emoji, an image with no alt — is `section`, and a later one of the
 * same kind stays on that basis (`section-1`), because the slugger records an empty result and
 * would otherwise hand back `-1`.
 */
export function headingIds() {
  return (tree: HastNode) => {
    const taken = new Set<string>();
    const collect = (node: HastNode) => {
      const id = node.properties?.id;
      if (typeof id === "string" && id !== "") taken.add(id);
      node.children?.forEach(collect);
    };
    collect(tree);
    const slugger = new GithubSlugger();
    const visit = (node: HastNode) => {
      if (node.type === "element" && node.tagName !== undefined && /^h[1-6]$/.test(node.tagName)) {
        const existing = node.properties?.id;
        if (typeof existing !== "string" || existing === "") {
          const basis = githubSlug(hastText(node)) || "section";
          let id = slugger.slug(basis);
          while (taken.has(id)) id = slugger.slug(basis);
          taken.add(id);
          node.properties = { ...node.properties, id };
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}
