#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { lstatSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

function command(cmd, args, { cwd, input } = {}) {
  const result = spawnSync(cmd, args, { cwd, input, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (result.error) throw result.error;
  return result;
}

function checked(cmd, args, options) {
  const result = command(cmd, args, options);
  if (result.status !== 0) throw new Error(result.stderr || `${cmd} failed (${result.status})`);
  return result.stdout;
}

// Both spellings of the index path (relative or absolute, through a symlink or not) must compare equal.
function realpath(cwd, file) {
  const resolved = path.resolve(cwd, file);
  try { return realpathSync(resolved); } catch { return resolved; }
}

export function createFormatter({ cwd, run = command, importEslint = () => import("eslint") } = {}) {
  let eslint;
  return async (file, content) => {
    if (file.endsWith(".rs")) {
      // The edition and style come from the root `rustfmt.toml`, which rustfmt finds from `cwd` just as
      // `cargo fmt` does. `skip_children` keeps rustfmt from following `mod x;` declarations: the content
      // arrives on stdin, so those modules would be read from the working tree, not the staged blob.
      const result = run("rustfmt", ["--emit", "stdout", "--config", "skip_children=true"], { cwd, input: content });
      if (result.status !== 0) throw new Error(result.stderr || `rustfmt failed: ${file}`);
      return result.stdout;
    }
    if (!eslint) {
      let ESLint;
      try {
        ({ ESLint } = await importEslint());
      } catch (error) {
        if (error.code !== "ERR_MODULE_NOT_FOUND") throw error;
        throw new Error("Cannot format staged files: eslint is not installed. Run `pnpm install` and commit again.", { cause: error });
      }
      eslint = new ESLint({ cwd, fix: true, fixTypes: ["layout"] });
    }
    const [result] = await eslint.lintText(content, { filePath: path.resolve(cwd, file), warnIgnored: false });
    if (result?.fatalErrorCount) throw new Error(`Cannot format ${file}: ${result.messages.map((message) => message.message).join("; ")}`);
    return result?.output ?? content;
  };
}

function merge(cwd, file, staged, working, formatted) {
  const temp = mkdtempSync(path.join(os.tmpdir(), "format-staged-"));
  try {
    const inputs = [working, staged, formatted].map((content, index) => {
      const target = path.join(temp, `${index}`);
      writeFileSync(target, content);
      return target;
    });
    const result = command("git", ["merge-file", "-p", ...inputs], { cwd });
    if (result.status !== 0) {
      throw new Error(`Formatting conflicts with unstaged changes in ${file}. Stage or separate those changes and retry; the index and working files were left unchanged.`);
    }
    return result.stdout;
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// Plan every merge before updating the index: a formatting conflict must leave even previously
// formatted files untouched, and the unstaged content must never become part of the commit.
export async function formatStaged({ cwd = process.cwd(), formatter, env = process.env } = {}) {
  const git = (...args) => checked("git", args, { cwd });
  // `git commit <paths>` runs the hook against a temporary index; formatting that one would leave the
  // real index with the unformatted blobs (staged as `MM`), so refuse instead. `git commit -a` / `-i`
  // hand the hook the real index's lock file, which becomes the index, so both names are accepted. The
  // real index is named from the git directory: `--git-path index` itself follows `GIT_INDEX_FILE`.
  if (env.GIT_INDEX_FILE) {
    const gitDir = realpath(cwd, git("rev-parse", "--absolute-git-dir").trim());
    const index = path.resolve(cwd, env.GIT_INDEX_FILE);
    const real = path.join(realpath(cwd, path.dirname(index)), path.basename(index));
    if (![path.join(gitDir, "index"), path.join(gitDir, "index.lock")].includes(real)) {
      throw new Error("Cannot format staged files in a partial commit (git commit <paths>). Stage the files with `git add` and commit without paths.");
    }
  }
  cwd = git("rev-parse", "--show-toplevel").trim();
  formatter ??= createFormatter({ cwd });
  const files = git("diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z").split("\0").filter((file) => /\.(m?[jt]sx?|cjs|cts|vue|rs)$/.test(file));
  const entries = new Map(git("ls-files", "--stage", "-z").split("\0").filter(Boolean).map((entry) => {
    const tab = entry.indexOf("\t");
    return [entry.slice(tab + 1), entry.slice(0, tab).split(" ")];
  }));
  const updates = [];
  for (const file of files) {
    const entry = entries.get(file);
    if (!entry || !["100644", "100755"].includes(entry[0]) || entry[2] !== "0") continue;
    const target = path.join(cwd, file);
    let stat;
    try { stat = lstatSync(target); } catch (error) {
      if (error.code === "ENOENT") continue;
      throw error;
    }
    if (!stat.isFile()) continue;
    const staged = git("cat-file", "blob", entry[1]);
    const working = readFileSync(target, "utf8");
    const formatted = await formatter(file, staged);
    if (formatted === staged) continue;
    const merged = working === staged ? formatted : merge(cwd, file, staged, working, formatted);
    updates.push({ file, target, mode: entry[0], staged, working, formatted, merged });
  }
  if (updates.length === 0) return [];
  // Editors can save while a formatter is running; do not overwrite a newer save.
  for (const update of updates) {
    if (!lstatSync(update.target).isFile() || readFileSync(update.target, "utf8") !== update.working || git("show", `:${update.file}`) !== update.staged) {
      throw new Error(`${update.file} changed during formatting; retry the commit.`);
    }
  }
  const indexInput = updates.map((update) => {
    const blob = checked("git", ["hash-object", "-w", "--stdin"], { cwd, input: update.formatted }).trim();
    return `${update.mode} ${blob}\t${update.file}\0`;
  }).join("");
  checked("git", ["update-index", "-z", "--index-info"], { cwd, input: indexInput });
  for (const update of updates) writeFileSync(update.target, update.merged);
  return updates.map((update) => update.file);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  formatStaged().then((files) => {
    if (files.length) console.log(`Formatted ${files.length} staged file(s).`);
  }).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
