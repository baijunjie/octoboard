// The file viewer's own model of what it shows. Callers build it from daemon replies; nothing in
// it names a rendering library's types, so the renderer behind `CodeSurface` can be replaced
// without touching the daemon contract or the callers.
import type { FileContent } from "../protocol";

/**
 * A file body classified for display. `text` is shown as code; `image` through an image element,
 * from a `data:` URL (the window's Content Security Policy admits images from `data:` only);
 * `binary` is anything else, shown as an explicit "cannot be displayed" presentation.
 */
export type ViewerBody =
  | { kind: "text"; text: string; size: number }
  | {
      kind: "image";
      mediaType: string;
      url: string;
      size: number;
      /** An SVG's own text, shown as code when the image cannot be decoded. */
      text?: string;
    }
  | { kind: "binary"; size: number };

/** One side of a change, mirroring the daemon's `ChangeSide` (see "Changes and comparisons" in
 * `apps/daemon/PROTOCOL.md`). `path` is a wire path relative to the project; `body` is present
 * only for a `file` side whose content was read. */
export type ViewerChangeSide =
  | { state: "present"; path: string; kind: "file" | "symlink" | "submodule"; body?: ViewerBody }
  | { state: "absent" }
  | { state: "out_of_scope"; repositoryPath: string };

/** A change between two sides, as the daemon's diff reply identifies it. `patch` is that one
 * change's unified patch as `git` writes it, which every text change is drawn from; a reply always
 * carries one, and a change without it has no diff to show. */
export interface ViewerChange {
  old: ViewerChangeSide;
  new: ViewerChangeSide;
  patch?: string;
}

export type ViewerContent =
  | { state: "loading" }
  | { state: "error"; message: string }
  | { state: "file"; body: ViewerBody }
  | { state: "change"; change: ViewerChange };

/**
 * What the viewer shows. `key` is the subject's identity — the caller composes it from the
 * project, the source and the path (a path alone is not an identity) — and a new key is a new
 * subject: what was shown for the previous one is dropped. `path` is the wire path the title
 * names; `source` is the caller's own wording of where the content came from.
 */
export interface ViewerSubject {
  key: string;
  path: string;
  source?: string;
  content: ViewerContent;
}

/**
 * The image formats the viewer hands to an image element. The binary ones are those the daemon
 * recognises from a body's first bytes (`media_type`); SVG arrives as text and is recognised by its
 * name. AVIF decodes only where the system's WebKit does (macOS 13 and later), so a body that fails
 * to decode falls back to the unsupported presentation rather than a broken image.
 */
const IMAGE_MEDIA_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
  "image/bmp",
  "image/x-icon",
  "image/avif",
]);

/** Classifies a body the daemon read for `path` (a wire path). */
export function bodyFromFileContent(path: string, file: FileContent): ViewerBody {
  if (file.kind === "binary") {
    if (file.media_type && IMAGE_MEDIA_TYPES.has(file.media_type) && file.data !== null) {
      return { kind: "image", mediaType: file.media_type, url: `data:${file.media_type};base64,${file.data}`, size: file.size };
    }
    return { kind: "binary", size: file.size };
  }
  const text = file.text ?? "";
  if (/\.svg$/i.test(path)) {
    return {
      kind: "image",
      mediaType: "image/svg+xml",
      url: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`,
      size: file.size,
      text,
    };
  }
  return { kind: "text", text, size: file.size };
}

/** How the viewer words a change: by which sides exist and whether their paths or kinds differ. */
export type ChangeStatus = "added" | "deleted" | "renamed" | "typeChanged" | "modified";

export function changeStatus(change: ViewerChange): ChangeStatus {
  const { old: before, new: after } = change;
  if (before.state === "absent") return "added";
  if (after.state === "absent") return "deleted";
  if (before.state === "present" && after.state === "present") {
    if (before.kind !== after.kind) return "typeChanged";
    if (before.path !== after.path) return "renamed";
  }
  // A side outside the project's scope is a rename across its boundary.
  return before.state === "out_of_scope" || after.state === "out_of_scope" ? "renamed" : "modified";
}

/**
 * Which presentation a change gets:
 *
 * - `restricted`: one side lies outside the project. Only the permitted side is shown, as a file,
 *   with the restriction named; no patch is made up for it and the other side is not treated as
 *   empty, which is what an absent side means.
 * - `text`: a line diff, drawn from the patch.
 * - `image`: the two images (or the one that exists) side by side.
 * - `binary`: a side is a non-image binary, or an image faces text; neither has a line diff.
 * - `unreadable`: no patch to draw a diff from.
 */
export type ChangePresentation =
  | { kind: "restricted"; hidden: "old" | "new"; repositoryPath: string; shown: ViewerChangeSide }
  | { kind: "text"; patch: string }
  | { kind: "image" }
  | { kind: "binary" }
  | { kind: "unreadable" };

export function changePresentation(change: ViewerChange): ChangePresentation {
  const { old: before, new: after } = change;
  if (before.state === "out_of_scope") return { kind: "restricted", hidden: "old", repositoryPath: before.repositoryPath, shown: after };
  if (after.state === "out_of_scope") return { kind: "restricted", hidden: "new", repositoryPath: after.repositoryPath, shown: before };
  const bodies = [before, after].flatMap((side) => (side.state === "present" && side.body ? [side.body] : []));
  if (bodies.some((body) => body.kind === "image") && bodies.every((body) => body.kind === "image")) return { kind: "image" };
  // A binary side, or an image facing text, has no line diff to show.
  if (bodies.some((body) => body.kind !== "text")) return { kind: "binary" };
  return change.patch !== undefined ? { kind: "text", patch: change.patch } : { kind: "unreadable" };
}
