#!/usr/bin/env node
// Read-only push verification. --base names the live destination tip, --head pins the pushed commit;
// --full checks every consumer, --release checks the shipping app only, --dry-run prints the plan,
// and --fresh ignores earlier passes. Formatting belongs to the pre-commit hook.
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { changedFiles, requirePushedTree, resolveBase } from "./verify/changes.mjs";
import { acquireSlot, describeWait } from "./verify/lock.mjs";
import { clearFailure, failedFiles, narrowedFiles, readFailure, recordFailure, REPORT_ENV } from "./verify/rerun.mjs";
import { MENU_KEYS_FILE, planScope } from "./verify/scope.mjs";
import { contentKey, readStamp, recordPass, stampDirectory, stepKey } from "./verify/stamp.mjs";
import { checkSteps } from "./verify/steps.mjs";
import { workingTree } from "./verify/tree.mjs";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const baseArg = args.includes("--base") ? args[args.indexOf("--base") + 1] : undefined;
const headArg = args.includes("--head") ? args[args.indexOf("--head") + 1] : undefined;
const known = new Set(["--full", "--release", "--fresh", "--dry-run", "--base", "--head", baseArg, headArg]);
const unknown = args.filter((arg) => !known.has(arg));
if (unknown.length > 0 || (flag("--base") && (!baseArg || baseArg.startsWith("--"))) || (flag("--head") && (!headArg || headArg.startsWith("--")))) {
  console.error(`verify: unknown or incomplete argument: ${unknown.join(" ") || "--base / --head needs a ref"}`);
  process.exit(2);
}

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  return result.status === 0 ? result.stdout.trim() : null;
}
const git = (...gitArgs) => run("git", gitArgs);

const root = git("rev-parse", "--show-toplevel");
if (!root) {
  console.error("verify: not inside a git working copy");
  process.exit(2);
}
process.chdir(root);
const stampDir = stampDirectory(root, git("rev-parse", "--git-common-dir"));
const toolchain = [
  `node ${process.version}`,
  `pnpm ${run("pnpm", ["--version"]) ?? "?"}`,
  `rustc ${run("rustc", ["--version"]) ?? "none"}`,
];

// The UI's message catalog holds the `menu.*` keys the desktop shell's menu reads, a relation
// across packages; only a change to one of those lines counts as touching it.
function changesMenuKeys(base, tree) {
  const diff = git("diff", "-U0", base, tree, "--", MENU_KEYS_FILE) ?? "";
  return diff.split("\n").some((line) => /^[+-]\s*["']?menu\./.test(line));
}

function plan(base) {
  const tree = workingTree(root);
  if (!tree) throw new Error("could not take the tree of the working copy");
  if (headArg) requirePushedTree(git, headArg, tree);
  let scope;
  if (flag("--release")) {
    scope = { mode: "release", reason: "shipping application", packages: [] };
  } else if (flag("--full")) {
    scope = { mode: "full", reason: "--full", packages: [] };
  } else if (!base.commit) {
    // Only this command takes --base, so the hint is added here.
    const reason = base.noTarget ? `${base.reason} (pass --base <ref>)` : base.reason;
    scope = { mode: "full", reason, packages: [] };
  } else {
    const files = changedFiles(git, base.commit, tree);
    if (!files) throw new Error("could not read changed files; refusing to skip checks");
    const fullFiles = files.includes(MENU_KEYS_FILE) && changesMenuKeys(base.commit, tree) ? [MENU_KEYS_FILE] : [];
    scope = planScope(files, { fullFiles });
  }
  return { tree, scope, checks: checkSteps(scope) };
}

function exec(cmd, env = process.env) {
  return new Promise((resolve) => {
    const child = spawn(cmd[0], cmd.slice(1), { stdio: "inherit", env });
    // A signal goes to the child and the run ends once it has closed, so the slot is not released
    // while the step's own processes are still running.
    let signaled = null;
    const forward = (signal) => {
      signaled = signal;
      child.kill(signal);
    };
    process.on("SIGINT", forward);
    process.on("SIGTERM", forward);
    child.on("close", (code) => {
      process.off("SIGINT", forward);
      process.off("SIGTERM", forward);
      if (signaled) process.exit(128 + (signaled === "SIGINT" ? 2 : 15));
      resolve(code ?? 1);
    });
    child.on("error", () => resolve(127));
  });
}

// Whether every pass so far could be recorded against the content the run started on.
let recorded = true;

// A step that can be narrowed (`rerun`) and failed last time on content that differs from `tree` only
// in isolated test files runs on the failed and changed files alone (`scripts/verify/rerun.mjs`); its
// failed files are recorded when it fails, for the next run to narrow to.
async function runStep(step, key, tree) {
  const files = step.rerun && !flag("--fresh") ? narrowedFiles(git, readFailure(stampDir, root, step), tree, toolchain) : null;
  const commands = files ? step.rerun(files) : [step.cmd];
  if (files) {
    const which = files.length === 0 ? "no UI test file" : files.join(", ");
    console.log(`verify: ${step.label}: only test files nothing else imports changed since it failed, so of the UI's tests it re-runs ${which}`);
  }
  const reportDir = step.rerun ? mkdtempSync(path.join(os.tmpdir(), "verify-report-")) : null;
  const env = reportDir ? { ...process.env, [REPORT_ENV]: path.join(reportDir, "report.json") } : process.env;
  const started = Date.now();
  let code = 0;
  for (const cmd of commands) {
    console.log(`verify: ${step.label}: ${cmd.join(" ")}`);
    code = await exec(cmd, env);
    if (code !== 0) break;
  }
  if (step.rerun) {
    // Like a pass, a failure is recorded only for the content it ran on.
    const failed = code === 0 || workingTree(root) !== tree ? null : failedFiles(env[REPORT_ENV], root);
    if (failed) recordFailure(stampDir, root, step, { tree, toolchain: toolchain.join("\n"), files: failed });
    else clearFailure(stampDir, root, step);
    rmSync(reportDir, { recursive: true, force: true });
  }
  if (code !== 0) {
    console.error(`verify: FAILED ${step.label} (exit ${code}); later steps were not run`);
    process.exit(code);
  }
  // A pass is recorded only for the content it ran on: a step that rewrote files, or an edit made
  // meanwhile, leaves the content different from the one the stamp is keyed by.
  if (workingTree(root) === tree) recordPass(stampDir, key, step);
  else {
    recorded = false;
    console.log(`verify: the content changed while "${step.label}" ran; its pass is not recorded`);
  }
  console.log(`verify: passed ${step.label} in ${Math.round((Date.now() - started) / 1000)}s`);
}

const base = flag("--full") || flag("--release") ? {} : resolveBase(git, baseArg);
const current = plan(base);
const describe = (scope) => `verify: scope ${scope.mode} (${scope.reason})`;
console.log(describe(current.scope));
if (current.scope.mode === "unread") {
  console.log("verify: nothing to run: no format check or lint covers these files");
  process.exit(0);
}
if (flag("--dry-run")) {
  for (const step of current.checks) console.log(`verify: would run${step.heavy ? " (heavy)" : ""}: ${step.cmd.join(" ")}`);
  process.exit(0);
}

const passedOn = (tree, step) => !flag("--fresh") && stepKey(step) in readStamp(stampDir, contentKey(tree, toolchain));
if (current.checks.every((step) => passedOn(current.tree, step))) {
  console.log("verify: this content already passed these checks with this toolchain; nothing to run (--fresh forces a run)");
  process.exit(0);
}

const key = contentKey(current.tree, toolchain);
const pending = current.checks.filter((step) => !passedOn(current.tree, step));
for (const step of current.checks) {
  if (!pending.includes(step)) console.log(`verify: skipped ${step.label}: passed earlier on this content`);
}
// The steps to run, with consecutive heavy ones grouped to run in one hold of the slot: it is served
// first come, first served, so releasing it between them would send the next one to the back of the
// queue, behind every run that started waiting meanwhile.
const groups = [];
for (const step of pending) {
  const last = groups.at(-1);
  if (step.heavy && last?.[0].heavy) last.push(step);
  else groups.push([step]);
}
for (const group of groups) {
  let release = () => {};
  if (group[0].heavy) {
    release = await acquireSlot(`${path.basename(root)}: ${group.map((step) => step.label).join(", then ")}`, {
      onWait: (holders, ahead) => console.log(`verify: heavy step "${group[0].label}" is waiting for a free slot; ${describeWait(holders, ahead)}`),
    });
  }
  for (const step of group) await runStep(step, key, current.tree);
  release();
}
if (!recorded) {
  console.error("verify: the steps passed, but the content changed while they ran, so the content as it stands now was not verified as a whole; run again");
  process.exit(1);
}
console.log("verify: all checks passed");
