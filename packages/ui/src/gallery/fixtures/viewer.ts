// The file viewer's gallery scenarios: representative files, images and changes, plus the inputs
// each fallback exists for — content past the render budgets, a body that is not an image, a patch
// the renderer cannot read, a grammar that fails to load, and repository markup that must stay
// inert — and a rendered document's diagrams, formulas, project images and links. The viewer
// renders them outside the app (see `viewerGallery.tsx`).
import { createElement } from "react";

import { format, type Translate } from "../../i18n/catalog";
import { isolate } from "../../i18n/isolate";
import type { Language } from "../../i18n/languages";
import type { FileContent } from "../../protocol";
import { RENDER_BUDGETS } from "../../viewer/budgets";
import { bodyFromFileContent, type ChangeBodies, ChangeBodiesError, type ViewerBody, type ViewerChangeSide, type ViewerSubject } from "../../viewer/content";
import { formatFileSize } from "../../viewer/format";
import { describing, type Ui } from "../interact";
import type { Scenario } from "../scenario";
import { IMAGES } from "./viewerImages";

const GROUP = "File viewer";

// The app's own modules are imported when a scenario's subjects are built, not here: the gallery
// frame reads the window's language from storage when `i18n/react` is first evaluated, which has to
// follow the preparation of that storage, and this module is loaded before it.
async function loadLiveParts() {
  return { Message: (await import("../../i18n/react")).Message, InlineReference: (await import("../../components/InlineReference")).InlineReference };
}
type LiveParts = Awaited<ReturnType<typeof loadLiveParts>>;

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

// Prose, lists, tables, fences, footnotes and right-to-left passages, plus inert markup: raw HTML,
// a remote image and a script link. No relative link: this text is also shown from paths where the
// target would not be a subject.
const GUIDE = `# Viewer guide

A paragraph with **bold**, _italic_, \`inline code\`, a [web link](https://example.com/guide), a bare https://example.com/bare address and a [script link](javascript:alert(1)).

## Lists and tasks

1. First step
2. Second step
   - a nested bullet
   - another one

- [x] A finished task
- [ ] An open task
  - An ordinary nested item
  - [ ] A nested task

A loose list keeps its text aligned with the tight list above:

- An ordinary item

- [x] A finished task with a second paragraph

  The second paragraph starts at the same text edge.

- [ ] An open task

  - [x] A nested finished task

  - An ordinary nested item

> A quoted remark that goes on for a while, so that the bar beside it spans more than one line of the document.

### A table

#### Column details

##### Small heading

The fifth heading level remains distinct from its paragraph.

###### Smallest heading

The sixth heading level keeps the same document text edge.

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

وهذه قائمة مفتوحة، بين عناصرها أسطر فارغة:

- عنصر أول

- عنصر ثان

> اقتباس بالعربية يمتد على أكثر من سطر واحد، ليظهر الشريط بجانبه من جهة النص نفسه.

| الاسم | النوع |
| --- | --- |
| ألف | نص |
| باء | صورة |

こんにちは、世界。これは日本語の段落です。

## Footnotes

A claim that needs a source[^1], and another[^2].

[^1]: The first source, with a [link](https://example.com/source).
[^2]: The second source.

A claim with a named footnote[^source].

[^source]: The source for the named footnote.

## Inert

![A remote picture that must not load](https://example.com/tracker.png)

<script>window.__viewerPwned = "markdown-script";</script>
<img src="https://example.com/raw.png" onerror="window.__viewerPwned = 'markdown-onerror'">
`;

// A document in Arabic, which reads from the right as a whole, with the blocks that may read
// otherwise or not at all: an English paragraph inside an Arabic list, a list that starts in English,
// one with no letters, and code.
const ARABIC_GUIDE = `# دليل العارض

يُقرأ هذا المستند كله من اليمين إلى اليسار، وفيه \`شيفرة مضمنة\` وروابط مثل [الموقع](https://example.com/guide).

## قوائم

- عنصر أول
- عنصر ثان

قائمة مفتوحة تفصل بين عناصرها أسطر فارغة، وفي عنصرها الثاني فقرة إنجليزية:

- عنصر أول

- عنصر ثان

  This paragraph is in English, inside an Arabic list.

* [x] مهمة منجزة
* [ ] مهمة مفتوحة
  - عنصر فرعي
  - [ ] مهمة فرعية

قائمة مهام مفتوحة مع فقرة ثانية وعنصر فرعي:

- عنصر عادي

- [x] مهمة منجزة

  تبدأ هذه الفقرة من حافة النص نفسها.

  This English continuation reads from the left while the task marker stays on the right.

- [ ] مهمة مفتوحة

  - [x] مهمة فرعية منجزة

  - عنصر فرعي عادي

- [ ] An English task within this Arabic-first loose list

+ Starts in English
+ ثم يتابع بالعربية

- 2024
- 2025

2024 – 2025

> اقتباس بالعربية يمتد على أكثر من سطر واحد، ليظهر الشريط بجانبه من جهة النص نفسه.

> A quotation in English inside this Arabic document.

## جدول

| الاسم | النوع | ملاحظات |
| --- | --- | --- |
| ألف | نص | الأول |
| باء | صورة | خلية أطول بكثير يجب أن تلتف داخل عمودها بدل أن توسع الجدول |

## شيفرة

\`\`\`ts
export const answer = 42;
\`\`\`
`;

// A document with no letter at all, so that nothing in it decides its direction and it takes the
// interface's.
const FIGURES = `# 2024

1 + 1 = 2

- 1
- 2

> 3

| 1 | 2 |
| - | - |
| 3 | 4 |
`;

// One compact document with the elements a style has to dress, so that the styles can be compared:
// headings, a paragraph with inline marks, a footnote and a long identifier in inline code, nested
// and ordered lists with a fence inside an item, a task list, a table, a blockquote, a fenced block,
// a block formula, a diagram, a rule and links. The fence, the formula and the diagram sit in
// `not-prose` blocks, which no style may set as text.
const STYLE_SAMPLE = `# Release handbook

A short introduction with **bold**, _italic_, ~~struck~~ text, \`inline code\` and a [link to the guide](https://example.com/guide), followed by enough words to wrap onto a second line so that the measure of the page can be read.[^1]

Internationalization of the changelog characteristically produces incompatibilities between the archive named \`renderReleaseArchiveWithDetachedSignatureAndChecksumManifest\` and the tooling that unpacks it, which is why the long identifier in this paragraph has to break only where there is no room left.

## Shipping a release

### Before you start

1. Update the changelog.
2. Tag the commit:
   - use the version from \`package.json\`
   - sign the tag
3. Publish the archive with the helper below, which must not be justified or hyphenated like the text around it:
   \`\`\`sh
   ./scripts/publish-release-archive.sh --channel stable --checksum-manifest SHA256SUMS --sign
   \`\`\`

- [x] Tests pass
- [x] Notes written
- [ ] Announcement sent

#### Checks by platform

| Platform | Architecture | Status |
| :--- | :-: | ---: |
| macOS | arm64 | ready |
| Linux | x86_64 | in review |

> A release is only as good as the last thing that was checked, and that thing is rarely the code.

\`\`\`ts
export function bump(version: string): string {
  const [major, minor, patch] = version.split(".").map(Number);
  return [major, minor, patch + 1].join(".");
}
\`\`\`

The chance that a release goes out unchecked falls with every reviewer:

$$
P_{\\text{unchecked}} = \\prod_{i=1}^{n} (1 - r_i)
$$

\`\`\`mermaid
flowchart LR
  Tag --> Build --> Sign --> Publish
\`\`\`

---

Read the [full guide](https://example.com/guide) or the [changelog](https://example.com/changelog) for more.

[^1]: The measure is read from the longest line of this paragraph.
`;

// A rendered document that scrolls sideways: a table whose cells hold download addresses and a
// checksum, which have no place to break, so the table is wider than the viewer, and long enough
// to scroll down as well.
const releaseRows = () =>
  Array.from({ length: 30 }, (_, i) => {
    const version = `1.4.${i}`;
    const sum = Array.from({ length: 64 }, (_, at) => ((i * 7 + at * 13) % 16).toString(16)).join("");
    const archive = `https://downloads.example.com/octoboard/releases/${version}/octoboard-${version}-aarch64-apple-darwin.tar.gz`;
    return `| ${version} | ${archive} | ${archive}.minisig | \`${sum}\` |`;
  }).join("\n");
const RELEASES = `# Releases

Every archive is signed; check its SHA-256 before installing.

| Version | Download | Signature | SHA-256 |
| :--- | :--- | :--- | :--- |
${releaseRows()}
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

const BROKEN_SVG = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"";

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

// A file from the tree has no source: only a committed change opened from the Changes view says where it is from.
// A live file is the project's, so the header offers File manager and Default application on it.
function file(path: string, body: ViewerBody): ViewerSubject {
  return { key: `live:${path}`, path, host: hostOf(path), content: { state: "file", body } };
}

const hostOf = (path: string) => ({ project: "gallery", path });

function present(path: string, body?: ViewerBody, kind: "file" | "symlink" | "submodule" = "file"): ViewerChangeSide {
  return { state: "present", path, kind, body };
}

// Generated content around the budgets. A 2 MB log is past the highlighting budget; a minified line
// is far past the tokenizing line length; 9,000 lines of code sit just under the line budget.
function generated(lines: number, line: (index: number) => string): string {
  return Array.from({ length: lines }, (_, index) => line(index)).join("\n") + "\n";
}
// Most entries are short; every hundredth carries the request that was served as one unbroken line,
// so the plain text scrolls sideways as well as down.
const LOG_REQUEST = "GET /api/v1/projects/octoboard/sessions?filter[status]=waiting&filter[agent]=claude&include=console,branch,changes&sort=-lastActivity&page[size]=200 {\"user\":\"anonymous\",\"agent\":\"octoboard-ui/1.4.2\",\"trace\":\"3fa85f64-5717-4562-b3fc-2c963f66afa6\"}";
const largeLog = () =>
  generated(40_000, (i) => {
    const entry = `2026-10-08T12:${String(i % 60).padStart(2, "0")}:00Z INFO request ${i} served in ${i % 97} ms`;
    return i % 100 === 7 ? `${entry} ${LOG_REQUEST}` : entry;
  });
const minified = () => `!function(){${generated(1, () => Array.from({ length: 4_000 }, (_, i) => `var a${i}=${i}*2;`).join(""))}}();`;
const nearBudget = () => generated(9_000, (i) => `export const value${i} = { id: ${i}, label: "Item ${i}", enabled: ${i % 2 === 0} };`);
// A file renamed into a path below itself (`lib` to `lib/index.ts`): one section, the diff between its two versions.
const BELOW_ITSELF_PATCH = `diff --git a/lib b/lib/index.ts
index 3b18e51..9c8d7e6 100644
--- a/lib
+++ b/lib/index.ts
@@ -1,3 +1,3 @@
-export const version = "2.3";
+export const version = "2.4";
 export const name = "lib";
 export default version;
`;
// The reverse, `lib/index.ts` to `lib`.
const OUT_OF_ITSELF_PATCH = `diff --git a/lib/index.ts b/lib
index 3b18e51..9c8d7e6 100644
--- a/lib/index.ts
+++ b/lib
@@ -1,3 +1,3 @@
-export const version = "2.3";
+export const version = "2.4";
 export const name = "lib";
 export default version;
`;
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

// A change within the diff budget that is still far taller than the viewer: every row of a table
// edited, so the rendered diff scrolls.
const longPatch = () =>
  "diff --git a/data/rows.ts b/data/rows.ts\nindex 1111111..2222222 100644\n--- a/data/rows.ts\n+++ b/data/rows.ts\n@@ -1,150 +1,150 @@\n" +
  generated(150, (i) => `-export const row${i} = ${i};\n+export const row${i} = ${i + 1};`);

/** Shared file subjects, built only when a viewer scenario opens; navigation selects its explicit
 * normal or exceptional targets below. */
async function allFiles(): Promise<ViewerSubject[]> {
  const { DaemonRequestError } = await import("../../daemon-client");
  return [
    file("src/session.ts", text("src/session.ts", NEW_SESSION)),
    file("scripts/build.py", text("scripts/build.py", PYTHON)),
    file("crates/words/src/lib.rs", text("crates/words/src/lib.rs", RUST)),
    file("src/describe.ts", text("src/describe.ts", longLines())),
    file("README.md", text("README.md", MARKDOWN)),
    file("docs/guide.md", text("docs/guide.md", GUIDE)),
    file("docs/arabic.md", text("docs/arabic.md", ARABIC_GUIDE)),
    file("docs/figures.md", text("docs/figures.md", FIGURES)),
    file("docs/releases.md", text("docs/releases.md", RELEASES)),
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
    { key: "live:src/pending.ts", path: "src/pending.ts", content: { state: "loading" } },
    {
      key: "live:src/gone.ts",
      path: "src/gone.ts",
      content: {
        state: "error",
        message: new DaemonRequestError("src/gone.ts does not exist any more.", "file_not_found", { path: "src/gone.ts" }),
      },
    },
    { key: "live:src/offline.ts", path: "src/offline.ts", content: { state: "disconnected", what: "file" } },
    // Names too long for the header, for both source files and design notes.
    file(
      "packages/settings/src/components/notification-preferences/an-extraordinarily-long-component-file-name-that-keeps-going.tsx",
      text("an-extraordinarily-long-component-file-name-that-keeps-going.tsx", NEW_SESSION),
    ),
    file(DESIGN_PATH, text("notes.md", MARKDOWN)),
    // A generated name with no place to break a line.
    file(
      "dist/assets/chunk.3f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c.js",
      text("chunk.js", JSON_TEXT),
    ),
  ];
}

// Explicit confirmation targets, shared by normal and failure navigation without mixing them.
const FAILED_FILE_PATHS = new Set(["assets/broken.svg", "src/strange.brokengrammar", "src/gone.ts", "src/offline.ts"]);
async function files(): Promise<ViewerSubject[]> {
  return (await allFiles()).filter(({ path }) => !FAILED_FILE_PATHS.has(path));
}
async function failedFiles(): Promise<ViewerSubject[]> {
  const { registerCustomLanguage } = await import("@pierre/diffs");
  registerCustomLanguage("octoboard-broken-grammar", () => Promise.reject(new Error("Gallery: this grammar fails to load")), ["brokengrammar"]);
  const subjects = (await allFiles()).filter(({ path }) => FAILED_FILE_PATHS.has(path));
  // Start with the failed read so the exceptional purpose is visible immediately.
  return [subjects.find(({ path }) => path === "src/gone.ts")!, ...subjects.filter(({ path }) => path !== "src/gone.ts")];
}

// The comparison wording of a committed change opened from the Changes view, in the window's language.
// The branches and commits are set as code, as the app sets them, with the same wording as plain text
// for the tooltip.
const comparisonWording = ({ Message, InlineReference }: LiveParts, t: Translate, from: string, fromCommit: string, to: string, toCommit: string) => {
  return {
    source: createElement(Message<"git.compare.source">, {
      id: "git.compare.source",
      params: {
        from: createElement(InlineReference, { monospace: true, wrap: true, color: "default" }, from),
        fromCommit: createElement(InlineReference, { monospace: true, wrap: true, color: "default" }, fromCommit),
        to: createElement(InlineReference, { monospace: true, wrap: true, color: "default" }, to),
        toCommit: createElement(InlineReference, { monospace: true, wrap: true, color: "default" }, toCommit),
      },
    }),
    sourceText: t("git.compare.source", { from: isolate(from), fromCommit: isolate(fromCommit), to: isolate(to), toCommit: isolate(toCommit) }),
  };
};
const longComparison = (parts: LiveParts, t: Translate) =>
  comparisonWording(parts, t, "release/2024-11-stabilisation-and-hotfix-candidates", "1234567", "feature/ranking-experiments-with-a-long-name", "fedcba0");
/** A one-hunk patch of a Markdown file, from `oldPath` to `newPath` (the same for a plain edit). */
const markdownPatch = (oldPath: string, newPath: string) => `diff --git a/${oldPath} b/${newPath}
index 1111111..2222222 100644
--- a/${oldPath}
+++ b/${newPath}
@@ -1,3 +1,3 @@
 # Viewer guide
-A paragraph with **bold**.
+A paragraph with **bold** and _italic_.
 
`;
const DESIGN_PATH = "docs/design-notes/interaction-and-visuals/project-file-browser-interaction-and-visual-design-notes-final-review-do-not-edit-directly.md";
const designComparison = (parts: LiveParts, t: Translate) =>
  comparisonWording(
    parts,
    t,
    "release/november-2024-stabilisation-and-hotfix-candidates",
    "1234567",
    "feature/ranking-experiments-with-a-long-name-for-checking-line-wrap",
    "fedcba0",
  );
const oldSession = text("src/session.ts", OLD_SESSION);
const newSession = text("src/session.ts", NEW_SESSION);
const before = binary("assets/logo.png", IMAGES.before_png);
const after = binary("assets/logo.png", IMAGES.after_png);

function change(t: Translate, path: string, oldSide: ViewerChangeSide, newSide: ViewerChangeSide, patch?: string, stage: "staged" | "unstaged" = "unstaged"): ViewerSubject {
  // A change in the working copy has its file to show in the file manager, unless it is deleted there.
  const host = newSide.state === "present" ? hostOf(path) : undefined;
  return { key: `${stage}:${path}`, path, host, stage: t(stage === "staged" ? "git.group.staged" : "git.group.unstaged"), content: { state: "change", change: { old: oldSide, new: newSide, patch } } };
}

// An untracked file is shown alone, with the Untracked chip and no stage tag.
function untracked(path: string, body: ViewerBody): ViewerSubject {
  return { key: `Untracked:${path}`, path, host: hostOf(path), status: "untracked", content: { state: "change", change: { old: { state: "absent" }, new: present(path, body) } } };
}

// An untracked symbolic link, or a repository of its own, is shown alone with nothing to read on its side.
function untrackedNotFile(path: string, kind: "symlink" | "submodule"): ViewerSubject {
  return { key: `Untracked:${path}`, path, host: hostOf(path), status: "untracked", content: { state: "change", change: { old: { state: "absent" }, new: present(path, undefined, kind) } } };
}

/** A committed change opened from the Changes view: the comparison is the subtitle under the title. */
function compared(parts: LiveParts, t: Translate): ViewerSubject {
  const wording = comparisonWording(parts, t, "main", "1234567", "feature", "fedcba0");
  return {
    key: "Compare:src/session.ts",
    path: "src/session.ts",
    ...wording,
    content: { state: "change", change: { old: present("src/session.ts", oldSession), new: present("src/session.ts", newSession), patch: MODIFIED_PATCH } },
  };
}

async function allChanges(language: Language): Promise<ViewerSubject[]> {
  const parts = await loadLiveParts();
  const t: Translate = (key, ...args) => format(language, key, ...args);
  return [
    change(t, "src/session.ts", present("src/session.ts", oldSession), present("src/session.ts", newSession), MODIFIED_PATCH),
    change(t, "src/describe.ts", present("src/describe.ts"), present("src/describe.ts"), LONG_LINES_PATCH),
    change(t, "src/badge.ts", { state: "absent" }, present("src/badge.ts"), ADDED_PATCH),
    change(t, "src/legacy.ts", present("src/legacy.ts"), { state: "absent" }, DELETED_PATCH),
    change(t, "src/badgeLabel.ts", present("src/badge.ts"), present("src/badgeLabel.ts"), RENAMED_PATCH),
    change(t, "src/current.ts", present("src/current.ts", undefined, "symlink"), present("src/current.ts", newSession), TYPE_CHANGE_PATCH),
    change(t, "lib/index.ts", present("lib"), present("lib/index.ts"), BELOW_ITSELF_PATCH),
    change(t, "lib", present("lib/index.ts"), present("lib"), OUT_OF_ITSELF_PATCH),
    change(t, "src/moved-in.ts", { state: "out_of_scope", repositoryPath: "packages/shared/src/moved-in.ts" }, present("src/moved-in.ts", newSession)),
    change(t, "src/moved-out.ts", present("src/moved-out.ts", oldSession), { state: "out_of_scope", repositoryPath: "packages/shared/src/moved-out.ts" }),
    change(t, "assets/logo.png", present("assets/logo.png", before), present("assets/logo.png", after)),
    untracked("assets/new.png", after),
    untracked("src/scratch.ts", text("src/scratch.ts", NEW_SESSION)),
    untracked("docs/draft.md", text("docs/draft.md", MARKDOWN)),
    untrackedNotFile("src/config.local.ts", "symlink"),
    untrackedNotFile("vendor/search-client", "submodule"),
    {
      key: "Conflicted:docs/merge.md",
      path: "docs/merge.md",
      host: hostOf("docs/merge.md"),
      content: {
        state: "conflict",
        conflict: t("git.conflict.bothModified"),
        body: text("docs/merge.md", "<<<<<<< ours\n# Release notes\n=======\n# Changes\n>>>>>>> theirs\n"),
      },
    },
    {
      key: "Conflicted:src/merge.ts",
      path: "src/merge.ts",
      host: hostOf("src/merge.ts"),
      content: {
        state: "conflict",
        conflict: t("git.conflict.bothModified"),
        body: text("src/merge.ts", "<<<<<<< ours\nconst a = 1;\n=======\nconst a = 2;\n>>>>>>> theirs\n"),
      },
    },
    compared(parts, t),
    // A long comparison under the title, and a Markdown file shown alone from the Changes view, which
    // has the source / document choice and the wrap choice in source (a Markdown diff has the layout
    // and wrap choices and no document, so all three never show together).
    {
      key: "Compare:docs/guide.md",
      path: "docs/guide.md",
      ...longComparison(parts, t),
      content: { state: "change", change: { old: { state: "absent" }, new: present("docs/guide.md", text("docs/guide.md", GUIDE)) } },
    },
    change(
      t,
      "docs/handbook/engineering/onboarding/renamed-guide.md",
      present("docs/handbook/engineering/onboarding/previous-locations/legacy/old-guide.md", text("old-guide.md", GUIDE)),
      present("docs/handbook/engineering/onboarding/renamed-guide.md", text("renamed-guide.md", GUIDE)),
      "diff --git a/docs/handbook/engineering/onboarding/previous-locations/legacy/old-guide.md b/docs/handbook/engineering/onboarding/renamed-guide.md\nsimilarity index 100%\nrename from docs/handbook/engineering/onboarding/previous-locations/legacy/old-guide.md\nrename to docs/handbook/engineering/onboarding/renamed-guide.md\n",
      "staged",
    ),
    // The tallest header: a Compare-view rename with long branches and a long origin, the comparison
    // a subtitle row of its own and the origin a row of its own, and a Markdown diff with the layout
    // and wrap choices.
    {
      key: "Compare:docs/handbook/renamed-notes.md",
      path: "docs/handbook/engineering/onboarding/renamed-notes.md",
      ...longComparison(parts, t),
      content: {
        state: "change",
        change: {
          old: present("docs/handbook/engineering/onboarding/previous-locations/legacy/old-notes.md", text("old-notes.md", GUIDE)),
          new: present("docs/handbook/engineering/onboarding/renamed-notes.md", text("renamed-notes.md", GUIDE)),
          patch: markdownPatch(
            "docs/handbook/engineering/onboarding/previous-locations/legacy/old-notes.md",
            "docs/handbook/engineering/onboarding/renamed-notes.md",
          ),
        },
      },
    },
    // Long design-note paths and branch names make the subtitle wrap.
    {
      key: `Compare:${DESIGN_PATH}`,
      path: DESIGN_PATH,
      ...designComparison(parts, t),
      content: { state: "change", change: { old: { state: "absent" }, new: present(DESIGN_PATH, text("design-notes.md", GUIDE)) } },
    },
    // Before cannot be drawn and falls back to its text, which is too long to highlight; After is drawn.
    // Each side's marks stand beside its own heading.
    change(t, "assets/sides.svg", present("assets/sides.svg", text("assets/sides.svg", OVERSIZED_BROKEN_SVG)), present("assets/sides.svg", text("assets/sides.svg", SVG))),
    change(t, "bin/tool.wasm", present("bin/tool.wasm", binary("bin/tool.wasm", { mediaType: null, data: "AGFzbQEAAAA=" })), present("bin/tool.wasm", binary("bin/tool.wasm", { mediaType: null, data: "AGFzbQEAAAABBAFgAAA=" }))),
    change(t, "data/rows.ts", present("data/rows.ts"), present("data/rows.ts"), longPatch()),
    change(t, "data/table.ts", present("data/table.ts"), present("data/table.ts"), largePatch()),
    // A hunk header that promises three lines per side and delivers two: the renderer rejects it,
    // and the viewer falls back to the raw patch.
    change(
      t,
      "src/garbled.ts",
      present("src/garbled.ts"),
      present("src/garbled.ts"),
      "diff --git a/src/garbled.ts b/src/garbled.ts\n--- a/src/garbled.ts\n+++ b/src/garbled.ts\n@@ -1,3 +1,3 @@\n a\n+b\n",
    ),
    // An empty file staged as added: its patch has no hunk, so the diff draws no line.
    change(
      t,
      "src/empty.ts",
      { state: "absent" },
      present("src/empty.ts", text("src/empty.ts", "")),
      "diff --git a/src/empty.ts b/src/empty.ts\nnew file mode 100644\nindex 0000000..e69de29\n",
      "staged",
    ),
    { key: "Unstaged:src/offline.ts", path: "src/offline.ts", stage: t("git.group.unstaged"), status: "modified", content: { state: "disconnected", what: "change" } },
    // A rename whose two versions are equal: git's diff of them is empty.
    change(t, "src/kept.ts", present("src/previous.ts", oldSession), present("src/kept.ts", oldSession), "", "staged"),
    // Two versions that are the same file: git's diff of them is empty, and with no way to read the
    // whole file the viewer can only say nothing differs.
    change(t, "src/unchanged.ts", present("src/unchanged.ts", oldSession), present("src/unchanged.ts", oldSession), "", "staged"),
    // Two sides holding the same text, whose whole content the viewer reads itself: the file is shown
    // with a note about it after the path. Under a rename it says so; where the read cannot be had,
    // the stand-in stays and the note says why.
    readable(change(t, "src/identical.ts", present("src/identical.ts", oldSession), present("src/identical.ts", oldSession), "", "staged"), async () => ({ old: OLD_SESSION, new: OLD_SESSION })),
    readable(change(t, "src/kept-renamed.ts", present("src/previous-name.ts", oldSession), present("src/kept-renamed.ts", oldSession), "", "staged"), async () => ({ old: OLD_SESSION, new: OLD_SESSION })),
    readable(change(t, "src/unreadable.ts", present("src/unreadable.ts", oldSession), present("src/unreadable.ts", oldSession), "", "staged"), () => Promise.reject(new ChangeBodiesError("unavailable"))),
    // A change that arrived without a patch, which has no diff to draw.
    change(t, "src/session.ts", present("src/session.ts", oldSession), present("src/session.ts", newSession), undefined, "staged"),
  ];
}

/** `subject`, a change, with a way to read the whole text of its two sides. */
function readable(subject: ViewerSubject, loadBodies: () => Promise<ChangeBodies>): ViewerSubject {
  if (subject.content.state !== "change") return subject;
  return { ...subject, content: { state: "change", change: { ...subject.content.change, loadBodies } } };
}

/** A diagram whose source is past the per-diagram length. An over-long diagram does not take a
 * count slot, so it does not turn the diagrams above it into source. */
function diagramPastBudget(): string {
  const unit = "A --> B ";
  return unit.repeat(Math.ceil((RENDER_BUDGETS.documentDiagramChars + 1) / unit.length));
}

// One note that holds the document states that can be on screen together. The other file is only
// there so a relative link has a subject to open, with lines above the heading the link names:
// opening it on that heading is not the same as opening it at the top.
const FAILED_CHANGE_KEYS = new Set(["Conflicted:docs/merge.md", "Conflicted:src/merge.ts", "unstaged:src/garbled.ts", "Unstaged:src/offline.ts", "staged:src/unreadable.ts", "unstaged:assets/sides.svg"]);
async function changes(language: Language): Promise<ViewerSubject[]> {
  return (await allChanges(language)).filter(({ key }) => !FAILED_CHANGE_KEYS.has(key));
}
async function failedChanges(language: Language): Promise<ViewerSubject[]> {
  return (await allChanges(language)).filter(({ key }) => FAILED_CHANGE_KEYS.has(key));
}

const NOTES =
  `# Notes

` +
  // Mermaid 12.2.0 rejects accTitle and accDescr before the diagram type ("No diagram type detected").
  // They stay after flowchart LR, the order this scenario relies on, which produces the SVG title "Request flow".
  `\`\`\`mermaid
flowchart LR
accTitle: Request flow
accDescr: A request goes from the client to the index.
  Client --> Index --> Reply
\`\`\`

\`\`\`mermaid
flowchart LR
  A --> B
\`\`\`

\`\`\`mermaid
flowchart LR
  Client -->
\`\`\`

The note continues under the diagram that could not be drawn.

It costs $20 and $30, and $100/2$ of it, and $E=mc^2$.

$$
\\int_0^1 x^2 \\, dx
$$

This formula is unfinished: $\\frac{1}{$.

The same unfinished formula on its own line:

$$
\\frac{1}{
$$

\`\`\`math
x = y
\`\`\`

مرحبا $E=mc^2$ بالعالم.

![A small picture](picture.png)

![A picture outside the project](../../outside.png)

![The missing figure](missing.png)

A linked figure that is missing: [![The linked figure](missing-linked.png)](./other.md) is a link to the other note.

See [the other note](./other.md#kept) and [the heading below](#kept-aside).

## Kept aside

What the link in this note scrolls to.
`;

// A document that spends the formulas it may draw, then asks for two more (one in a line of text, one
// of its own), and has a diagram too long to draw. What is past a budget is left as its source with a
// note at its spot. Built from the budget itself, so a change to it moves the document with it.
const spentFormulas = () => Array.from({ length: RENDER_BUDGETS.documentFormulas }, (_, i) => `$x_{${i}}$`).join(" ");
const LIMITS = () => `# Past the budgets

The formulas below are drawn, up to the most a document may draw. ${spentFormulas()} The next one, $y^2$, is left as its source, with a note where it stands.

A formula on its own line comes after them too:

$$
z^2
$$

A diagram whose source is too long is left as its source:

\`\`\`mermaid
${diagramPastBudget()}
\`\`\`
`;

const INLINE_FAILURE = `# Trip notes

We left early and the road was quiet for the first hour, with fields on both sides and very little traffic.

The route we took is drawn here ![The route](route.png) and the stops are listed below it, in the order we made them.

## Stops

- The first stop was for coffee.
- The second was for the view.
`;

const OTHER_LEAD = Array.from({ length: 50 }, (_, index) => `Line ${index + 1} of the other note, above its heading.`).join("\n\n");

const OTHER = `# Other note

${OTHER_LEAD}

## Kept

What the link from the first note opens this file on.
`;

const NORMAL_NOTES = `# A project note

\`\`\`mermaid
flowchart LR
accTitle: Request flow
accDescr: A request goes from the client to the index.
  Client --> Index --> Reply
\`\`\`

\`\`\`mermaid
flowchart LR
  A --> B
\`\`\`

It costs $20 and $30, and $100/2$ of it, and $E=mc^2$.

$$
\\int_0^1 x^2 \\, dx
$$

مرحبا $E=mc^2$ بالعالم.

![A small picture](picture.png)

See [the other note](./other.md#kept) and [the heading below](#kept-aside).

## Kept aside

What the link in this note scrolls to.
`;
type Refusal = { message: string; code: string; params?: Record<string, string> };

function failure(id: string, title: string, description: string, slot: "file" | "conflict" | "change", path: string, refusal: Refusal | ((t: Translate, language: Language) => string), steps?: Scenario["steps"]): Scenario {
  return {
    id, group: GROUP, category: "exceptional", title, description, steps,
    viewer: {
      subjects: async (language) => {
        const { DaemonRequestError } = await import("../../daemon-client");
        const { displayWirePath } = await import("../../wirePath");
        const t: Translate = (key, ...args) => format(language, key, ...args);
        const message = typeof refusal === "function" ? refusal(t, language) : new DaemonRequestError(refusal.message, refusal.code, { path: displayWirePath(path), ...refusal.params });
        switch (slot) {
          case "file":
            return [{ key: `live:${path}`, path, host: hostOf(path), content: { state: "error", message } }];
          case "conflict":
            return [{ key: `Conflicted:${path}`, path, host: hostOf(path), content: { state: "conflict", conflict: t("git.conflict.bothModified"), message } }];
          case "change":
            return [{ key: `unstaged:${path}`, path, host: hostOf(path), stage: t("git.group.unstaged"), status: "modified", content: { state: "change", change: { old: present(path), new: present(path), unavailable: message } } }];
        }
      },
    },
  };
}

const failureScenarios: Scenario[] = [
  failure("viewer-file-error-not-found", "Failed read: file gone", "The explanation says this file does not exist any more without repeating the path the header names.", "file", "docs/plans/20261020-ui-standard-audit/alert-verification.md", { message: "docs/plans/20261020-ui-standard-audit/alert-verification.md does not exist any more.", code: "file_not_found" }),
  failure("viewer-file-error-invalid-path", "Failed read: name not valid in the project", "The header shows the name that is not valid UTF-8 in its display form; the explanation refers to it as this path rather than repeating it.", "file", "docs/%FFreport%2520final.md", { message: "docs/�report%20final.md is not a valid path in this project.", code: "invalid_path" }),
  failure("viewer-file-error-permission", "Failed read: permission refused", "The path is left to the header: the explanation says only that Octoboard is not allowed to read this file, and the system's reason (Permission denied, os error 13) is behind Details, not in the sentence.", "file", "config/secrets.env", { message: "Octoboard is not allowed to read config/secrets.env: Permission denied (os error 13)", code: "permission_denied", params: { detail: "Permission denied (os error 13)" } }),
  failure("viewer-file-error-outside-scope", "Failed read: link leads outside the project", "The explanation says the file leads outside this project without repeating its path.", "file", "docs/shared-notes.md", { message: "docs/shared-notes.md leads outside this project, so it is not shown.", code: "outside_scope" }),
  failure("viewer-file-error-not-regular", "Failed read: not a regular file", "A named pipe: the explanation says this is not a regular file without repeating its path.", "file", "logs/live.pipe", { message: "logs/live.pipe is not a regular file.", code: "unsupported_file_type", params: { file_type: "fifo" } }),
  failure("viewer-file-error-folder-denied", "Failed read: project folder refused", "The refusal names the project's folder, not this file, so its absolute path stays in the explanation; the system's reason (Operation not permitted, os error 1) is behind Details, not in the sentence.", "file", "src/session.ts", { message: "Octoboard is not allowed to read /Users/dev/Projects/search-api: Operation not permitted (os error 1)", code: "permission_denied", params: { path: "/Users/dev/Projects/search-api", detail: "Operation not permitted (os error 1)" } }),
  failure("viewer-file-error-too-large", "Failed read: file too large", "The explanation gives the file's size and the most that can be opened here, without repeating the path the header names.", "file", "data/export.csv", (t, language) => t("browser.error.fileTooLarge", { size: formatFileSize(language, 5_242_880), max: formatFileSize(language, 4_194_304) })),
  failure("viewer-file-error-too-large-no-size", "Failed read: file too large, size unknown", "Without the file's size, the explanation gives only the most that can be opened here.", "file", "data/stream.log", (t, language) => t("browser.error.fileTooLargeNoSize", { max: formatFileSize(language, 4_194_304) })),
  failure("viewer-file-error-source-unavailable", "Failed read: worktree folder unavailable", "The folder the read is scoped to is gone, so its absolute path stays in the explanation; the system's reason (No such file or directory, os error 2) is behind Details, not in the sentence.", "file", "src/session.ts", { message: "The folder /Users/dev/Projects/search-api-review cannot be read: No such file or directory (os error 2)", code: "source_unavailable", params: { path: "/Users/dev/Projects/search-api-review", cause: "missing", detail: "No such file or directory (os error 2)" } }),
  failure("viewer-conflict-error-not-found", "Conflict whose file on disk is gone", "The pane holds the empty-state failure \"Could not open this file\" with the explanation that this file does not exist any more, without repeating the path. The header has no marks: the title's conflict chip is what says the path is in conflict.", "conflict", "docs/merge.md", { message: "docs/merge.md does not exist any more.", code: "file_not_found" }),
  failure("viewer-change-error-unmerged", "Change read: conflicting index entry", "The change's explanation says the file is in conflict without repeating its path.", "change", "src/merge.ts", { message: "src/merge.ts is not a regular file.", code: "unsupported_file_type", params: { file_type: "unmerged" } }),
  failure("viewer-change-error-changing", "Change read: still changing", "A failure the app words itself, with no code behind it, is shown as it is.", "change", "src/session.ts", (t) => t("git.error.changeChanging")),
];

const EXPAND_FAILURE_PATCH = `diff --git a/src/pipeline.ts b/src/pipeline.ts
index 1111111..2222222 100644
--- a/src/pipeline.ts
+++ b/src/pipeline.ts
@@ -20,3 +20,3 @@
 export function run() {
-  return stage(1);
+  return stage(2);
 }
`;

function bodiesFailure(id: string, title: string, description: string, build: (t: Translate, refuse: (header: string, refusal: Refusal & { params: { path: string } }) => () => Promise<never>) => ViewerSubject, steps?: Scenario["steps"]): Scenario {
  return {
    id, group: GROUP, category: "exceptional", title, description,
    viewer: {
      subjects: async (language) => {
        const { DaemonRequestError } = await import("../../daemon-client");
        const { changeBodiesFailure } = await import("../../viewer/viewerError");
        const t: Translate = (key, ...args) => format(language, key, ...args);
        return [build(t, (header, refusal) => () => {
          const error = new DaemonRequestError(refusal.message, refusal.code, refusal.params);
          return Promise.reject(changeBodiesFailure(t, language, error, header, { consoles: new Map(), projects: new Map(), sessions: new Map() }));
        })];
      },
    },
    steps,
  };
}

/** The mark of a changed file's header that says its content could not be read or shown, whatever its reason says
 * after its fixed opening words. */
const failureMark = (ui: Ui, key: "viewer.expand.failed" | "viewer.change.contentNotShownBecause" | "viewer.change.contentNotShown") => {
  const opening = ui.t(key, { reason: "\0" }).split("\0")[0];
  return describing(`the failure mark opening "${opening}"`, (name) => name.startsWith(opening));
};

const counted = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", lines: 40 });

const bodiesFailureScenarios: Scenario[] = [
  bodiesFailure(
    "viewer-change-expand-error", "Unmodified lines that could not be read",
    "A request for unmodified lines has failed: a danger mark after the path says in its popover that Octoboard (a chip) is not allowed to read the file, without repeating its path, then the system's reason (Permission denied, os error 13) in a block of its own under that sentence (the popover is held open here), and the separator it came from offers to try again.",
    (t, refuse) => ({
      key: "unstaged:src/pipeline.ts", path: "src/pipeline.ts", host: hostOf("src/pipeline.ts"), stage: t("git.group.unstaged"),
      content: { state: "change", change: { old: counted("src/pipeline.ts"), new: counted("src/pipeline.ts"), patch: EXPAND_FAILURE_PATCH, loadBodies: refuse("src/pipeline.ts", { message: "Octoboard is not allowed to read src/pipeline.ts: Permission denied (os error 13)", code: "permission_denied", params: { path: "src/pipeline.ts", detail: "Permission denied (os error 13)" } }) } },
    }),
    [(ui) => ui.pressInDiff(ui.t("viewer.expand.above")), (ui) => ui.hoverStatusMark(failureMark(ui, "viewer.expand.failed"))],
  ),
  bodiesFailure(
    "viewer-change-identical-error", "Unchanged file whose content could not be read",
    "A warning mark (a triangle) after the path says in its popover that the file's content could not be shown, with the reason that the file does not exist any more, without repeating its path and with no raw text under it since there is none (the popover is held open here); the \"No differences\" stand-in fills the pane.",
    (t, refuse) => ({
      key: "staged:src/unchanged.ts", path: "src/unchanged.ts", host: hostOf("src/unchanged.ts"), stage: t("git.group.staged"),
      content: { state: "change", change: { old: present("src/unchanged.ts"), new: present("src/unchanged.ts"), patch: "", loadBodies: refuse("src/unchanged.ts", { message: "src/unchanged.ts does not exist any more.", code: "file_not_found", params: { path: "src/unchanged.ts" } }) } },
    }),
    [(ui) => ui.hoverStatusMark(failureMark(ui, "viewer.change.contentNotShownBecause"))],
  ),
  bodiesFailure(
    "viewer-change-unexpected-error", "Unchanged file whose content failed in a way the application has no words for",
    "A warning mark (a triangle) after the path says in its popover only \"Could not show this file's content.\" with no cause appended, since the application has no words for this failure, then the daemon's own text for it (an unexpected session state, in English whatever the language) in a block of its own under it, held open here; the \"No differences\" stand-in fills the pane.",
    (t, refuse) => ({
      key: "staged:src/unchanged.ts", path: "src/unchanged.ts", host: hostOf("src/unchanged.ts"), stage: t("git.group.staged"),
      content: { state: "change", change: { old: present("src/unchanged.ts"), new: present("src/unchanged.ts"), patch: "", loadBodies: refuse("src/unchanged.ts", { message: "Unexpected session state while reading the file", code: "internal_error", params: { path: "src/unchanged.ts", detail: "Unexpected session state while reading the file" } }) } },
    }),
    [(ui) => ui.hoverStatusMark(failureMark(ui, "viewer.change.contentNotShown"))],
  ),
  bodiesFailure(
    "viewer-change-renamed-unchanged-error", "Unchanged rename whose original could not be read",
    "The warning mark (a triangle) after the path says in its popover that the file's content could not be shown, with the reason naming the original path, which is not the one in the header, so it stays in the sentence as a path chip (the popover is held open here); the \"Renamed without changes\" stand-in fills the pane.",
    (t, refuse) => ({
      key: "staged:src/kept.ts", path: "src/kept.ts", host: hostOf("src/kept.ts"), stage: t("git.group.staged"),
      content: { state: "change", change: { old: present("src/previous.ts"), new: present("src/kept.ts"), patch: "", loadBodies: refuse("src/kept.ts", { message: "src/previous.ts does not exist any more.", code: "file_not_found", params: { path: "src/previous.ts" } }) } },
    }),
    [(ui) => ui.hoverStatusMark(failureMark(ui, "viewer.change.contentNotShownBecause"))],
  ),
  {
    id: "viewer-image-error", group: GROUP, category: "exceptional",
    title: "Image that cannot be decoded",
    description: "The empty-state failure stands in place of the image: a danger glyph and a heading saying the image could not be shown, with the sentence that the file may be damaged, too large or unsupported as the line under it, then the remedy of repairing or replacing the file, and no Details, since there is no raw text; the file's size is a neutral badge in the header row.",
    viewer: { subjects: async () => [file("assets/corrupt.png", binary("assets/corrupt.png", { mediaType: "image/png", data: "AAAAAAAAAAAAAAAA" }))] },
  },
];

// A malformed SVG as long as a file the viewer shows without highlighting, so that the one change
// carries a mark of each tone: the part outside the project, the image that cannot be drawn and the
// text it falls back to being too long to highlight.
const OVERSIZED_BROKEN_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">\n${generated(RENDER_BUDGETS.highlightLines + 100, (i) => `<!-- metadata ${i} -->`)}<rect width="10" height="10"`;
async function marksOfEveryToneSubjects(language: Language): Promise<ViewerSubject[]> {
  const t: Translate = (key, ...args) => format(language, key, ...args);
  const path = "assets/oversized.svg";
  return [
    change(t, path, { state: "out_of_scope", repositoryPath: "packages/shared/assets/oversized.svg" }, present(path, text(path, OVERSIZED_BROKEN_SVG))),
  ];
}

async function styleSampleSubjects(): Promise<ViewerSubject[]> {
  return [file("docs/handbook.md", text("docs/handbook.md", STYLE_SAMPLE))];
}
async function documentSubjects(): Promise<ViewerSubject[]> {
  return [file("docs/notes.md", text("docs/notes.md", NORMAL_NOTES)), file("docs/other.md", text("docs/other.md", OTHER))];
}

export const viewerScenarios: Scenario[] = [
  {
    id: "viewer-files-errors", group: GROUP, category: "exceptional",
    title: "Failed reads, disconnected files and renderer fallbacks",
    description: "The failed read takes the empty-state shape in the pane: a danger glyph, the heading and the application's own sentence under it, with no Try again, since a read of a file that is gone would not change. Previous / Next also shows a disconnected read, malformed SVG and a grammar that fails to load, the last two each a danger mark after the path over the text shown as plain text.",
    viewer: { subjects: failedFiles },
  },
  {
    id: "viewer-changes-errors", group: GROUP, category: "exceptional",
    title: "Merge conflicts, malformed patches and disconnected changes",
    description: "Starts on a merge conflict: the file on disk with its markers, and the title's Conflicted chip held hovered, its tooltip saying how the conflict arose (no mark in the header); Previous / Next stays within conflicts and deliberately failing change subjects: a patch that cannot be drawn (a danger mark, the patch shown as text), a compared SVG whose Before side cannot be drawn and is too long to highlight (a danger mark and a note beside the Before heading, none in the header; After is drawn), and an unchanged file whose content is not to be had (a note, with the stand-in under it).",
    viewer: { subjects: failedChanges },
    steps: [(ui) => ui.hoverTitleStatus("conflicted")],
  },
  {
    id: "viewer-document-errors", group: GROUP, category: "exceptional",
    title: "Document rendering failures",
    description: "A document with an invalid diagram, unfinished formulas, a missing image and a missing image that is also a link. Scroll to compare the failures and retained source: each is a danger-coloured circled \"!\" mark at its spot, a button (a tab stop) that opens the sentence in a popover when tapped, activated from the keyboard or rested on with the mouse, and does nothing for a mouse click. A block that failed (the diagram, a formula on its own line) has its mark before the source it keeps; one inside a line of text (the unfinished inline formula, the missing images) has it in the sentence, right after the link where the image is a link. Successful content provides context for each failure.",
    files: { "docs/picture.png": { image: IMAGES.before_png } },
    viewer: { subjects: async () => [file("docs/notes.md", text("docs/notes.md", NOTES)), file("docs/other.md", text("docs/other.md", OTHER))] },
  },
  {
    id: "viewer-inline-failure-reason", group: GROUP, category: "exceptional",
    title: "The reason an image in a sentence failed",
    description: "The danger-coloured circled \"!\" where the image should be, in the middle of a paragraph, is tapped with a finger: its small popover above it, drawn like a tooltip, holds the sentence naming the failure and takes focus.",
    viewer: { subjects: async () => [file("docs/trip.md", text("docs/trip.md", INLINE_FAILURE))] },
    steps: [(ui) => ui.touchPressStatusMark(ui.t("viewer.markdown.imageFailed"))],
  },
  {
    id: "viewer-change-marks", group: GROUP, category: "exceptional",
    title: "One file with a mark of each tone",
    description: "Three marks stand after the path in the header, in this order and with even gaps: the image that could not be drawn (danger, circled \"!\"), the other side being outside the project (warning triangle) and the text being shown without highlighting (note, circled \"i\"). The danger mark is held open in its popover, which words the failure.",
    viewer: { subjects: marksOfEveryToneSubjects },
    steps: [(ui) => ui.hoverStatusMark(ui.t("viewer.image.asText"))],
  },
  ...failureScenarios,
  ...bodiesFailureScenarios,
  {
    id: "viewer-files",
    group: GROUP,
    category: "normal",
    title: "Files",
    description: "Successful code, Markdown, images, inert repository markup, unsupported binary formats, large-file fallbacks (a note mark after the path) and loading views. Dedicated internationalization subjects cover Arabic document direction and mixed-script source. Previous / Next stays within normal subjects. Compare long paths, source/document and wrap controls, and scroll fades using the width and language controls. File manager and Default application are the last two buttons of every file's header row, after the copy, view and wrap buttons.",
    viewer: { subjects: files },
  },
  {
    id: "viewer-files-delayed",
    group: GROUP,
    category: "normal",
    title: "Files, each read taking a moment",
    description: "As Files, but each subject shows its loading state for 400 ms first, as a read from the daemon would.",
    viewer: { subjects: files, loadDelay: 400 },
  },
  {
    id: "viewer-document",
    group: GROUP,
    category: "normal",
    title: "Rendered document",
    description: "Rendered diagrams and formulas, a project image, mixed text directions and links between documents. Outbound web links carry an external-link icon, while local document links remain internal. The named diagram is Request flow; the second uses the translated Diagram label. Follow the other-note link to its Kept heading or the local link to Kept aside.",
    files: { "docs/picture.png": { image: IMAGES.before_png } },
    viewer: { subjects: documentSubjects },
  },
  {
    id: "viewer-document-limits",
    group: GROUP,
    category: "normal",
    title: "Rendered document past its budgets",
    description: "The first formulas are drawn; the one after the budget is left as its source with a note mark (circled \"i\") inside the sentence, a button that opens its sentence in a popover when a resting mouse, a tap or the keyboard asks. The display formula and the over-long diagram keep their source with the same mark before it.",
    viewer: { subjects: async () => [file("docs/limits.md", text("docs/limits.md", LIMITS()))] },
  },
  {
    id: "viewer-document-github",
    group: GROUP,
    category: "normal",
    title: "Rendered document, GitHub style",
    description: "The document in the GitHub style: rules under the first two heading levels, a fully ruled zebra table, a barred quote and tinted inline code. Scroll down to confirm the table, the code block, the formula, the diagram and the footnote take neither the text's tint nor its rules.",
    preferences: { markdownStyle: "github" },
    viewer: { subjects: styleSampleSubjects },
  },
  {
    id: "viewer-document-newsprint",
    group: GROUP,
    category: "normal",
    title: "Rendered document, Newsprint style",
    description: "The document in the Newsprint style: a serif page capped in width and centred, a centred title over a double rule, an italic second-level heading and an italic quote. The Wide window shows the margins either side.",
    preferences: { markdownStyle: "newsprint" },
    viewer: { subjects: styleSampleSubjects },
  },
  {
    id: "viewer-document-latex",
    group: GROUP,
    category: "normal",
    title: "Rendered document, LaTeX style",
    description: "The document in the LaTeX style: a serif page capped in width and centred, justified and hyphenated text, a centred title, a table ruled only at its head and foot and an indented quote. The long identifier in inline code stays unhyphenated, and the code block in the list item, the formula and the diagram are not justified or hyphenated.",
    preferences: { markdownStyle: "latex" },
    viewer: { subjects: styleSampleSubjects },
  },
  {
    id: "git-renamed-viewer",
    group: GROUP,
    category: "normal",
    title: "Renamed file, long source path in the viewer header",
    description: "The comparison subtitle starts at the same inset as the current path's file icon. Both current and renamed-from paths use Markdown icons matching the file tree. The renamed-from path is plain monospace text without a chip background. Narrow the window to check that the long comparison clips and the source path wraps inside the header.",
    viewer: {
      subjects: async (language) => (await changes(language)).filter(({ key }) => key === "Compare:docs/handbook/renamed-notes.md"),
    },
  },
  {
    id: "viewer-changes",
    group: GROUP,
    category: "normal",
    title: "Changes",
    description: "Successful modified, added, deleted, renamed, type-changed, untracked and compared changes. Includes images, binary notices, empty diffs, and the notes and warnings that stand as marks after the path: a patch too large to draw, a missing newline at the end of a file, a side outside the project (warning), and an unchanged file or rename shown in full. Previous / Next stays within normal subjects; compare the soft status badges beside the title (untracked is neutral), long comparison subtitles and layout controls. File manager and Default application end the header row of the working-copy changes (staged, unstaged, untracked and conflicted) but not of the ones whose file is gone from the working copy (src/legacy.ts, src/moved-out.ts) nor of the committed ones, which carry a comparison under the title.",
    viewer: { subjects: changes },
  },
];
