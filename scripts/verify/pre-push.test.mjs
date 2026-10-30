import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync, chmodSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { parseUpdates, verifyPush } from "../pre-push.mjs";

const zero = "0".repeat(40);
function fixture(run) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "pre-push-"));
  const root = path.join(dir, "work");
  const remote = path.join(dir, "remote.git");
  mkdirSync(root);
  const git = (...args) => {
    try {
      return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
    } catch {
      return null;
    }
  };
  const commit = (content) => {
    writeFileSync(path.join(root, "file"), content);
    assert.notEqual(git("add", "."), null);
    assert.notEqual(git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", content), null);
    return git("rev-parse", "HEAD");
  };
  try {
    git("init", "-qb", "main");
    git("init", "--bare", "-q", "-b", "main", remote);
    git("remote", "add", "destination", remote);
    const base = commit("base");
    // Populate the bare remote directly; these fixtures do not exercise the user's installed hooks.
    git("--git-dir", remote, "fetch", "-q", root, "main:main");
    const head = commit("head");
    const calls = [];
    const verify = (args) => calls.push(args);
    run({ root, remote, git, commit, base, head, calls, verify });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
const update = (head, base = zero, local = "refs/heads/main", remote = local) => `${local} ${head} ${remote} ${base}\n`;

test("parses SHA-1 and SHA-256 update lines and rejects malformed input", () => {
  assert.equal(parseUpdates(update("a".repeat(64))).length, 1);
  assert.deepEqual(parseUpdates("\n"), []);
  assert.throws(() => parseUpdates("refs/heads/a invalid refs/heads/a invalid"), /Invalid/);
});

test("empty pushes and deletions never fetch or run checks", () => {
  for (const input of ["", update(zero, "b".repeat(40))]) {
    verifyPush(input, null, null, {
      git: () => assert.fail("no Git command expected"),
      verify: () => assert.fail("no checks expected"),
    });
  }
});

test("checks the exact remote tip and pushed commit, deduplicates refs, and skips deletion", () => {
  fixture(({ git, base, head, calls, verify }) => {
    verifyPush(update(head, base) + update(head, base, "refs/heads/other") + update(zero, base), "destination", null, { git, verify });
    assert.deepEqual(calls, [["--base", base, "--head", head]]);
  });
});

test("force pushes preserve the advertised base, and tag updates are not verified", () => {
  fixture(({ git, base, head, calls, verify }) => {
    git("-c", "user.name=Test", "-c", "user.email=test@example.com", "tag", "-am", "release", "v1");
    const tag = git("rev-parse", "v1");
    verifyPush(update(tag, base, "refs/tags/v1") + update(base, head), "destination", null, { git, verify });
    assert.deepEqual(calls, [["--base", head, "--head", base]]);
  });
});

test("new branches use the configured target's live destination tip, otherwise the remote HEAD", () => {
  fixture(({ git, remote, base, head, calls, verify }) => {
    git("config", "branch.feature.targetBranch", "main");
    git("update-ref", "refs/remotes/destination/main", head);
    verifyPush(update(head, zero, "refs/heads/feature"), "destination", remote, { git, verify });
    verifyPush(update(head), "destination", remote, { git, verify });
    assert.deepEqual(calls, Array(2).fill(["--base", base, "--head", head]));
  });
});

test("a new branch is measured from its merge base with the live target tip", () => {
  fixture(({ git, root, remote, base, head, calls, verify }) => {
    // The target moved on after the branch forked: its tip is not an ancestor of the branch.
    git("checkout", "-q", "-b", "target-side", base);
    writeFileSync(path.join(root, "other"), "x");
    git("add", ".");
    git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "target moves");
    const tip = git("rev-parse", "HEAD");
    git("push", "-q", "destination", "target-side:main", "--force");
    git("checkout", "-q", "main");
    verifyPush(update(head, zero, "refs/heads/feature"), "destination", remote, { git, verify });
    assert.deepEqual(calls, [["--base", base, "--head", head]]);
    assert.notEqual(tip, base);
  });
});

test("unreadable new-branch destination falls back to the full set", () => {
  fixture(({ git, root, head, calls, verify }) => {
    verifyPush(update(head), "destination", path.join(root, "absent"), { git, verify });
    assert.deepEqual(calls, [["--full", "--head", head]]);
  });
});

test("fetches a missing advertised remote object by exact ID, and refuses fetch failure", () => {
  const head = "a".repeat(40), base = "b".repeat(40);
  let fetched = false;
  const commands = [];
  const git = (...args) => {
    commands.push(args);
    if (args[0] === "fetch") { fetched = true; return ""; }
    if (args[0] === "rev-parse") return args[2].startsWith(head) ? head : fetched ? base : null;
    return null;
  };
  const calls = [];
  verifyPush(update(head, base), "destination", "actual-push-url", { git, verify: (args) => calls.push(args) });
  assert.deepEqual(commands.find((args) => args[0] === "fetch"), ["fetch", "--no-tags", "--no-write-fetch-head", "--", "actual-push-url", base]);
  assert.deepEqual(calls, [["--base", base, "--head", head]]);
  assert.throws(() => verifyPush(update(head, base), "destination", null, {
    git: (...args) => args[0] === "rev-parse" && args[2].startsWith(head) ? head : null,
    verify: () => assert.fail("must not verify an unknown base"),
  }), /Cannot fetch/);
});

test("passes each distinct update to verify in order and propagates refusal", () => {
  fixture(({ git, base, head, calls }) => {
    assert.throws(() => verifyPush(update(head, base) + update(base, head), "destination", null, {
      git,
      verify: (args) => { calls.push(args); throw new Error("checkout is dirty or wrong HEAD"); },
    }), /dirty or wrong HEAD/);
    assert.equal(calls.length, 1);
  });
});

test("installed hook dispatch resolves the current worktree and passes the pushed lines to the verification", () => {
  fixture(({ root, git, base, head }) => {
    const bin = path.join(root, "bin");
    mkdirSync(bin);
    const marker = path.join(root, "dispatch");
    mkdirSync(path.join(root, "scripts"));
    for (const script of ["pre-push.mjs", "format-staged.mjs"]) writeFileSync(path.join(root, "scripts", script), "");
    const node = path.join(bin, "node");
    writeFileSync(node, '#!/bin/sh\nprintf "%s\\n" "$@" > "$MARKER"\ncat >> "$MARKER"\nexit "${REFUSE:-0}"\n');
    chmodSync(node, 0o755);
    const hook = path.join(root, ".git/hooks/pre-push");
    copyFileSync(new URL("../../.githooks/hook.sh", import.meta.url), hook);
    const input = update(head, base);
    const invoke = (args, extra = {}, cwd = root, inputText = input) => spawnSync("sh", [hook, ...args], {
      cwd, input: inputText, encoding: "utf8", env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, MARKER: marker, ...extra },
    });
    assert.equal(invoke(["destination", "push-url"]).status, 0);
    assert.equal(readFileSync(marker, "utf8"), `${realpathSync(root)}/scripts/pre-push.mjs\ndestination\npush-url\n${input}`);
    assert.equal(invoke(["destination", "push-url"], { REFUSE: "1" }).status, 1);
    const linked = path.join(root, "linked");
    assert.notEqual(git("worktree", "add", "--detach", linked, head), null);
    mkdirSync(path.join(linked, "scripts"));
    writeFileSync(path.join(linked, "scripts/pre-push.mjs"), "");
    assert.equal(invoke(["destination", "push-url"], {}, linked, update(head, base, "refs/tags/v1")).status, 0);
    assert.match(readFileSync(marker, "utf8"), /linked\/scripts\/pre-push.mjs/);
    assert.equal(invoke(["pre-commit"]).status, 0);
    assert.match(readFileSync(marker, "utf8"), /scripts\/format-staged.mjs/);
    // hook.sh dispatches only pre-commit and pre-push; any other hook name passes without running anything,
    // even outside a work tree.
    rmSync(marker);
    const reference = path.join(root, ".git/hooks/reference-transaction");
    copyFileSync(hook, reference);
    const skipped = spawnSync("sh", [reference, "prepared"], {
      cwd: root, input: "", encoding: "utf8", env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, MARKER: marker },
    });
    assert.equal(skipped.status, 0);
    assert.equal(existsSync(marker), false);
    const outside = spawnSync("sh", [reference, "prepared"], {
      cwd: path.join(root, ".git"), input: "", encoding: "utf8", env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, MARKER: marker },
    });
    assert.equal(outside.status, 0, outside.stderr);
    assert.equal(existsSync(marker), false);
  });
});

test("installer installs pre-commit and pre-push, refuses external hooks, configured hook paths and old flags before writing, and migrates", () => {
  fixture(({ root, git }) => {
    mkdirSync(path.join(root, ".githooks"));
    for (const name of ["install.sh", "hook.sh"]) {
      copyFileSync(new URL(`../../.githooks/${name}`, import.meta.url), path.join(root, ".githooks", name));
    }
    const hooks = path.join(root, ".git/hooks");
    const invoke = (args = [], extra = {}) => spawnSync("sh", [".githooks/install.sh", ...args], { cwd: root, encoding: "utf8", env: { ...process.env, ...extra } });
    const external = path.join(hooks, "pre-commit");
    writeFileSync(external, "#!/bin/sh\n# external hook\n");
    assert.equal(invoke().status, 1);
    assert.equal(readFileSync(external, "utf8"), "#!/bin/sh\n# external hook\n");
    rmSync(external);
    git("config", "core.hooksPath", "external-hooks");
    assert.equal(invoke().status, 1);
    git("config", "--unset", "core.hooksPath");
    for (const flag of ["--pr-only", "--branches-only", "main"]) {
      const old = invoke([flag]);
      assert.equal(old.status, 1);
      assert.match(old.stderr, /takes no arguments/);
      for (const name of ["pre-commit", "pre-push", "reference-transaction"]) assert.equal(existsSync(path.join(hooks, name)), false, flag);
    }
    // A core.hooksPath injected for one process tree does not count.
    const injected = invoke([], { GIT_CONFIG_COUNT: "1", GIT_CONFIG_KEY_0: "core.hooksPath", GIT_CONFIG_VALUE_0: "external-hooks" });
    assert.equal(injected.status, 0, injected.stderr);
    for (const name of ["pre-commit", "pre-push"]) {
      assert.match(readFileSync(path.join(hooks, name), "utf8"), /^# octoboard repository hook entry/m);
    }
    assert.equal(existsSync(path.join(hooks, "reference-transaction")), false);

    // The earlier revert-gate entry point is overwritten; its reference-transaction hook and config go.
    for (const name of ["pre-commit", "pre-push", "reference-transaction"]) {
      writeFileSync(path.join(hooks, name), "#!/bin/sh\n# revert-gate hook entry\n");
    }
    git("config", "--add", "revert-gate.branch", "main");
    git("config", "--add", "revert-gate.branch", "other");
    const migrated = invoke();
    assert.equal(migrated.status, 0, migrated.stderr);
    assert.match(migrated.stdout, /Removed the reference-transaction hook/);
    assert.match(migrated.stdout, /Unset the revert-gate\.branch/);
    for (const name of ["pre-commit", "pre-push"]) {
      assert.match(readFileSync(path.join(hooks, name), "utf8"), /octoboard repository hook entry/);
    }
    assert.equal(existsSync(path.join(hooks, "reference-transaction")), false);
    assert.equal(git("config", "--get-all", "revert-gate.branch"), null);

    // A foreign reference-transaction hook is left alone, with a note.
    writeFileSync(path.join(hooks, "reference-transaction"), "#!/bin/sh\n# mine\n");
    const foreign = invoke();
    assert.equal(foreign.status, 0);
    assert.match(foreign.stderr, /left alone/);
    assert.equal(readFileSync(path.join(hooks, "reference-transaction"), "utf8"), "#!/bin/sh\n# mine\n");
    assert.equal(invoke().status, 0);
  });
});

test("pnpm install's hook installer installs on the main branch of the main working copy only, and never fails", () => {
  fixture(({ root, git }) => {
    mkdirSync(path.join(root, ".githooks"));
    for (const name of ["auto-install.sh", "install.sh", "hook.sh"]) {
      copyFileSync(new URL(`../../.githooks/${name}`, import.meta.url), path.join(root, ".githooks", name));
    }
    const hooks = path.join(root, ".git/hooks");
    const hook = path.join(hooks, "pre-commit");
    const env = { ...process.env };
    delete env.CI;
    const invoke = (cwd = root, extra = {}) => spawnSync("sh", [path.join(root, ".githooks/auto-install.sh")], { cwd, encoding: "utf8", env: { ...env, ...extra } });
    const clear = () => rmSync(hooks, { recursive: true, force: true });

    assert.equal(invoke(root, { CI: "true" }).status, 0);
    assert.equal(existsSync(hook), false);

    git("worktree", "add", "-q", "--detach", path.join(root, "linked"));
    assert.equal(invoke(path.join(root, "linked")).status, 0);
    assert.equal(existsSync(hook), false);

    // No remote names a default branch: the main branch falls back to `main`.
    const fresh = invoke();
    assert.equal(fresh.status, 0, fresh.stderr);
    assert.match(readFileSync(hook, "utf8"), /octoboard repository hook entry/);

    // The remote's default branch is the main branch: with another branch checked out nothing is installed,
    // and with it checked out the hooks are.
    clear();
    git("checkout", "-q", "-b", "trunk");
    git("update-ref", "refs/remotes/origin/trunk", "HEAD");
    git("symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/trunk");
    assert.equal(invoke().status, 0);
    assert.match(readFileSync(hook, "utf8"), /octoboard repository hook entry/);
    clear();
    git("checkout", "-q", "main");
    assert.equal(invoke().status, 0);
    assert.equal(existsSync(hook), false);
    // The first remote's HEAD is used when origin has none.
    git("symbolic-ref", "--delete", "refs/remotes/origin/HEAD");
    git("update-ref", "refs/remotes/destination/main", "HEAD");
    git("symbolic-ref", "refs/remotes/destination/HEAD", "refs/remotes/destination/main");
    assert.equal(invoke().status, 0);
    assert.match(readFileSync(hook, "utf8"), /octoboard repository hook entry/);
    git("symbolic-ref", "--delete", "refs/remotes/destination/HEAD");

    // Another revision checked out would install its own, possibly older, hook.sh.
    clear();
    git("checkout", "-q", "--detach");
    assert.equal(invoke().status, 0);
    assert.equal(existsSync(hook), false);
    git("checkout", "-q", "main");

    // A hook system owning core.hooksPath calls hook.sh itself: nothing to install or warn about.
    git("config", "core.hooksPath", "external-hooks");
    const owned = invoke();
    assert.equal(owned.status, 0);
    assert.equal(owned.stderr, "");
    assert.equal(existsSync(hook), false);
    git("config", "--unset", "core.hooksPath");

    mkdirSync(hooks);
    writeFileSync(hook, "#!/bin/sh\n# external hook\n");
    const refused = invoke();
    assert.equal(refused.status, 0);
    assert.match(refused.stderr, /were not installed; fix the cause and run pnpm hooks:install/);
    assert.equal(readFileSync(hook, "utf8"), "#!/bin/sh\n# external hook\n");
  });
});

test("the hook skips a stage whose script this checkout lacks, with a warning, and fails a script that fails", () => {
  fixture(({ root, base, head }) => {
    const bin = path.join(root, "bin");
    mkdirSync(bin);
    const node = path.join(bin, "node");
    writeFileSync(node, "#!/bin/sh\nexit 1\n");
    chmodSync(node, 0o755);
    const hook = path.join(root, ".git/hooks/pre-push");
    copyFileSync(new URL("../../.githooks/hook.sh", import.meta.url), hook);
    const run = (name, inputText = "") => spawnSync("sh", [hook, name, "destination", "url"], {
      cwd: root, input: inputText, encoding: "utf8", env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
    });
    const pushed = update(head, base);
    for (const [name, script] of [["pre-push", "pre-push.mjs"], ["pre-commit", "format-staged.mjs"]]) {
      const skipped = run(name, name === "pre-push" ? pushed : "");
      assert.equal(skipped.status, 0, name);
      assert.match(skipped.stderr, new RegExp(`${script} is missing`));
      mkdirSync(path.join(root, "scripts"), { recursive: true });
      writeFileSync(path.join(root, "scripts", script), "");
      assert.equal(run(name, name === "pre-push" ? pushed : "").status, 1, name);
    }
  });
});
