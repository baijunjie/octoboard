import { expect, it } from "vitest";

import type { FileContent } from "../protocol";
import { bodyFromFileContent, changePresentation, changeStatus, type ViewerChange, type ViewerChangeSide } from "./content";

const textFile = (text: string): FileContent => ({ size: text.length, kind: "text", media_type: null, text, data: null });
const binaryFile = (media_type: string | null): FileContent => ({ size: 3, kind: "binary", media_type, text: null, data: "AAEC" });

it.each([
  ["a text body is code", "src/a.ts", textFile("x"), { kind: "text", text: "x", size: 1 }],
  ["a recognised image is an image", "a.png", binaryFile("image/png"), { kind: "image", mediaType: "image/png", url: "data:image/png;base64,AAEC", size: 3 }],
  ["any other binary body is unsupported", "a.wasm", binaryFile(null), { kind: "binary", size: 3 }],
])("%s", (_, path, file, body) => {
  expect(bodyFromFileContent(path, file)).toEqual(body);
});

it("shows an SVG, which arrives as text, as an image that keeps its text", () => {
  const body = bodyFromFileContent("logo.SVG", textFile("<svg/>"));
  expect(body).toMatchObject({ kind: "image", mediaType: "image/svg+xml", text: "<svg/>" });
  expect(body.kind === "image" && body.url).toBe("data:image/svg+xml;charset=utf-8,%3Csvg%2F%3E");
});

const TYPE_CHANGE = "diff --git a/t b/t\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-x\ndiff --git a/t b/t\nnew file mode 120000\n@@ -0,0 +1 @@\n+y\n";

const text = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", body: { kind: "text", text: "x", size: 1 } });
const image = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", body: { kind: "image", mediaType: "image/png", url: "data:", size: 1 } });
const binary = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "file", body: { kind: "binary", size: 1 } });
const link = (path: string): ViewerChangeSide => ({ state: "present", path, kind: "symlink" });
const absent: ViewerChangeSide = { state: "absent" };
const outside: ViewerChangeSide = { state: "out_of_scope", repositoryPath: "lib/a.ts" };

it.each<[string, ViewerChange, string, string]>([
  ["an added file", { old: absent, new: text("a.ts"), patch: "@@" }, "added", "text"],
  ["a deleted file", { old: text("a.ts"), new: absent, patch: "@@" }, "deleted", "text"],
  ["a text change without a patch", { old: text("a.ts"), new: text("a.ts") }, "modified", "unreadable"],
  ["a renamed file with a patch", { old: link("a.ts"), new: link("b.ts"), patch: "@@" }, "renamed", "text"],
  ["a file that became a link", { old: text("a.ts"), new: link("a.ts"), patch: TYPE_CHANGE }, "typeChanged", "text"],
  ["an untracked file, alone", { old: absent, new: text("a.ts") }, "added", "single"],
  ["an untracked link, alone", { old: absent, new: link("a.ts") }, "added", "notFile"],
  ["a change committed since it was listed", { old: text("a.ts"), new: text("a.ts"), patch: "" }, "modified", "identical"],
  ["a change that could not be read", { old: link("a.ts"), new: link("a.ts"), patch: "@@", unavailable: "Too large." }, "modified", "unreadable"],
  ["a link without a patch", { old: link("a.ts"), new: link("a.ts") }, "modified", "unreadable"],
  ["a changed image", { old: image("a.png"), new: image("a.png") }, "modified", "image"],
  ["an added image", { old: absent, new: image("a.png") }, "added", "image"],
  ["a changed binary", { old: binary("a.bin"), new: binary("a.bin") }, "modified", "binary"],
  ["an image that became text", { old: image("a.svg"), new: text("a.svg") }, "modified", "binary"],
])("presents %s", (_, change, status, presentation) => {
  expect(changeStatus(change)).toBe(status);
  expect(changePresentation(change).kind).toBe(presentation);
});

// A side outside the project is never shown as empty or patched against: only the permitted side
// is shown, with the restriction named.
it.each<[ViewerChange, "old" | "new"]>([
  [{ old: outside, new: text("a.ts") }, "old"],
  [{ old: text("a.ts"), new: outside, patch: "@@" }, "new"],
])("restricts a change with a side outside the project", (change, hidden) => {
  expect(changePresentation(change)).toMatchObject({ kind: "restricted", hidden, repositoryPath: "lib/a.ts" });
  expect(changeStatus(change)).toBe("renamed");
});
