import GithubSlugger, { slug as githubSlug } from "github-slugger";

import { RENDER_BUDGETS } from "./budgets";
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

/**
 * Marks diagrams, formulas and project images past the document's budget (`RENDER_BUDGETS`). An
 * over-size diagram or formula does not take a place in the count. An image counts only when it
 * resolves to a project file; one that cannot be read is its description and costs nothing.
 * `$$...$$` inside a paragraph is still a display formula: the parser calls it inline, and the
 * marker length is what decides.
 */
function budgetRenderings(tree: MdNode, source: string, documentPath: string): void {
  let diagrams = 0;
  let formulas = 0;
  let images = 0;
  const visit = (node: MdNode) => {
    if (node.type === "code" && node.lang === "mermaid") {
      const length = node.value?.length ?? 0;
      if (length > RENDER_BUDGETS.documentDiagramChars || diagrams >= RENDER_BUDGETS.documentDiagrams) {
        mark(node, { dataOverBudget: true });
      } else diagrams += 1;
    }
    if (node.type === "inlineMath" || node.type === "math") {
      const length = node.value?.length ?? 0;
      const properties: Record<string, unknown> = {};
      if (node.type === "inlineMath" && (sliceOf(source, node)?.startsWith("$$") ?? false)) properties.dataDisplay = true;
      if (length > RENDER_BUDGETS.documentFormulaChars || formulas >= RENDER_BUDGETS.documentFormulas) properties.dataOverBudget = true;
      else formulas += 1;
      if (Object.keys(properties).length > 0) mark(node, properties);
    }
    if (node.type === "image" && resolveProjectAddress(documentPath, node.url ?? undefined).kind === "file") {
      if (images >= RENDER_BUDGETS.documentImages) mark(node, { dataOverBudget: true });
      else images += 1;
    }
    node.children?.forEach(visit);
  };
  visit(tree);
}

/**
 * Reverts inline dollars GitHub would not draw, then applies the document's render budget.
 * Called with the source before it is placed in the plugin list, so this returns the plugin.
 * Unified calls each list entry to obtain the transformer. Returning the transformer itself
 * would run it once at setup, with no tree, and never on the document. `limitFences` and
 * `headingIds` are the plugins themselves and are passed uncalled.
 */
export function prepareMarkdown(source: string, documentPath: string) {
  return function prepare() {
    return (tree: MdNode) => {
      revertInlineDollars(tree, source);
      budgetRenderings(tree, source, documentPath);
    };
  };
}

/**
 * Marks the fenced blocks past the document's highlighting budget as plain. A `mermaid` fence is
 * not highlighted at all, so it takes no place in that budget; it is drawn as a diagram, or as its
 * source when it is past the diagram budget. Only a block that stays highlighted counts, so one
 * over-size block does not turn the later, small ones plain.
 */
export function limitFences() {
  return (tree: MdNode) => {
    let blocks = 0;
    let chars = 0;
    const visit = (node: MdNode) => {
      if (node.type === "code" && node.lang !== "mermaid") {
        const length = node.value?.length ?? 0;
        if (blocks + 1 > RENDER_BUDGETS.documentFences || chars + length > RENDER_BUDGETS.documentFenceChars) {
          mark(node, { dataPlain: true });
        } else {
          blocks += 1;
          chars += length;
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

function hastText(node: HastNode): string {
  if (node.type === "text") return node.value ?? "";
  if (node.type !== "element") return "";
  return (node.children ?? []).map(hastText).join("");
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
