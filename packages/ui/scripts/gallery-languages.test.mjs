import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, expect, test } from "vitest";

import { chooseLanguages, languagesFromChanges } from "./gallery-languages.mjs";

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

const scratch = mkdtempSync(path.join(os.tmpdir(), "gallery-languages-"));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));

// A repository with one commit on `main`, then `feat` branched from it with its target branch recorded when
// `targetBranch` is set. `edit` changes the working tree of `feat`.
function checkout({ targetBranch = true, edit = () => {} }) {
  const root = mkdtempSync(path.join(scratch, "repo-"));
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const write = (file, content) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), content);
  };
  git("init", "-q", "-b", "main");
  write("packages/ui/src/components/Badge.tsx", "a");
  write("packages/ui/src/i18n/joinPhrases.ts", "a");
  git("add", ".");
  git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "x");
  git("switch", "-q", "-c", "feat");
  if (targetBranch) git("config", "branch.feat.targetBranch", "main");
  edit(write);
  return root;
}

test("from a real checkout, a change to components alone is swept in English only", () => {
  const root = checkout({ edit: (write) => write("packages/ui/src/components/Badge.tsx", "b") });
  expect(languagesFromChanges(root, ALL).languages).toEqual(["en"]);
});

test("from a real checkout, a change under src/i18n sweeps every language", () => {
  const root = checkout({ edit: (write) => write("packages/ui/src/i18n/joinPhrases.ts", "b") });
  const chosen = languagesFromChanges(root, ALL);
  expect(chosen.languages).toEqual(ALL);
  expect(chosen.reason).toContain("packages/ui/src/i18n/joinPhrases.ts");
});

test("from a real checkout, an untracked file counts as a change", () => {
  const root = checkout({ edit: (write) => write("packages/ui/src/components/New.tsx", "b") });
  expect(languagesFromChanges(root, ALL).languages).toEqual(["en"]);
});

test("from a real checkout, nothing differing from the merge base sweeps every language", () => {
  const chosen = languagesFromChanges(checkout({}), ALL);
  expect(chosen.languages).toEqual(ALL);
  expect(chosen.reason).toContain("nothing differs");
});

test("from a real checkout without a recorded target branch, every language is swept", () => {
  const root = checkout({ targetBranch: false, edit: (write) => write("packages/ui/src/components/Badge.tsx", "b") });
  const chosen = languagesFromChanges(root, ALL);
  expect(chosen.languages).toEqual(ALL);
  expect(chosen.reason).toContain("no recorded target branch");
});

test("outside a git working copy every language is swept", () => {
  expect(languagesFromChanges(null, ALL).languages).toEqual(ALL);
});
