import assert from "node:assert/strict";
import { test } from "node:test";
import { SMALL_RUN_FILES, SMALL_RUN_WORKERS, smallRun } from "./small-run.mjs";

const everything = () => true;
const nothing = () => false;
const testFile = (index) => `src/a${index}.test.ts`;
const files = (count) => Array.from({ length: count }, (_, index) => testFile(index));

test("a vitest run naming existing test files is small, with the workers capped at the file count", () => {
  assert.deepEqual(smallRun("vitest", ["run", testFile(0)], everything), ["run", testFile(0), "--maxWorkers=1"]);
  assert.deepEqual(smallRun("vitest", ["run", ...files(3)], everything), ["run", ...files(3), "--maxWorkers=3"]);
});

test("the workers are capped at four however many files are named, and eight files are the most", () => {
  assert.equal(SMALL_RUN_WORKERS, 4);
  assert.equal(SMALL_RUN_FILES, 8);
  assert.deepEqual(smallRun("vitest", ["run", ...files(8)], everything), ["run", ...files(8), "--maxWorkers=4"]);
  assert.equal(smallRun("vitest", ["run", ...files(9)], everything), null);
});

test("a path to the vitest binary counts as vitest, and another command is never small", () => {
  for (const command of ["node_modules/.bin/vitest", "/abs/node_modules/.bin/vitest", "C:\\repo\\node_modules\\.bin\\vitest"]) {
    assert.ok(smallRun(command, ["run", testFile(0)], everything), command);
  }
  for (const command of ["vitest-extra", "node", "cargo", "/bin/notvitest"]) {
    assert.equal(smallRun(command, ["run", testFile(0)], everything), null, command);
  }
});

test("only `vitest run` is small: related, a bare invocation, or run with no files queue", () => {
  assert.equal(smallRun("vitest", ["related", testFile(0)], everything), null);
  assert.equal(smallRun("vitest", [testFile(0)], everything), null);
  assert.equal(smallRun("vitest", [], everything), null);
  assert.equal(smallRun("vitest", ["run"], everything), null);
  assert.equal(smallRun("vitest", ["run", "--reporter=dot"], everything), null);
});

test("every extension of a test file counts, and a file that is not a test file does not", () => {
  for (const extension of ["js", "ts", "jsx", "tsx", "mjs", "cjs", "mts", "cts"]) {
    assert.ok(smallRun("vitest", ["run", `a.test.${extension}`], everything), extension);
  }
  for (const name of ["a.ts", "a.spec.ts", "a.test.json", "a.test", "atest.ts", "a.test.ts.snap", "a.test.tsx.bak"]) {
    assert.equal(smallRun("vitest", ["run", name], everything), null, name);
  }
});

test("a positional that is not an existing test file makes the run queue", () => {
  assert.equal(smallRun("vitest", ["run", "src/lifecycle"], everything), null, "a directory");
  assert.equal(smallRun("vitest", ["run", "useApp"], everything), null, "a filter pattern");
  assert.equal(smallRun("vitest", ["run", testFile(0), "src/b"], everything), null, "one bad among good");
  assert.equal(smallRun("vitest", ["run", testFile(0)], nothing), null, "a missing file");
  assert.equal(smallRun("vitest", ["run", testFile(0), testFile(1)], (file) => file === testFile(0)), null, "one missing file");
});

test("existence is asked of the file as named", () => {
  const asked = [];
  smallRun("vitest", ["run", testFile(0), testFile(1)], (file) => (asked.push(file), true));
  assert.deepEqual(asked, [testFile(0), testFile(1)]);
});

test("flags are carried through, and the value after -t or --testNamePattern is not a file", () => {
  assert.deepEqual(smallRun("vitest", ["run", "--reporter=dot", testFile(0), "--bail=1"], everything), [
    "run",
    "--reporter=dot",
    testFile(0),
    "--bail=1",
    "--maxWorkers=1",
  ]);
  for (const flag of ["-t", "--testNamePattern"]) {
    assert.deepEqual(smallRun("vitest", ["run", testFile(0), flag, "renders the menu"], everything), [
      "run",
      testFile(0),
      flag,
      "renders the menu",
      "--maxWorkers=1",
    ]);
  }
  assert.ok(smallRun("vitest", ["run", "-t", "name", testFile(0)], everything), "the filter before the file");
  assert.ok(smallRun("vitest", ["run", testFile(0), "--testNamePattern=name"], everything), "the = form");
});

test("a pattern value is not counted as a file, and a test-file-looking one does not add to the count", () => {
  assert.deepEqual(smallRun("vitest", ["run", testFile(0), "-t", "x.test.ts"], everything), [
    "run",
    testFile(0),
    "-t",
    "x.test.ts",
    "--maxWorkers=1",
  ]);
  assert.ok(smallRun("vitest", ["run", ...files(8), "-t", "x.test.ts"], everything), "eight files and a pattern");
});

test("a flag without = right before a positional makes the run queue, since vitest may take it as the flag's value", () => {
  for (const args of [
    ["run", testFile(0), "--reporter", "dot"],
    ["run", testFile(0), "--project", "ui"],
    ["run", "-u", testFile(0)],
    ["run", "--bail", testFile(0)],
    ["run", "--reporter", "dot", testFile(0)],
    ["run", testFile(0), "--maxWorkers", "2"],
  ]) {
    assert.equal(smallRun("vitest", args, everything), null, args.join(" "));
  }
});

test("a bare flag followed by another flag or by nothing is small", () => {
  assert.deepEqual(smallRun("vitest", ["run", testFile(0), "-u"], everything), ["run", testFile(0), "-u", "--maxWorkers=1"]);
  assert.deepEqual(smallRun("vitest", ["run", testFile(0), "--bail", "--silent"], everything), [
    "run",
    testFile(0),
    "--bail",
    "--silent",
    "--maxWorkers=1",
  ]);
});

test("`--` anywhere after run makes the run queue", () => {
  assert.equal(smallRun("vitest", ["run", testFile(0), "--"], everything), null);
  assert.equal(smallRun("vitest", ["run", "--", testFile(0)], everything), null);
  assert.equal(smallRun("vitest", ["run", testFile(0), "--", "src"], everything), null);
  assert.equal(smallRun("vitest", ["run", "-t", "--", "a.test.ts"], everything), null, "as the value of -t");
  assert.equal(smallRun("vitest", ["run", testFile(0), "--testNamePattern", "--"], everything), null, "as the value of --testNamePattern");
});

test("a single-dash argument of more than one letter makes the run queue, a single letter does not", () => {
  for (const flag of ["-uw", "-wu", "-ut"]) {
    assert.equal(smallRun("vitest", ["run", testFile(0), flag], everything), null, flag);
    assert.equal(smallRun("vitest", ["run", flag, testFile(0)], everything), null, `${flag} before the file`);
  }
  assert.deepEqual(smallRun("vitest", ["run", testFile(0), "-u"], everything), ["run", testFile(0), "-u", "--maxWorkers=1"]);
  assert.deepEqual(smallRun("vitest", ["run", "-t", "x", testFile(0)], everything), ["run", "-t", "x", testFile(0), "--maxWorkers=1"]);
});

test("--changed makes the run queue, with or without a value", () => {
  assert.equal(smallRun("vitest", ["run", testFile(0), "--changed"], everything), null);
  assert.equal(smallRun("vitest", ["run", testFile(0), "--changed=main"], everything), null);
  assert.equal(smallRun("vitest", ["run", "--changed", testFile(0)], everything), null);
});

test("a name filter with no value after it makes the run queue", () => {
  assert.equal(smallRun("vitest", ["run", testFile(0), "-t"], everything), null);
  assert.equal(smallRun("vitest", ["run", testFile(0), "--testNamePattern"], everything), null);
});

test("flags that make a run cost more than its files make it queue, by name before = and before any .", () => {
  for (const flag of [
    "--typecheck",
    "--typecheck.only",
    "--typecheck=true",
    "--coverage",
    "--coverage.enabled=true",
    "--coverage.provider=v8",
    "--browser",
    "--browser.enabled=true",
    "--watch",
    "--watch=true",
    "-w",
    "--ui",
    "--standalone",
  ]) {
    assert.equal(smallRun("vitest", ["run", testFile(0), flag], everything), null, flag);
    assert.equal(smallRun("vitest", ["run", flag, testFile(0)], everything), null, `${flag} before the file`);
  }
  assert.ok(smallRun("vitest", ["run", testFile(0), "--watchman=1", "--coverages"], everything), "only the exact names are heavy");
});

test("a worker count given is replaced by one capped at 4 and at the file count", () => {
  const cases = [
    [["--maxWorkers=2"], 3, 2],
    [["--maxWorkers=8"], 8, 4],
    [["--max-workers=8"], 8, 4],
    [["--max-workers=2"], 3, 2],
    [["--maxWorkers=8"], 2, 2],
    [["--maxWorkers=100%"], 8, 4],
    [["--maxWorkers=100%"], 2, 2],
    [["--maxWorkers=abc"], 8, 4],
    [["--maxWorkers=0"], 8, 4],
    [["--maxWorkers=1", "--max-workers=3"], 8, 1],
  ];
  for (const [given, count, expected] of cases) {
    const result = smallRun("vitest", ["run", ...files(count), ...given], everything);
    assert.deepEqual(result, ["run", ...files(count), `--maxWorkers=${expected}`], `${given.join(" ")} over ${count} files`);
  }
  assert.deepEqual(smallRun("vitest", ["run", "--maxWorkers=2", testFile(0), "--bail"], everything), [
    "run",
    testFile(0),
    "--bail",
    "--maxWorkers=1",
  ], "the other arguments keep their order, the worker flag leaves");
});

test("the arguments given are not changed in place", () => {
  const args = ["run", testFile(0)];
  smallRun("vitest", args, everything);
  assert.deepEqual(args, ["run", testFile(0)]);
});
