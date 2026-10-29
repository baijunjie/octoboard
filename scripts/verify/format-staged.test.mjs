import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createFormatter, formatStaged } from "../format-staged.mjs";

async function repository(run) {
  const cwd = mkdtempSync(path.join(os.tmpdir(), "staged-"));
  const git = (...args) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  const write = (file, content) => writeFileSync(path.join(cwd, file), content);
  try {
    git("init", "-q");
    await run({ cwd, git, write });
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
}

test("formats the staged blob and preserves separate unstaged edits and unusual filenames", async () => {
  await repository(async ({ cwd, git, write }) => {
    const file = "a space\nand newline.ts";
    const staged = "const first=1;\n\n\nconst last = 1;\n";
    write(file, staged);
    git("add", "--", file);
    write(file, staged.replace("last = 1", "last = 2"));
    assert.deepEqual(await formatStaged({ cwd, formatter: async (_file, content) => content.replace("first=1", "first = 1") }), [file]);
    assert.equal(git("show", `:${file}`), staged.replace("first=1", "first = 1"));
    assert.equal(readFileSync(path.join(cwd, file), "utf8"), staged.replace("first=1", "first = 1").replace("last = 1", "last = 2"));
  });
});

test("a merge conflict leaves every index entry and working file unchanged", async () => {
  await repository(async ({ cwd, git, write }) => {
    write("a.ts", "const first=1;\n");
    write("b.ts", "const second=1;\n");
    git("add", ".");
    write("b.ts", "const second=2;\n");
    const index = git("ls-files", "--stage");
    await assert.rejects(formatStaged({ cwd, formatter: async (_file, content) => content.replace("=1", " = 1") }), /conflicts with unstaged/);
    assert.equal(git("ls-files", "--stage"), index);
    assert.equal(readFileSync(path.join(cwd, "a.ts"), "utf8"), "const first=1;\n");
    assert.equal(readFileSync(path.join(cwd, "b.ts"), "utf8"), "const second=2;\n");
  });
});

test("only staged supported regular files are passed to the formatter", async () => {
  await repository(async ({ cwd, git, write }) => {
    write("staged.rs", "fn main(){}\n");
    write("unstaged.ts", "const x=1;\n");
    write("notes.txt", "text");
    symlinkSync("staged.rs", path.join(cwd, "link.rs"));
    git("add", "staged.rs", "notes.txt", "link.rs");
    const seen = [];
    await formatStaged({ cwd, formatter: async (file, content) => { seen.push(file); return content; } });
    assert.deepEqual(seen, ["staged.rs"]);
  });
});

test("Rust formats stdin without traversing child modules", async () => {
  const formatter = createFormatter({ cwd: "/tmp", run: (cmd, args, options) => {
    assert.equal(cmd, "rustfmt");
    assert.deepEqual(args, ["--emit", "stdout", "--config", "skip_children=true"]);
    assert.equal(options.input, "mod child;\n");
    return { status: 0, stdout: "mod child;\n", stderr: "" };
  } });
  assert.equal(await formatter("lib.rs", "mod child;\n"), "mod child;\n");
});

test("renames format the new path and staged deletions stay deleted", async () => {
  await repository(async ({ cwd, git, write }) => {
    write("old.ts", "const value=1;\n");
    write("deleted.ts", "const deleted=1;\n");
    git("add", ".");
    git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "fixture");
    git("mv", "old.ts", "new name.ts");
    git("rm", "deleted.ts");
    const seen = [];
    await formatStaged({ cwd, formatter: async (file, content) => {
      seen.push(file);
      return content.replace("value=1", "value = 1");
    } });
    assert.deepEqual(seen, ["new name.ts"]);
    assert.equal(git("show", ":new name.ts"), "const value = 1;\n");
    assert.equal(git("ls-files", "old.ts", "deleted.ts"), "");
  });
});

test("a formatter failure leaves previously prepared updates unapplied", async () => {
  await repository(async ({ cwd, git, write }) => {
    write("a.ts", "const a=1;\n");
    write("b.rs", "invalid Rust\n");
    git("add", ".");
    const index = git("ls-files", "--stage");
    await assert.rejects(formatStaged({ cwd, formatter: async (file, content) => {
      if (file.endsWith(".rs")) throw new Error("parse failed");
      return content.replace("a=1", "a = 1");
    } }), /parse failed/);
    assert.equal(git("ls-files", "--stage"), index);
    assert.equal(readFileSync(path.join(cwd, "a.ts"), "utf8"), "const a=1;\n");
  });
});

test("a partial commit's temporary index is refused, while the repository's own index is accepted", async () => {
  await repository(async ({ cwd, git, write }) => {
    write("a.ts", "const a=1;\n");
    git("add", ".");
    const formatter = async (_file, content) => content;
    await assert.rejects(formatStaged({ cwd, formatter, env: { GIT_INDEX_FILE: path.join(cwd, ".git", "next-index-1.lock") } }), /without paths/);
    await assert.doesNotReject(formatStaged({ cwd, formatter, env: { GIT_INDEX_FILE: ".git/index" } }));
    await assert.doesNotReject(formatStaged({ cwd, formatter, env: {} }));
  });
});

// Real commits, so git's own GIT_INDEX_FILE reaches the git processes the script spawns.
test("as a pre-commit hook, a partial commit is refused and `commit -a` commits the formatted file", async () => {
  await repository(async ({ cwd, git, write }) => {
    const script = fileURLToPath(new URL("../format-staged.mjs", import.meta.url));
    writeFileSync(path.join(cwd, ".git", "hooks", "pre-commit"), `#!/bin/sh\nexec node "${script}"\n`, { mode: 0o755 });
    git("config", "user.email", "test@example.com");
    git("config", "user.name", "test");
    write("a.rs", "fn main() {}\n");
    git("add", ".");
    git("commit", "-qm", "init");
    write("a.rs", "fn main(){}\n");
    assert.throws(() => git("commit", "-qm", "partial", "a.rs"), /without paths/);
    assert.equal(git("rev-list", "--count", "HEAD").trim(), "1");
    write("a.rs", "fn main(){ let x=1; }\n");
    git("commit", "-qam", "all");
    assert.equal(git("show", "HEAD:a.rs"), "fn main() {\n    let x = 1;\n}\n");
    assert.equal(git("status", "--porcelain"), "");
  });
});

test("a missing eslint is reported with the command that fixes it", async () => {
  const formatter = createFormatter({ cwd: "/tmp", importEslint: async () => { throw Object.assign(new Error("nope"), { code: "ERR_MODULE_NOT_FOUND" }); } });
  await assert.rejects(formatter("a.ts", "x"), /pnpm install/);
});
