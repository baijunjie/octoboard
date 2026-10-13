/**
 * Where an address written in a document may lead. Resolved against the document's own wire path.
 * Nothing in the text is allowed to leave the project: a path that would, or that is not a project
 * file at all, is refused rather than read. `URL` is not used to join the path, because it folds
 * `..` above the root back to the root and would accept a path that has left the project.
 */

/** An address the platform may open: absolute, and one a person or a mail client takes. */
export function openableAddress(href: string | undefined): string | undefined {
  if (!href) return undefined;
  try {
    const url = new URL(href.trim());
    return url.protocol === "http:" || url.protocol === "https:" || url.protocol === "mailto:" ? url.href : undefined;
  } catch {
    return undefined;
  }
}

export type ProjectAddress =
  | { kind: "anchor"; id: string }
  | { kind: "external"; href: string }
  | { kind: "file"; path: string; fragment?: string }
  | { kind: "refused" };

const SCHEME = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

/** Resolves `href` as written in the document at `documentPath` (a wire path). */
export function resolveProjectAddress(documentPath: string, href: string | undefined): ProjectAddress {
  if (!href) return { kind: "refused" };
  const trimmed = href.trim();
  if (trimmed === "" || trimmed.includes("\0") || trimmed.includes("\\")) return { kind: "refused" };
  if (trimmed.startsWith("#")) return { kind: "anchor", id: trimmed.slice(1) };
  // A scheme-relative address (`//host`) would be fetched from whatever the page was served on.
  if (trimmed.startsWith("//")) return { kind: "refused" };
  const external = openableAddress(trimmed);
  if (external) return { kind: "external", href: external };
  if (SCHEME.test(trimmed) || trimmed.startsWith("/")) return { kind: "refused" };

  const hash = trimmed.indexOf("#");
  const beforeHash = hash === -1 ? trimmed : trimmed.slice(0, hash);
  const fragment = hash === -1 ? undefined : trimmed.slice(hash + 1);
  const pathPart = beforeHash.split("?")[0] ?? "";
  const slash = documentPath.lastIndexOf("/");
  const segments = slash === -1 ? [] : documentPath.slice(0, slash).split("/");
  for (const part of pathPart.split("/")) {
    if (part === "" || part === ".") continue;
    let decoded: string;
    try {
      decoded = decodeURIComponent(part);
    } catch {
      return { kind: "refused" };
    }
    if (decoded === "" || decoded === ".") continue;
    if (decoded === "..") {
      if (segments.length === 0) return { kind: "refused" };
      segments.pop();
      continue;
    }
    if (decoded.includes("/") || decoded.includes("\\") || decoded.includes("\0")) return { kind: "refused" };
    segments.push(decoded);
  }
  if (segments.length === 0) return { kind: "refused" };
  const path = segments.join("/");
  return fragment ? { kind: "file", path, fragment } : { kind: "file", path };
}
