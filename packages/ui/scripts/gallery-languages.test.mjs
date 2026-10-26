import { expect, test } from "vitest";

import { chooseLanguages } from "./gallery-languages.mjs";

const ALL = ["en", "zh-Hans"];
const base = { commit: "abc123" };
const choose = (files) => chooseLanguages(ALL, base, files);

test("every language is swept where there are no changes to judge by", () => {
  const none = { reason: "no recorded target branch" };
  const targetBranch = chooseLanguages(ALL, none, null);
  expect(targetBranch.languages).toEqual(ALL);
  expect(targetBranch.reason).toBe("no recorded target branch, so there are no changes to judge by");
  expect(chooseLanguages(ALL, { reason: "base x is not readable" }, null).languages).toEqual(ALL);
  expect(choose([]).languages).toEqual(ALL);
  expect(choose(null).languages).toEqual(ALL);
});

test("a change that can make the languages differ sweeps them all, and says which path", () => {
  for (const file of [
    "packages/ui/src/i18n/joinPhrases.ts",
    "packages/ui/src/i18n/messages/zh-Hans.ts",
    "packages/ui/scripts/gallery-sweep.mjs",
    "packages/ui/scripts/test-fixtures/gallery-sweep-loader.mjs",
    "packages/ui/src/gallery/fixtures/agents.ts",
    "scripts/verify/stamp.mjs",
    "pnpm-lock.yaml",
    "package.json",
    "packages/ui/package.json",
    "packages/ui/tsconfig.json",
    "packages/ui/vite.config.ts",
    "packages/ui/gallery-frame.html",
    "packages/ui/typography.mjs",
    "config/daemon.json",
    "patches/heroui.patch",
    "packages/ui/.env.local",
  ]) {
    const chosen = choose(["packages/ui/src/components/Badge.tsx", file]);
    expect(chosen.languages, file).toEqual(ALL);
    expect(chosen.reason, file).toContain(file);
  }
});

test("a change to components alone is swept in English only", () => {
  const chosen = choose(["packages/ui/src/components/Badge.tsx", "packages/ui/src/components/Badge.css", "docs/project-map.md", "apps/daemon/src/main.rs"]);
  expect(chosen.languages).toEqual(["en"]);
  expect(chosen.reason).toContain("--lang all");
});

test("a path that only resembles a sensitive one is not", () => {
  expect(choose(["packages/ui/src/components/i18n/Row.tsx", "packages/ui/src/gallery-like/x.ts", "packages/ui/src/pages/Home.html"]).languages).toEqual(["en"]);
});
