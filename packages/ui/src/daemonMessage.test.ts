import { expect, it } from "vitest";

import { appConfig } from "./appConfig";
import { daemonMessage } from "./daemonMessage";
import { CATALOGS } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Language } from "./i18n/languages";
import type { Console, Project, Session } from "./protocol";

const records = {
  consoles: new Map([["c1", { id: "c1", name: "Work" } as Console]]),
  projects: new Map<string, Project>(),
  sessions: new Map([["s1", { id: "s1", title: "Hub" } as Session]]),
};

it("words a known code from the catalog, showing a record by its name and an unknown record by its id", () => {
  expect(daemonMessage("en", "unknown_session", { session: "s1" }, "x", records)).toBe(`Unknown session ${isolate("Hub")}.`);
  expect(daemonMessage("en", "unknown_project", { project: "p9" }, "x", records)).toBe(`Unknown project ${isolate("p9")}.`);
  expect(daemonMessage("en", "path_not_found", { path: "/a/b" }, "x", records)).toBe(`“${isolate("/a/b")}” does not exist.`);
});

it("words a trust answer's failure from its reason code, keeping the English reason for an unknown one", () => {
  const reason = "the confirmation did not go away after the key was sent";
  const agent = "Codex";
  const params = { agent, reason, reason_code: "screen_not_dismissed" };
  expect(daemonMessage("en", "trust_answer_failed", params, "x", records)).toBe(
    `${appConfig.name} could not press ${isolate("Codex")}'s trust confirmation (${reason}). Answer it in the terminal.`,
  );
  expect(daemonMessage("en", "trust_answer_failed", { agent, reason: "why", reason_code: "newer" }, "x", records)).toBe(
    `${appConfig.name} could not press ${isolate("Codex")}'s trust confirmation (${isolate("why")}). Answer it in the terminal.`,
  );
});

it("shows the daemon's own text for a code the catalog does not have", () => {
  expect(daemonMessage("en", "a_newer_code", {}, "the English text", records)).toBe("the English text");
});

it("picks the singular or plural wording from a numeric count param", () => {
  const params = (count: string, sessions: string) => ({ count, sessions });
  const one = daemonMessage("en", "session_has_running_sessions", params("1", "“A”"), "x", records);
  const many = daemonMessage("en", "session_has_running_sessions", params("2", "“A”, “B”"), "x", records);
  expect(one).toContain(`1 session under it is running: ${isolate("“A”")}.`);
  expect(many).toContain(`2 sessions under it are running: ${isolate("“A”, “B”")}.`);
});

// Each code a configuration import can be refused with, or report on a project, is worded by the
// catalog of every language that has one; a miss would show English, or the daemon's own text.
it.each([
  ["configuration_not_recognised", {}],
  ["configuration_version_unsupported", { version: "3", supported: "1" }],
  ["configuration_unreadable", { detail: "bad" }],
  ["import_resolution_invalid", { console: "0", project: "1" }],
  ["import_choice_unavailable", { choice: "clone" }],
  ["import_settings_missing", {}],
] as const)("words the configuration code %s in every catalog, each in its own language", (code, params) => {
  const english = daemonMessage("en", code, params, "the daemon's text", records);
  expect(english).not.toBe("the daemon's text");
  for (const language of Object.keys(CATALOGS).filter((candidate) => candidate !== "en") as Language[]) {
    // Worded in its own language, not the English a missing message falls back to.
    const worded = daemonMessage(language, code, params, "the daemon's text", records);
    expect(worded, language).not.toBe(english);
    expect(worded, language).not.toBe("the daemon's text");
  }
});
