import { expect, it } from "vitest";

import { resolveProjectAddress } from "./projectAddress";

it("resolves a relative path against the document and keeps a heading", () => {
  expect(resolveProjectAddress("docs/guide.md", "./images/a.png")).toEqual({ kind: "file", path: "docs/images/a.png" });
  expect(resolveProjectAddress("docs/guide.md", "other.md#Heading")).toEqual({ kind: "file", path: "docs/other.md", fragment: "Heading" });
  expect(resolveProjectAddress("docs/guide.md", "../README.md")).toEqual({ kind: "file", path: "README.md" });
  expect(resolveProjectAddress("docs/guide.md", "file.md?raw=1#x")).toEqual({ kind: "file", path: "docs/file.md", fragment: "x" });
});

it("treats a fragment as an anchor in this document", () => {
  expect(resolveProjectAddress("docs/guide.md", "#title")).toEqual({ kind: "anchor", id: "title" });
});

it("leaves web and mail addresses to the platform", () => {
  expect(resolveProjectAddress("docs/guide.md", "https://example.com/a")).toEqual({ kind: "external", href: "https://example.com/a" });
  expect(resolveProjectAddress("docs/guide.md", "mailto:a@example.com")).toEqual({ kind: "external", href: "mailto:a@example.com" });
});

it("refuses a path that leaves the project, a scheme, or an absolute address", () => {
  for (const href of [
    "../../etc/passwd",
    "%2e%2e/%2e%2e/etc/passwd",
    "/etc/passwd",
    "//example.com/a.png",
    "javascript:alert(1)",
    "data:text/html,hi",
    "file:///etc/passwd",
    "a\\b.md",
    "a\0b.md",
    "",
    undefined,
  ]) {
    expect(resolveProjectAddress("docs/guide.md", href)).toEqual({ kind: "refused" });
  }
  expect(resolveProjectAddress("README.md", "../outside.md")).toEqual({ kind: "refused" });
});
