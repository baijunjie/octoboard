import { expect, it } from "vitest";

import type { ScheduledTask } from "../protocol";
import { taskName } from "./taskName";

function taskOf(prompt: string, title?: string): ScheduledTask {
  return {
    id: "t-1",
    console_id: "c-1",
    project_id: null,
    launch: { ...(title !== undefined && { title }), compatibility: {} },
    prompt,
    schedule: { kind: "manual" },
    enabled: true,
    created_at: 1,
    updated_at: 1,
    next_run_at: null,
    last_run_at: null,
    run_sessions: [],
  };
}

it("goes by the name a task was given, trimmed", () => {
  expect(taskName(taskOf("Check the build", "  Nightly  "))).toBe("Nightly");
});

it("goes by the first line of the prompt where it has no name, or a blank one", () => {
  expect(taskName(taskOf("Check the build\nthen report"))).toBe("Check the build");
  expect(taskName(taskOf("  Check the build  \r\nthen report", "   "))).toBe("Check the build");
});
