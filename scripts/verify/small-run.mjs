// Which heavy commands are small enough to run without waiting for the heavy-step slot (`lock.mjs`): a vitest
// run that names at most `SMALL_RUN_FILES` test files, the run an agent makes over and over while working on a
// change. Queued behind a Rust suite it could wait many minutes for seconds of work; run at once, with its workers
// held to `SMALL_RUN_WORKERS`, it costs the machine little beside the slot's holder. Anything else, a full or
// related suite included, still waits for the slot.

// The most test files a run may name and still run at once, and the most workers it then gets.
export const SMALL_RUN_FILES = 8;
export const SMALL_RUN_WORKERS = 4;

const TEST_FILE = /\.test\.(?:[cm]?[jt]sx?)$/;
// The flags whose value is the next argument that a small run may carry: the test name filter.
const VALUE_FLAGS = new Set(["-t", "--testNamePattern"]);
// Flags whose cost does not follow the number of files: type checking the project, coverage, a browser, watching,
// tracing changed files through the import graph.
const HEAVY_FLAGS = new Set(["typecheck", "coverage", "browser", "watch", "w", "ui", "standalone", "changed"]);
const WORKER_FLAGS = new Set(["maxWorkers", "max-workers"]);

// The arguments to run `command args` with at once when it is a small run, its worker count capped; null when it
// is not one and waits for the slot. `exists(file)` says whether a named file exists (relative to the command's
// working directory). A run is small only when every argument after `vitest run` is a flag, the value of a name
// filter, or an existing test file, and at most `SMALL_RUN_FILES` are named. vitest takes a positional argument
// as a filter matching every path that contains it, so a named test file matches itself and, at most, the few
// files whose paths contain its own. Anything that could make vitest take no filter at all queues: `--`, after
// which vitest reads no filters, and a flag given without `=` right before a file, which vitest may take as the
// flag's value (`-u src/a.test.ts`), as does any other positional argument (a directory or a pattern).
export function smallRun(command, args, exists) {
  // vitest cuts its arguments at the first `--` before reading any, so one anywhere, even where it would be a
  // flag's value, leaves it with no filter.
  if (commandName(command) !== "vitest" || args[0] !== "run" || args.includes("--")) return null;
  const files = [];
  const kept = ["run"];
  let workers = SMALL_RUN_WORKERS;
  for (let index = 1; index < args.length; index++) {
    const arg = args[index];
    if (VALUE_FLAGS.has(arg)) {
      if (index + 1 >= args.length) return null;
      kept.push(arg, args[index + 1]);
      index++;
      continue;
    }
    // Short flags run together (`-uw`) are not looked into one by one: such a run queues.
    if (/^-[^-]./.test(arg)) return null;
    if (arg.startsWith("-")) {
      const [name, value] = arg.replace(/^-+/, "").split("=");
      if (HEAVY_FLAGS.has(name.split(".")[0])) return null;
      const next = args[index + 1];
      if (value === undefined && next !== undefined && !next.startsWith("-")) return null;
      if (WORKER_FLAGS.has(name)) {
        const given = Number.parseInt(value ?? "", 10);
        if (given > 0) workers = Math.min(workers, given);
        continue;
      }
      kept.push(arg);
      continue;
    }
    if (!TEST_FILE.test(arg) || !exists(arg)) return null;
    files.push(arg);
    kept.push(arg);
  }
  if (files.length === 0 || files.length > SMALL_RUN_FILES) return null;
  return [...kept, `--maxWorkers=${Math.min(workers, files.length)}`];
}

// The last component of a command, so that a path to the binary counts as the binary.
function commandName(command) {
  return command.split(/[\\/]/).at(-1);
}
