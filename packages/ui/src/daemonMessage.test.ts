import { expect, it } from "vitest";

import { appConfig } from "./appConfig";
import { EXAMPLE_CONVERSATION_ID } from "./conversationId";
import { daemonMessage } from "./daemonMessage";
import { CATALOGS } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Language } from "./i18n/languages";
import { DEFAULT_ACCOUNT_NAME, type Console, type Project, type Session, type ShellTerminal } from "./protocol";

const records = {
  consoles: new Map([["c1", { id: "c1", name: "Work" } as Console]]),
  projects: new Map<string, Project>(),
  sessions: new Map([["s1", { id: "s1", title: "Hub" } as Session]]),
};

it("words a known code from the catalog, showing a record by its name and an unknown record by its id", () => {
  expect(daemonMessage("en", "unknown_session", { session: "s1" }, "x", records)).toBe(`Unknown session ${isolate("Hub")}.`);
  expect(daemonMessage("en", "unknown_project", { project: "p9" }, "x", records)).toBe(`Unknown project ${isolate("p9")}.`);
  expect(daemonMessage("en", "path_not_found", { path: "/a/b" }, "x", records)).toBe(`${isolate("/a/b")} does not exist.`);
});

it("shows a shell terminal param by the terminal's name, and by its id once the terminal is gone", () => {
  const withTerminal = { ...records, shellTerminals: new Map([["t1", { id: "t1", title: "npm run dev" } as ShellTerminal]]) };
  expect(daemonMessage("en", "unknown_shell_terminal", { shell_terminal: "t1" }, "x", withTerminal)).toBe(`Unknown terminal ${isolate("npm run dev")}.`);
  expect(daemonMessage("en", "unknown_shell_terminal", { shell_terminal: "t2" }, "x", withTerminal)).toBe(`Unknown terminal ${isolate("t2")}.`);
});

it("words an invalid conversation ID with the example the UI supplies", () => {
  expect(daemonMessage("en", "resume_id_invalid", { id: "x" }, "y", records)).toContain(isolate(EXAMPLE_CONVERSATION_ID));
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

it("words a failed launch's reason from the failure's own code and params, keeping the English reason for an unknown code", () => {
  const params = { title: "Hub", reason: "the English account", reason_code: "binary_not_found", binary: "codex" };
  expect(daemonMessage("en", "launch_failed", params, "x", records)).toBe(
    `Could not start ${isolate("Hub")}: ${isolate("codex")} was not found on ${isolate("PATH")} in the snapshotted shell environment.`,
  );
  expect(daemonMessage("en", "launch_failed", { ...params, reason_code: "newer" }, "x", records)).toBe(
    `Could not start ${isolate("Hub")}: ${isolate("the English account")}`,
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

it("localizes structured running-session lists using live titles without parsing punctuation", () => {
  const params = { count: "2", sessions: "legacy fallback", session_ids: JSON.stringify(["s1", "gone"]) };
  const message = daemonMessage("en", "session_has_running_sessions", params, "fallback", records);
  expect(message).toContain(`${isolate("Hub")}, ${isolate("gone")}`);
  expect(message).not.toContain("legacy fallback");
  expect(daemonMessage("en", "session_has_running_sessions", { ...params, session_ids: "bad" }, "fallback", records)).toContain("legacy fallback");
});

it.each([
  ["account_name_taken", "Claude Code 已有名为 ⁨默认⁩ 的账户。"],
  ["config_dir_taken", "Claude Code 的账户 ⁨默认⁩ 已在使用此配置目录。"],
] as const)("words the default account in a %s message by its name in the current language", (code, zh) => {
  const params = { agent: "Claude Code", name: "Default" };
  expect(daemonMessage("zh-Hans", code, params, "x", records)).toBe(zh.replace("Claude Code", isolate("Claude Code")));
  expect(daemonMessage("en", code, params, "x", records)).toContain(isolate("Default"));
  expect(daemonMessage("zh-Hans", code, { ...params, name: "Work" }, "x", records)).toContain(isolate("Work"));
});

it("words each branch or worktree still there with its reason in plain text, one apart from the next", () => {
  const items = JSON.stringify([
    { kind: "branch", name: "fix/a,b", branch: "fix/a,b", state: "unmerged", uncommitted: false, detail: null },
    { kind: "worktree", name: "/home/dev/wt", branch: null, state: "failed", uncommitted: false, detail: "error: x, y" },
  ]);
  const message = daemonMessage("en", "session_branches_not_removed", { count: "2", items }, "fallback", { ...records, homeDir: "/home/dev" });
  expect(message).toBe(`These were to be removed and are still there: Removing this failed: ${isolate("~/wt")} (detached HEAD): ${isolate("error: x, y")}. This branch is not merged: ${isolate("fix/a,b")}.`);
  const zh = daemonMessage("zh-Hans", "session_branches_not_removed", { count: "2", items }, "fallback", { ...records, homeDir: "/home/dev" });
  expect(zh).toBe(`以下内容本应删除，但仍然保留：以下内容删除失败：${isolate("~/wt")}（分离的 HEAD）：${isolate("error: x, y")}。以下分支尚未合并：${isolate("fix/a,b")}。`);
});

it("puts the reason what goes with a removed project's folder is unknown on a line of its own", () => {
  const message = daemonMessage("en", "project_branches_not_determined", { name: "Docs", detail: "fatal: not a git repository" }, "fallback", records);
  expect(message.endsWith(".\nfatal: not a git repository")).toBe(true);
});

it("says a single group of leftovers by its own lead alone", () => {
  const items = JSON.stringify([{ kind: "branch", name: "fix/a", branch: "fix/a", state: "unmerged", uncommitted: false, detail: null }]);
  expect(daemonMessage("en", "session_branches_not_removed", { count: "1", items }, "fallback", records)).toBe(`This branch is not merged: ${isolate("fix/a")}.`);
});

it.each([
  ["name", "100", "Name must be 100 characters or fewer.", "名称不能超过 100 个字符。"],
  ["description", "2000", "Description must be 2,000 characters or fewer.", "描述不能超过 2,000 个字符。"],
])("localizes the field and limit for an overlong %s", (field, limit, english, chinese) => {
  expect(daemonMessage("en", "field_too_long", { field, limit }, "x", records)).toBe(english);
  expect(daemonMessage("zh-Hans", "field_too_long", { field, limit }, "x", records)).toBe(chinese);
});

it("words an exhausted account's quota, with the time left when the reset is known and the window in the UI's language", () => {
  const params = { agent: "Claude Code", account: "Work", window: "5-hour" };
  expect(daemonMessage("en", "account_quota_exhausted", params, "x", records)).toBe(
    `${isolate("Work")} (${isolate("Claude Code")}) cannot run a session because it has used up its 5-hour quota.`,
  );
  const resetsAt = String(Date.now() + 2 * 60 * 60_000 + 30_000);
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", { ...params, resets_at: resetsAt }, "x", records)).toContain("5 小时额度已用完");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, resets_at: resetsAt }, "x", records)).toContain("It resets in 2h");
});

it("words an exhausted credit pool as credits and any other window as a quota of its own name", () => {
  const params = { agent: "Codex", account: "Work" };
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, window: "Credits" }, "x", records)).toContain("has used up its credits.");
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", { ...params, window: "Credits" }, "x", records)).toContain("：额度已用完。");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, window: "Opus weekly" }, "x", records)).toContain(`its ${isolate("Opus weekly")} quota.`);
});

it("shows the default account of an exhausted quota by its name in the UI's language", () => {
  const params = { agent: "Claude Code", account: DEFAULT_ACCOUNT_NAME, window: "Weekly" };
  expect(daemonMessage("en", "account_quota_exhausted", params, "x", records)).toContain(isolate("Default"));
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", params, "x", records)).toContain(isolate("默认"));
});

it("words a refused Grok sandbox with its profile and where it was set, leaving no placeholder in either language", () => {
  const params = { source: "GROK_SANDBOX", profile: "workspace" };
  for (const language of ["en", "zh-Hans"] as const) {
    const text = daemonMessage(language, "grok_sandbox_unsupported", params, "fallback", records);
    expect(text).toContain(isolate("workspace"));
    expect(text).toContain(isolate("GROK_SANDBOX"));
    expect(text).not.toMatch(/\{\w+\}/);
  }
});

it("resolves only producer-declared record IDs, preserving names that happen to equal another ID", () => {
  const state = { ...records, projects: new Map([["literal-name", { name: "Wrong project" } as Project]]), accounts: [{ id: "literal-name", name: "Wrong account" } as import("./protocol").Account] };
  expect(daemonMessage("en", "project_directory_shared", { project: "literal-name" }, "fallback", state)).toContain(isolate("literal-name"));
  expect(daemonMessage("en", "account_quota_exhausted", { account: "literal-name", agent: "Codex", window: "Weekly" }, "fallback", state)).toContain(isolate("literal-name"));
  expect(daemonMessage("en", "launch_failed", { title: "Draft", reason_code: "unknown_console", console: "c1" }, "fallback", state)).toContain(isolate("Work"));
});

it.each([
  ["login_codex_busy", {}], ["codex_unarchive_failed", { detail: "denied" }],
  ["grok_home_not_initialized", { path: "/tmp/home" }],
  ["agent_not_available", { agent: "Codex" }], ["claude_workspace_untrusted", {}],
] as const)("supplies fixed references for %s without unresolved placeholders in either language", (code, params) => {
  for (const language of ["en", "zh-Hans"] as const) expect(daemonMessage(language, code, params, "fallback", records)).not.toMatch(/\{\w+\}/);
});

it("keeps a nested detail-only shell failure usable without inventing missing command fields", () => {
  const params = { title: "Draft", reason_code: "shell_environment_timeout", detail: "environment capture failed\n    permission denied" };
  for (const language of ["en", "zh-Hans"] as const) {
    const plain = daemonMessage(language, "launch_failed", params, "fallback", records);
    expect(plain).toContain(isolate(params.detail));
    expect(plain.endsWith(isolate(params.detail))).toBe(true);
    const reason = "تعذر الالتقاط / capture failed";
    const withReason = daemonMessage(language, "launch_failed", { ...params, reason }, "fallback", records);
    expect(withReason.endsWith(isolate(reason))).toBe(true);
    expect(withReason).not.toContain(params.detail);
    expect(daemonMessage(language, "shell_environment_timeout", { detail: params.detail }, "original fallback", records)).toBe("original fallback");
    expect(plain).not.toMatch(/\{(?:shell|command|timeout)\}/);

  }
});
