// The file viewer's gallery scenarios: representative files, images and changes, plus the inputs
// each fallback exists for — content past the render budgets, a body that is not an image, a patch
// the renderer cannot read, a grammar that fails to load, and repository markup that must stay
// inert. The viewer renders them outside the app (see `viewerGallery.tsx`).
import type { FileContent } from "../../protocol";
import { bodyFromFileContent, type ViewerBody, type ViewerChangeSide, type ViewerSubject } from "../../viewer/content";
import type { Scenario } from "../scenario";
import { IMAGES } from "./viewerImages";

const GROUP = "File viewer";

const OLD_SESSION = `import type { Session } from "./protocol";

/** How long a session may stay idle before it is shown as waiting. */
const IDLE_AFTER_MS = 30_000;

export interface SessionView {
  id: string;
  title: string;
  waiting: boolean;
}

export function viewOf(session: Session, now: number): SessionView {
  const idle = now - session.lastActivity > IDLE_AFTER_MS;
  return {
    id: session.id,
    title: session.title || "Untitled",
    waiting: idle && session.status === "idle",
  };
}

export function sortViews(views: SessionView[]): SessionView[] {
  return [...views].sort((a, b) => Number(b.waiting) - Number(a.waiting));
}
`;

const NEW_SESSION = `import type { Session } from "./protocol";

/** How long a session may stay idle before it is shown as waiting. */
const IDLE_AFTER_MS = 45_000;

export interface SessionView {
  id: string;
  title: string;
  waiting: boolean;
  /** Set while the agent asks for permission. */
  raisedHand: boolean;
}

export function viewOf(session: Session, now: number): SessionView {
  const idle = now - session.lastActivity > IDLE_AFTER_MS;
  return {
    id: session.id,
    title: session.title.trim() || "Untitled",
    waiting: idle && session.status === "idle",
    raisedHand: session.status === "permission",
  };
}

export function sortViews(views: SessionView[]): SessionView[] {
  return [...views].sort((a, b) => Number(b.raisedHand) - Number(a.raisedHand) || Number(b.waiting) - Number(a.waiting));
}
`;

const MODIFIED_PATCH = `diff --git a/src/session.ts b/src/session.ts
index b565890..abb0951 100644
--- a/src/session.ts
+++ b/src/session.ts
@@ -1,23 +1,26 @@
 import type { Session } from "./protocol";
 
 /** How long a session may stay idle before it is shown as waiting. */
-const IDLE_AFTER_MS = 30_000;
+const IDLE_AFTER_MS = 45_000;
 
 export interface SessionView {
   id: string;
   title: string;
   waiting: boolean;
+  /** Set while the agent asks for permission. */
+  raisedHand: boolean;
 }
 
 export function viewOf(session: Session, now: number): SessionView {
   const idle = now - session.lastActivity > IDLE_AFTER_MS;
   return {
     id: session.id,
-    title: session.title || "Untitled",
+    title: session.title.trim() || "Untitled",
     waiting: idle && session.status === "idle",
+    raisedHand: session.status === "permission",
   };
 }
 
 export function sortViews(views: SessionView[]): SessionView[] {
-  return [...views].sort((a, b) => Number(b.waiting) - Number(a.waiting));
+  return [...views].sort((a, b) => Number(b.raisedHand) - Number(a.raisedHand) || Number(b.waiting) - Number(a.waiting));
 }
`;

const ADDED_PATCH = `diff --git a/src/badge.ts b/src/badge.ts
new file mode 100644
index 0000000..8699b27
--- /dev/null
+++ b/src/badge.ts
@@ -0,0 +1,3 @@
+export function badgeCount(waiting: number): string {
+  return waiting > 99 ? "99+" : String(waiting);
+}
`;

const DELETED_PATCH = `diff --git a/src/legacy.ts b/src/legacy.ts
deleted file mode 100644
index e529597..0000000
--- a/src/legacy.ts
+++ /dev/null
@@ -1,4 +0,0 @@
-// Kept for one release while callers move to \`viewOf\`.
-export function legacyTitle(title: string): string {
-  return title.trim() || "Untitled";
-}
`;

const TYPE_CHANGE_PATCH = `diff --git a/src/current.ts b/src/current.ts
deleted file mode 120000
index fee798f..0000000
--- a/src/current.ts
+++ /dev/null
@@ -1 +0,0 @@
-session.ts
\\ No newline at end of file
diff --git a/src/current.ts b/src/current.ts
new file mode 100644
index 0000000..abb0951
--- /dev/null
+++ b/src/current.ts
@@ -0,0 +1,26 @@
+import type { Session } from "./protocol";
+
+/** How long a session may stay idle before it is shown as waiting. */
+const IDLE_AFTER_MS = 45_000;
+
+export interface SessionView {
+  id: string;
+  title: string;
+  waiting: boolean;
+  /** Set while the agent asks for permission. */
+  raisedHand: boolean;
+}
+
+export function viewOf(session: Session, now: number): SessionView {
+  const idle = now - session.lastActivity > IDLE_AFTER_MS;
+  return {
+    id: session.id,
+    title: session.title.trim() || "Untitled",
+    waiting: idle && session.status === "idle",
+    raisedHand: session.status === "permission",
+  };
+}
+
+export function sortViews(views: SessionView[]): SessionView[] {
+  return [...views].sort((a, b) => Number(b.raisedHand) - Number(a.raisedHand) || Number(b.waiting) - Number(a.waiting));
+}
`;

const RENAMED_PATCH = `diff --git a/src/badge.ts b/src/badgeLabel.ts
similarity index 52%
rename from src/badge.ts
rename to src/badgeLabel.ts
index 8699b27..6e71616 100644
--- a/src/badge.ts
+++ b/src/badgeLabel.ts
@@ -1,3 +1,3 @@
 export function badgeCount(waiting: number): string {
-  return waiting > 99 ? "99+" : String(waiting);
+  return waiting > 999 ? "999+" : String(waiting);
 }
`;

const PYTHON = `"""Builds the release archive from the compiled bundle."""
from pathlib import Path
import tarfile


def build(out: Path, sources: list[Path]) -> Path:
    archive = out / "release.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for source in sources:
            tar.add(source, arcname=source.name)
    return archive


if __name__ == "__main__":
    print(build(Path("dist"), [Path("app.js"), Path("app.css")]))
`;

const MARKDOWN = `# Release notes

- The viewer keeps **code** and paths left to right.
- Images fit the window and are never scaled up.

\`\`\`sh
pnpm build:app --bundles app
\`\`\`
`;

// A fenced block far taller than the viewer, to be scrolled within the document's own frame.
const LONG_FENCE = Array.from({ length: 200 }, (_, i) => `export const entry${i} = { id: ${i}, label: "Entry ${i}" };`).join("\n");

// A document exercising every element the document view draws, plus what it must keep inert: raw
// HTML, a remote image and a script link; and a passage in Arabic and one in Japanese.
const GUIDE = `# Viewer guide

A paragraph with **bold**, _italic_, \`inline code\`, a [web link](https://example.com/guide), a bare https://example.com/bare address,
a [relative link](./README.md) and a [script link](javascript:alert(1)).

## Lists and tasks

1. First step
2. Second step
   - a nested bullet
   - another one

- [x] A finished task
- [ ] An open task

> A quoted remark that goes on for a while, so that the bar beside it spans more than one line of the document.

### A table

| Name | Kind | Notes |
| :--- | :-: | ---: |
| alpha | text | the first |
| beta | image | a much longer cell that has to wrap inside its column instead of widening the table |

## Code

\`\`\`ts
export function greet(name: string): string {
  return \`Hello, \${name}\`; // a comment long enough to need wrapping inside the block, because blocks never scroll sideways
}
\`\`\`

\`\`\`
a fence with no language
\`\`\`

\`\`\`nosuchgrammar
a fence whose language does not exist
\`\`\`

\`\`\`ts
${LONG_FENCE}
\`\`\`

An empty fence follows.

\`\`\`ts
\`\`\`

## Right to left and CJK

مرحبا بالعالم، هذه فقرة بالعربية داخل مستند إنجليزي.

- عنصر أول
- عنصر ثان

こんにちは、世界。これは日本語の段落です。

## Footnotes

A claim that needs a source[^1], and another[^2].

[^1]: The first source, with a [link](https://example.com/source).
[^2]: The second source.

A claim in Chinese[^注1].

[^注1]: 中文脚注的来源。

## Inert

![A remote picture that must not load](https://example.com/tracker.png)

<script>window.__viewerPwned = "markdown-script";</script>
<img src="https://example.com/raw.png" onerror="window.__viewerPwned = 'markdown-onerror'">
`;

const JSON_TEXT = `{
  "name": "octoboard-fixture",
  "version": "1.2.3",
  "private": true,
  "scripts": { "build": "vite build", "test": "vitest run" }
}
`;

const RUST = `use std::collections::HashMap;

/// Counts how often each word occurs.
pub fn word_counts(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}
`;

// Text in a language with no grammar, which must still be readable as plain text.
const UNKNOWN = `[plan]
step one => fetch sources
step two => build, then verify
`;

// Repository markup that must stay data: none of it may run or load anything in the window.
const HOSTILE_HTML = `<!doctype html>
<title>Not a page</title>
<script>window.__viewerPwned = "script";</script>
<img src="x" onerror="window.__viewerPwned = 'onerror'">
</span></code></pre><img src=x onerror="window.__viewerPwned='breakout'">
`;

const SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" viewBox="0 0 120 80" onload="window.__viewerPwned='svg-onload'">
  <script>window.__viewerPwned = "svg-script";</script>
  <rect x="4" y="4" width="112" height="72" rx="10" fill="#3884ff"/>
  <circle cx="60" cy="40" r="22" fill="#ffffff"/>
  <image href="https://example.com/tracker.png" width="1" height="1"/>
</svg>
`;

const BROKEN_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"`;

const MIXED_DIRECTION = `// مرحبا — a comment in Arabic, and one in Japanese: こんにちは、世界
export const greeting = { ar: "مرحبا بالعالم", ja: "こんにちは", en: "Hello" };
`;

/** A text body as the daemon sends it. */
function text(path: string, body: string): ViewerBody {
  const file: FileContent = { size: new TextEncoder().encode(body).length, kind: "text", media_type: null, text: body, data: null };
  return bodyFromFileContent(path, file);
}

/** A binary body as the daemon sends it, with the media type it would recognise. */
function binary(path: string, image: { mediaType: string | null; data: string }): ViewerBody {
  const size = Math.floor((image.data.length * 3) / 4) - (image.data.endsWith("==") ? 2 : image.data.endsWith("=") ? 1 : 0);
  return bodyFromFileContent(path, { size, kind: "binary", media_type: image.mediaType, text: null, data: image.data });
}

function file(path: string, body: ViewerBody, source = "Working tree"): ViewerSubject {
  return { key: `live:${path}`, path, source, content: { state: "file", body } };
}

function present(path: string, body?: ViewerBody, kind: "file" | "symlink" | "submodule" = "file"): ViewerChangeSide {
  return { state: "present", path, kind, body };
}

// Generated content around the budgets. A 2 MB log is past the highlighting budget; a minified line
// is far past the tokenizing line length; 9,000 lines of code sit just under the line budget.
function generated(lines: number, line: (index: number) => string): string {
  return Array.from({ length: lines }, (_, index) => line(index)).join("\n") + "\n";
}
const largeLog = () => generated(40_000, (i) => `2026-10-08T12:${String(i % 60).padStart(2, "0")}:00Z INFO request ${i} served in ${i % 97} ms`);
const minified = () => `!function(){${generated(1, () => Array.from({ length: 4_000 }, (_, i) => `var a${i}=${i}*2;`).join(""))}}();`;
const nearBudget = () => generated(9_000, (i) => `export const value${i} = { id: ${i}, label: "Item ${i}", enabled: ${i % 2 === 0} };`);
// Lines longer than the viewer is wide, in code and in prose, for the word wrap choice.
const LONG_LINE = "const message = `The session ${session.title} has been waiting for ${minutes} minutes for an answer to ${request.question}, which was last asked by ${request.agent} at ${request.askedAt}`;";
const longLines = () => ["export function describe(session: Session): string {", `  ${LONG_LINE}`, "  return message;", "}", `// ${"A comment that goes on and on without ever finding a place to stop. ".repeat(4)}`, ""].join("\n");
const LONG_LINES_PATCH = `diff --git a/src/describe.ts b/src/describe.ts
index 1111111..2222222 100644
--- a/src/describe.ts
+++ b/src/describe.ts
@@ -1,5 +1,5 @@
 export function describe(session: Session): string {
-  ${LONG_LINE.replace("waiting for", "idle for")}
+  ${LONG_LINE}
   return message;
 }
-// ${"A comment that goes on and on without ever finding a place to stop. ".repeat(3)}
+// ${"A comment that goes on and on without ever finding a place to stop. ".repeat(4)}
`;
const largePatch = () =>
  "diff --git a/data/table.ts b/data/table.ts\nindex 1111111..2222222 100644\n--- a/data/table.ts\n+++ b/data/table.ts\n@@ -1,12000 +1,12000 @@\n" +
  generated(12_000, (i) => `-export const row${i} = ${i};\n+export const row${i} = ${i + 1};`);


/** The files, built when the scenario opens: the large ones take a moment to generate, and the
 * failing grammar is registered with the renderer library, which other scenarios do not load. */
async function files(): Promise<ViewerSubject[]> {
  const { registerCustomLanguage } = await import("@pierre/diffs");
  // A grammar whose loader fails, standing in for a grammar chunk that cannot be fetched.
  registerCustomLanguage("octoboard-broken-grammar", () => Promise.reject(new Error("Gallery: this grammar fails to load")), ["brokengrammar"]);
  return [
    file("src/session.ts", text("src/session.ts", NEW_SESSION)),
    file("scripts/build.py", text("scripts/build.py", PYTHON)),
    file("crates/words/src/lib.rs", text("crates/words/src/lib.rs", RUST)),
    file("src/describe.ts", text("src/describe.ts", longLines())),
    file("README.md", text("README.md", MARKDOWN)),
    file("docs/guide.md", text("docs/guide.md", GUIDE)),
    file("package.json", text("package.json", JSON_TEXT)),
    file("notes/plan.zzplan", text("notes/plan.zzplan", UNKNOWN)),
    file("web/index.html", text("web/index.html", HOSTILE_HTML)),
    file("assets/logo.svg", text("assets/logo.svg", SVG)),
    file("assets/broken.svg", text("assets/broken.svg", BROKEN_SVG)),
    file("assets/before.png", binary("assets/before.png", IMAGES.before_png)),
    file("assets/photo.jpg", binary("assets/photo.jpg", IMAGES.photo_jpg)),
    file("assets/anim.gif", binary("assets/anim.gif", IMAGES.anim_gif)),
    file("assets/image.webp", binary("assets/image.webp", IMAGES.image_webp)),
    file("assets/bitmap.bmp", binary("assets/bitmap.bmp", IMAGES.bitmap_bmp)),
    file("assets/favicon.ico", binary("assets/favicon.ico", IMAGES.icon_ico)),
    file("assets/image.avif", binary("assets/image.avif", IMAGES.image_avif)),
    file("bin/tool.wasm", binary("bin/tool.wasm", { mediaType: null, data: "AGFzbQEAAAABBAFgAAADAgEABwkBBW1haW4AAAoEAQIACw==" })),
    file("src/i18n/greeting.ts", text("src/i18n/greeting.ts", MIXED_DIRECTION)),
    // A name that is not valid UTF-8 and contains a percent sign, in its wire form.
    file("docs/%FFreport%2520final.md", text("docs/report.md", MARKDOWN)),
    file("src/strange.brokengrammar", text("src/strange.brokengrammar", RUST)),
    file("dist/app.min.js", text("dist/app.min.js", minified())),
    file("src/generated/table.ts", text("src/generated/table.ts", nearBudget())),
    file("logs/server.log", text("logs/server.log", largeLog())),
    file("src/empty.ts", text("src/empty.ts", "")),
    { key: "live:src/pending.ts", path: "src/pending.ts", source: "Working tree", content: { state: "loading" } },
    {
      key: "live:src/gone.ts",
      path: "src/gone.ts",
      source: "Working tree",
      content: { state: "error", message: "src/gone.ts no longer exists in this project." },
    },
    { key: "live:src/offline.ts", path: "src/offline.ts", source: "Working tree", content: { state: "disconnected", what: "file" } },
    // Names too long for the header, in Latin and in CJK script.
    file(
      "packages/settings/src/components/notification-preferences/an-extraordinarily-long-component-file-name-that-keeps-going.tsx",
      text("an-extraordinarily-long-component-file-name-that-keeps-going.tsx", NEW_SESSION),
    ),
    file("docs/设计说明/交互与视觉/项目文件浏览器的交互与视觉设计说明（最终评审版，请勿直接修改）.md", text("说明.md", MARKDOWN)),
    // A generated name with no place to break a line.
    file(
      "dist/assets/chunk.3f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c.js",
      text("chunk.js", JSON_TEXT),
    ),
  ];
}

const oldSession = text("src/session.ts", OLD_SESSION);
const newSession = text("src/session.ts", NEW_SESSION);
const before = binary("assets/logo.png", IMAGES.before_png);
const after = binary("assets/logo.png", IMAGES.after_png);

function change(path: string, oldSide: ViewerChangeSide, newSide: ViewerChangeSide, patch?: string, stage = "Unstaged"): ViewerSubject {
  return { key: `${stage}:${path}`, path, stage, content: { state: "change", change: { old: oldSide, new: newSide, patch } } };
}

// An untracked file is shown alone, with the Untracked chip and no stage tag.
function untracked(path: string, body: ViewerBody): ViewerSubject {
  return { key: `Untracked:${path}`, path, status: "untracked", content: { state: "change", change: { old: { state: "absent" }, new: present(path, body) } } };
}

async function changes(): Promise<ViewerSubject[]> {
  return [
    change("src/session.ts", present("src/session.ts", oldSession), present("src/session.ts", newSession), MODIFIED_PATCH),
    change("src/describe.ts", present("src/describe.ts"), present("src/describe.ts"), LONG_LINES_PATCH),
    change("src/badge.ts", { state: "absent" }, present("src/badge.ts"), ADDED_PATCH),
    change("src/legacy.ts", present("src/legacy.ts"), { state: "absent" }, DELETED_PATCH),
    change("src/badgeLabel.ts", present("src/badge.ts"), present("src/badgeLabel.ts"), RENAMED_PATCH),
    change("src/current.ts", present("src/current.ts", undefined, "symlink"), present("src/current.ts", newSession), TYPE_CHANGE_PATCH),
    change("src/moved-in.ts", { state: "out_of_scope", repositoryPath: "packages/shared/src/moved-in.ts" }, present("src/moved-in.ts", newSession)),
    change("src/moved-out.ts", present("src/moved-out.ts", oldSession), { state: "out_of_scope", repositoryPath: "packages/shared/src/moved-out.ts" }),
    change("assets/logo.png", present("assets/logo.png", before), present("assets/logo.png", after)),
    untracked("assets/new.png", after),
    untracked("src/scratch.ts", text("src/scratch.ts", NEW_SESSION)),
    untracked("docs/draft.md", text("docs/draft.md", MARKDOWN)),
    {
      key: "Conflicted:docs/merge.md",
      path: "docs/merge.md",
      content: {
        state: "conflict",
        conflict: "Both sides modified this file. The file on disk is shown below with its conflict markers.",
        body: text("docs/merge.md", "<<<<<<< ours\n# Release notes\n=======\n# Changes\n>>>>>>> theirs\n"),
      },
    },
    {
      key: "Conflicted:src/merge.ts",
      path: "src/merge.ts",
      content: {
        state: "conflict",
        conflict: "Both sides modified this file. The file on disk is shown below with its conflict markers.",
        body: text("src/merge.ts", "<<<<<<< ours\nconst a = 1;\n=======\nconst a = 2;\n>>>>>>> theirs\n"),
      },
    },
    {
      key: "Compare:src/session.ts",
      path: "src/session.ts",
      source: "main at 1234567 to feature at fedcba0",
      sourceText: "main at 1234567 to feature at fedcba0",
      content: { state: "change", change: { old: present("src/session.ts", oldSession), new: present("src/session.ts", newSession), patch: MODIFIED_PATCH } },
    },
    change("bin/tool.wasm", present("bin/tool.wasm", binary("bin/tool.wasm", { mediaType: null, data: "AGFzbQEAAAA=" })), present("bin/tool.wasm", binary("bin/tool.wasm", { mediaType: null, data: "AGFzbQEAAAABBAFgAAA=" }))),
    change("data/table.ts", present("data/table.ts"), present("data/table.ts"), largePatch()),
    // A hunk header that promises three lines per side and delivers two: the renderer rejects it,
    // and the viewer falls back to the raw patch.
    change(
      "src/garbled.ts",
      present("src/garbled.ts"),
      present("src/garbled.ts"),
      "diff --git a/src/garbled.ts b/src/garbled.ts\n--- a/src/garbled.ts\n+++ b/src/garbled.ts\n@@ -1,3 +1,3 @@\n a\n+b\n",
    ),
    // An empty file staged as added: its patch has no hunk, so the diff draws no line.
    change(
      "src/empty.ts",
      { state: "absent" },
      present("src/empty.ts", text("src/empty.ts", "")),
      "diff --git a/src/empty.ts b/src/empty.ts\nnew file mode 100644\nindex 0000000..e69de29\n",
      "Staged",
    ),
    { key: "Unstaged:src/offline.ts", path: "src/offline.ts", stage: "Unstaged", status: "modified", content: { state: "disconnected", what: "change" } },
    // A change that arrived without a patch, which has no diff to draw.
    change("src/session.ts", present("src/session.ts", oldSession), present("src/session.ts", newSession), undefined, "Staged"),
  ];
}

export const viewerScenarios: Scenario[] = [
  {
    id: "viewer-files",
    group: GROUP,
    title: "Files",
    description:
      "Code in several languages (one file with lines wider than the viewer), Markdown that opens as a document (headings, lists, tables, fenced code, right-to-left text, and markup that must stay inert), every image format, an SVG and HTML whose scripts must not run, unsupported binary, a grammar that fails, files past the budgets, loading and failed reads. Previous / Next walks them in the same mounted viewer.",
    viewer: { subjects: files },
  },
  {
    id: "viewer-files-delayed",
    group: GROUP,
    title: "Files, each read taking a moment",
    description: "As Files, but each subject shows its loading state for 400 ms first, as a read from the daemon would.",
    viewer: { subjects: files, loadDelay: 400 },
  },
  {
    id: "viewer-changes",
    group: GROUP,
    title: "Changes",
    description:
      "Modified (one with lines wider than the viewer), added, deleted, renamed and type-changed files from patches, sides outside the project, image changes, a binary change, a patch past the budget, a patch that cannot be read and a change without a patch.",
    viewer: { subjects: changes },
  },
];
