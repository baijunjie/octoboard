// @vitest-environment node
import { fileURLToPath } from "node:url";

import { build, type Rolldown } from "vite";
import { expect, it } from "vitest";

// The task-list rules in `markdown.css` follow each item's reading direction through `:dir()`. A
// build's CSS minifier rewrites `:dir()` into `:lang()` lists for older targets unless the config
// keeps it, and only a build minifies, so the dev server and the gallery never show the rewrite.

it("keeps :dir() in the built task-list rules", async () => {
  const output = (await build({
    configFile: fileURLToPath(new URL("../../vite.config.ts", import.meta.url)),
    logLevel: "silent",
    build: { write: false, rolldownOptions: { input: fileURLToPath(new URL("./markdown.css", import.meta.url)) } },
  })) as Rolldown.RolldownOutput;
  const css = output.output
    .filter((chunk): chunk is Rolldown.OutputAsset => chunk.type === "asset" && chunk.fileName.endsWith(".css"))
    .map((chunk) => String(chunk.source))
    .join("");

  expect(css).toContain(".task-list-item:dir(ltr)>input");
  expect(css).toContain(".task-list-item:dir(rtl)>input");
  expect(css).not.toContain(":lang(");
}, 60_000);
