/** The node fields used to choose a document renderer and measure exactly what it will draw. */
export interface DocumentNode {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: DocumentNode[];
}

export function textOf(node: DocumentNode): string {
  if (node.type === "text") return node.value ?? "";
  if (node.type !== "element") return "";
  if (node.tagName === "img") return String(node.properties?.alt ?? "");
  return (node.children ?? []).map(textOf).join("");
}

export function classNamesOf(node: DocumentNode | undefined): string[] {
  const names = node?.properties?.className;
  return Array.isArray(names) ? names.filter((name): name is string => typeof name === "string") : [];
}

export function codeChild(node: DocumentNode): DocumentNode | undefined {
  return node.children?.find((child) => child.type === "element" && child.tagName === "code");
}

/** The same source and first language class drive both rendering and its budget. */
export function fenceOf(node: DocumentNode): { language?: string; text: string; highlight: boolean } {
  const code = codeChild(node);
  const info = classNamesOf(code).find((name) => name.startsWith("language-"));
  return {
    language: info?.slice("language-".length),
    text: code ? textOf(code).replace(/\n$/, "") : "",
    highlight: code?.properties?.dataPlain === undefined,
  };
}

/** Formatting wrappers and whitespace do not turn an image link into a text link. */
export function isImageLink(node: DocumentNode | undefined): boolean {
  let image = false;
  const onlyImages = (child: DocumentNode): boolean => {
    if (child.type === "text") return (child.value ?? "").trim() === "";
    if (child.type === "element" && child.tagName === "img") {
      image = true;
      return true;
    }
    return (child.children ?? []).every(onlyImages);
  };
  return !!node && onlyImages(node) && image;
}
