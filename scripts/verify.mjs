#!/usr/bin/env node
// Verifies the working copy: runs the checks the branch's changes can affect, cheap ones first, and
// stops at the first failure. `CLAUDE.md` "Before delivery" decides which checks a change calls for;
// `scripts/verify/scope.mjs` implements that rule so no one applies it by hand, and `steps.mjs` lists
// the checks.
//
//   pnpm verify                 the checks for the changes since the branch's fork point
//   pnpm verify --full          the full set, whatever changed
//   pnpm verify --fresh         ignore earlier passes of the same content
//   pnpm verify --dry-run       print the plan and run nothing
//   pnpm verify --base <ref>    take the changes since <ref> (a branch with no recorded fork point)
//
// The changes are those since the branch's recorded fork point (`branch.<name>.forkPoint`), which is
// also what a rebase or merge brought in: a plain rebase leaves the fork point where it was, so what
// the target branch gained since counts as changed. (`flow-restack.sh` moves it to the new merge
// base, which drops that range from the scope.) A branch with no fork point, or one that is not an
// ancestor of HEAD, cannot be judged and runs the full set.
//
// Steps that passed on the same content (tree and toolchain) are skipped. Heavy steps wait for a
// machine-wide slot (`scripts/verify/lock.mjs`), and consecutive ones hold it together; light ones
// never wait. The UI gallery's quick check is not run here: `agent-gallery` decides it from the
// scenarios touched.
import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { changedFiles, resolveBase } from "./verify/changes.mjs";
import { acquireSlot, describeWait } from "./verify/lock.mjs";
import { MENU_KEYS_FILE, planScope } from "./verify/scope.mjs";
import { contentKey, readStamp, recordPass, stampDirectory, stepKey } from "./verify/stamp.mjs";
import { checkSteps, formatSteps } from "./verify/steps.mjs";
import { workingTree } from "./verify/tree.mjs";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const baseArg = args.includes("--base") ? args[args.indexOf("--base") + 1] : undefined;
const known = new Set(["--full", "--fresh", "--dry-run", "--base", baseArg]);
const unknown = args.filter((arg) => !known.has(arg));
if (unknown.length > 0 || (flag("--base") && !baseArg)) {
  console.error(`verify: unknown or incomplete argument: ${unknown.join(" ") || "--base needs a ref"}`);
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
  let files = [];
  let scope;
  if (flag("--full")) {
    scope = { mode: "full", reason: "--full", packages: [], advice: [] };
  } else if (!base.commit) {
    // Only this command takes --base, so the hint is added here.
    const reason = base.noForkPoint ? `${base.reason} (pass --base <ref>)` : base.reason;
    scope = { mode: "full", reason, packages: [], advice: [] };
  } else {
    files = changedFiles(git, base.commit, tree) ?? [];
    const fullFiles = files.includes(MENU_KEYS_FILE) && changesMenuKeys(base.commit, tree) ? [MENU_KEYS_FILE] : [];
    scope = planScope(files, { fullFiles });
  }
  return { tree, files, scope, checks: checkSteps(scope, files), formats: formatSteps(scope, files) };
}

function exec(step) {
  return new Promise((resolve) => {
    const child = spawn(step.cmd[0], step.cmd.slice(1), { stdio: "inherit" });
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

async function runStep(step, key, tree) {
  console.log(`verify: ${step.label}: ${step.cmd.join(" ")}`);
  const started = Date.now();
  const code = await exec(step);
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

const base = resolveBase(git, baseArg);
let current = plan(base);
const describe = (scope) => `verify: scope ${scope.mode} (${scope.reason})`;
console.log(describe(current.scope));
for (const note of current.scope.advice) console.log(`verify: note: ${note}`);
if (current.scope.mode === "unread") {
  console.log("verify: nothing to run: no format check or lint covers these files");
  process.exit(0);
}
if (flag("--dry-run")) {
  for (const step of [...current.formats, ...current.checks]) console.log(`verify: would run${step.heavy ? " (heavy)" : ""}: ${step.cmd.join(" ")}`);
  process.exit(0);
}

const passedOn = (tree, step) => !flag("--fresh") && stepKey(step) in readStamp(stampDir, contentKey(tree, toolchain));
if (current.checks.every((step) => passedOn(current.tree, step))) {
  console.log("verify: this content already passed these checks with this toolchain; nothing to run (--fresh forces a run)");
  process.exit(0);
}

for (const step of current.formats) {
  console.log(`verify: ${step.label}: ${step.cmd.join(" ")}`);
  const code = await exec(step);
  if (code !== 0) {
    console.error(`verify: FAILED ${step.label} (exit ${code})`);
    process.exit(code);
  }
}
// Formatting may have rewritten files, so the plan and the content the stamp names are taken again.
if (current.formats.length > 0) current = plan(base);
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
if (current.scope.packages.includes("ui") || current.scope.mode === "full") {
  console.log("verify: the gallery quick check is not part of this command; run it through the agent-gallery skill");
}
if (!recorded) {
  console.error("verify: the steps passed, but the content changed while they ran, so the content as it stands now was not verified as a whole; run again");
  process.exit(1);
}
console.log("verify: all checks passed");
