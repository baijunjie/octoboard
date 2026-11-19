import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { redirectPage, retiredSegmentPages, retiredSegmentRedirects } from "./redirects.ts";

const refresh = (page: string) => /<meta http-equiv="refresh" content="0; url=([^"]*)">/.exec(page)?.[1];

test("each built page of a Chinese language gets a redirect page at its retired address, and no other page does", () => {
  const pages = retiredSegmentPages(
    [
      "", "install/linux/", "404/", "ja/", "ja/install/linux/", "zh-CNx/", "zh/", "zh-Hant/remote-connections/",
      "zh-CN/", "zh-CN/install/linux/", "zh-TW/", "zh-TW/remote-connections/",
    ],
    "/site/docs/",
    "/site/favicon.png",
  );
  const expected: Record<string, string> = {
    "zh/index.html": "/site/docs/zh-CN/",
    "zh/install/linux/index.html": "/site/docs/zh-CN/install/linux/",
    "zh-Hant/index.html": "/site/docs/zh-TW/",
    "zh-Hant/remote-connections/index.html": "/site/docs/zh-TW/remote-connections/",
  };
  assert.deepEqual(Object.keys(pages).sort(), Object.keys(expected).sort());
  for (const [file, target] of Object.entries(expected)) {
    assert.equal(refresh(pages[file]!), target, file);
    assert.ok(pages[file]!.includes(`location.replace(${JSON.stringify(target)} + location.search + location.hash)`), file);
    assert.ok(pages[file]!.includes(`<a href="${target}">`), file);
    assert.match(pages[file]!, /<meta name="robots" content="noindex, nofollow">/);
    assert.equal(pages[file]!.includes('rel="canonical"'), false);
    assert.match(pages[file]!, /<link rel="icon" type="image\/png" href="\/site\/favicon\.png">/);
  }
});

test("the redirects follow the docs' base path, and a built path may carry a leading slash", () => {
  assert.equal(refresh(retiredSegmentPages(["zh-CN/install/linux/"], "/docs/", "/favicon.png")["zh/install/linux/index.html"]!), "/docs/zh-CN/install/linux/");
  assert.equal(refresh(retiredSegmentPages(["/zh-TW/"], "/a/b/docs/", "/a/b/favicon.png")["zh-Hant/index.html"]!), "/a/b/docs/zh-TW/");
  assert.deepEqual(retiredSegmentPages([], "/docs/", "/favicon.png"), {});
});

test("the redirect page escapes the target, and keeps a script from ending early", () => {
  const page = redirectPage({ url: '/docs/a"b&c</script>', title: "T <&>", notice: "N", favicon: "/f.png" });
  assert.equal(refresh(page), "/docs/a&quot;b&amp;c&lt;/script&gt;");
  assert.ok(page.includes("<title>T &lt;&amp;&gt;</title>"));
  assert.ok(page.includes(String.raw`location.replace("/docs/a\"b&c\u003c/script>")`));
  assert.equal(page.match(/<\/script>/g)?.length, 1);
});

test("the redirect page sends the query and fragment along only when asked to", () => {
  const input = { url: "/docs/a/", title: "T", notice: "N", favicon: "/f.png" };
  assert.ok(redirectPage(input).includes('location.replace("/docs/a/");'));
  assert.ok(redirectPage({ ...input, carry: true }).includes('location.replace("/docs/a/" + location.search + location.hash);'));
  assert.equal(refresh(redirectPage({ ...input, carry: true })), "/docs/a/");
});

test("once the site is built, the integration writes each redirect page into the output", async () => {
  const output = mkdtempSync(path.join(os.tmpdir(), "octoboard-docs-redirects-"));
  try {
    const integration = retiredSegmentRedirects("/site/docs/", "/site/favicon.png");
    const done = integration.hooks["astro:build:done"] as unknown as (options: unknown) => Promise<void>;
    await done({ dir: pathToFileURL(`${output}/`), pages: [{ pathname: "zh-CN/" }, { pathname: "zh-TW/install/linux/" }, { pathname: "ja/" }, { pathname: "" }] });
    const files = readdirSync(output, { recursive: true, withFileTypes: true }).filter((entry) => entry.isFile()).map((entry) => path.relative(output, path.join(entry.parentPath, entry.name))).sort();
    assert.deepEqual(files, ["zh-Hant/install/linux/index.html", "zh/index.html"]);
    assert.equal(refresh(readFileSync(path.join(output, "zh/index.html"), "utf8")), "/site/docs/zh-CN/");
    assert.equal(refresh(readFileSync(path.join(output, "zh-Hant/install/linux/index.html"), "utf8")), "/site/docs/zh-TW/install/linux/");
  } finally {
    rmSync(output, { recursive: true, force: true });
  }
});
