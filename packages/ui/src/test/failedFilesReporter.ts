import { writeFileSync } from "node:fs";

import type { Reporter, SerializedError, TestModule, TestRunEndReason } from "vitest/node";

// The environment variable through which `pnpm verify` asks for the report: the path to write it to.
export const REPORT_ENV = "OCTOBOARD_VERIFY_TEST_REPORT";

/** Writes, when a run ends, which test files failed (absolute paths) and whether the run is complete:
 * every file ran and nothing failed outside one, so the failed files account for every failure. `pnpm
 * verify` re-runs only those files when nothing but isolated test files changed since
 * (`scripts/verify/rerun.mjs`). */
export default class FailedFilesReporter implements Reporter {
  onTestRunEnd(modules: readonly TestModule[], unhandledErrors: readonly SerializedError[], reason: TestRunEndReason) {
    const outputFile = process.env[REPORT_ENV];
    if (!outputFile) return;
    const failed = modules.filter((module) => module.state() === "failed").map((module) => module.moduleId);
    const complete = reason !== "interrupted" && unhandledErrors.length === 0 && modules.every((module) => module.state() !== "pending" && module.state() !== "queued");
    writeFileSync(outputFile, JSON.stringify({ failed, complete }));
  }
}
