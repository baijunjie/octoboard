import { describe, expect, it } from "vitest";

import { appConfig } from "./appConfig";
import { EXAMPLE_CONVERSATION_ID } from "./conversationId";
import { daemonMessage, daemonParams, richMessageCode } from "./daemonMessage";
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

it("words the refusals of an owner change under way without ids, naming the owner by its title", () => {
  expect(daemonMessage("en", "session_changing", { session: "s1" }, "x", records)).not.toContain("Try again");
  expect(daemonMessage("en", "session_owner_changing", { session: "s2", owner: "s1" }, "x", records)).toBe(
    `${isolate("Hub")} is being started, resumed, handed off, switched to another account or given another owner, so sessions cannot be bound to or moved from it right now.`,
  );
  expect(daemonMessage("en", "session_not_bound_to_owner", { session: "s2", owner: "s1" }, "x", records)).toContain(isolate("Hub"));
  expect(daemonMessage("zh-Hans", "session_owner_changing", { session: "s2", owner: "s1" }, "x", records)).toContain(isolate("Hub"));
  const withTasks = { ...records, sessions: new Map([["s1", { id: "s1", title: "Hub", terminal_title: "Fixing tests" } as Session], ["s3", { id: "s3", title: "" } as Session]]) };
  expect(daemonMessage("en", "session_not_bound_to_owner", { session: "s2", owner: "s1" }, "x", withTasks)).toContain("Fixing tests");
  expect(daemonMessage("en", "session_not_bound_to_owner", { session: "s2", owner: "s3" }, "x", withTasks)).toContain(isolate("s3"));
  for (const language of ["en", "zh-Hans"] as const) {
    expect(daemonMessage(language, "session_changing", { session: "s1" }, "x", records)).not.toContain("s1");
  }
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

it("names the console whose avatar an import refuses, and words the same refusal of a lone console without a name", () => {
  expect(daemonMessage("en", "icon_too_large", { limit_kib: "256", console_name: "Hub" }, "x", records)).toBe(`The avatar of the console ${isolate("Hub")} is larger than ${isolate("256")} KiB.`);
  expect(daemonMessage("en", "icon_not_an_image", { console_name: "Hub" }, "x", records)).toBe(`The avatar of the console ${isolate("Hub")} is not an image.`);
  expect(daemonMessage("en", "icon_too_large", { limit_kib: "256" }, "x", records)).toBe(`The console avatar is larger than ${isolate("256")} KiB.`);
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

it("words a worktree kept for a project folder that could not be checked by its own group, with what to do after it", () => {
  const items = JSON.stringify([
    { kind: "worktree", name: "/home/dev/wt", branch: null, state: "failed", uncommitted: false, detail: "whether it is or holds another project's folder could not be told", cause: "project_folder_unknown" },
  ]);
  const shown = { ...records, homeDir: "/home/dev" };
  expect(daemonMessage("en", "session_branches_not_removed", { count: "1", items }, "fallback", shown)).toBe(
    `Whether this worktree is or holds another project's folder could not be checked: ${isolate("~/wt")} (detached HEAD). `
      + "It may hold another project's files. Once it can be read, check what is in it before you remove it yourself.",
  );
  expect(daemonMessage("zh-Hans", "session_branches_not_removed", { count: "1", items }, "fallback", shown)).toBe(
    `无法检查以下工作树是否为或包含其他项目的文件夹：${isolate("~/wt")}（分离的 HEAD）。其中可能有其他项目的文件。请等到可以读取后先检查内容，再自行删除。`,
  );
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

it("words a refusal of a disabled account in the UI's language", () => {
  const params = { agent: "Codex", account: "Work" };
  expect(daemonMessage("en", "account_disabled", params, "x", records)).toBe(
    `${isolate("Work")} (${isolate("Codex")}) is disabled, so a session cannot be opened on it.`,
  );
  expect(daemonMessage("zh-Hans", "account_disabled", params, "x", records)).toContain("已停用");
});

it("words a refusal of an account that is not logged in in the UI's language, the default account under its localized name", () => {
  const params = { agent: "Codex", account: DEFAULT_ACCOUNT_NAME };
  expect(daemonMessage("en", "account_not_signed_in", params, "x", records)).toBe(
    `${isolate("Default")} (${isolate("Codex")}) is not logged in, so a session cannot be opened on it.`,
  );
  const chinese = daemonMessage("zh-Hans", "account_not_signed_in", params, "x", records);
  expect(chinese).toContain("未登录");
  expect(chinese).not.toContain("Default");
});

it("words an automatic account switch with the default account named in the UI's language", () => {
  const params = { agent: "Claude Code", from: DEFAULT_ACCOUNT_NAME, account: "Work", session: "s1" };
  expect(daemonMessage("en", "account_switched_automatically", params, "x", records)).toBe(
    `${isolate(DEFAULT_ACCOUNT_NAME)} (${isolate("Claude Code")}) reached its usage limit, so ${appConfig.name} switched ${isolate("Hub")} to ${isolate("Work")}.`,
  );
  const chinese = daemonMessage("zh-Hans", "account_switched_automatically", params, "x", records);
  expect(chinese).toContain("已达使用限制");
  expect(chinese).not.toContain(isolate(DEFAULT_ACCOUNT_NAME));
});

it("words an automatic account switch onto the default account in the UI's language", () => {
  const params = { agent: "Claude Code", from: "Work", account: DEFAULT_ACCOUNT_NAME, session: "s1" };
  expect(daemonMessage("en", "account_switched_automatically", params, "x", records)).toBe(
    `${isolate("Work")} (${isolate("Claude Code")}) reached its usage limit, so ${appConfig.name} switched ${isolate("Hub")} to ${isolate("Default")}.`,
  );
  const chinese = daemonMessage("zh-Hans", "account_switched_automatically", params, "x", records);
  expect(chinese).toContain("已达使用限制");
  expect(chinese).not.toContain("Default");
});

describe("an automatic account switch that failed after the session's process ended", () => {
  const params = {
    agent: "Claude Code",
    from: DEFAULT_ACCOUNT_NAME,
    account: "Work",
    account_id: "claude-a",
    name: "Work",
    reason: "the English account",
    reason_code: "account_sharing_in_progress",
  };

  it("words the sentence in both languages, the default account under its localized name", () => {
    expect(daemonMessage("en", "account_switch_automatically_failed", params, "x", records)).toBe(
      `${isolate("Default")} (${isolate("Claude Code")}) reached its usage limit, and ${appConfig.name} could neither switch this session to ${isolate("Work")} nor bring it back up on the account it had, so it is interrupted.`,
    );
    const chinese = daemonMessage("zh-Hans", "account_switch_automatically_failed", params, "x", records);
    expect(chinese).toContain("已达使用限制，");
    expect(chinese).toContain("既未能将此会话切换到");
    expect(chinese).toContain("也未能让它在原账户上重新运行，因此会话已中断。");
    expect(chinese).not.toContain("Default");
  });

  it("shows a move onto the default account under its localized name", () => {
    const toDefault = { ...params, from: "Work", account: DEFAULT_ACCOUNT_NAME };
    expect(daemonMessage("en", "account_switch_automatically_failed", toDefault, "x", records)).toContain(`switch this session to ${isolate("Default")}`);
    const chinese = daemonMessage("zh-Hans", "account_switch_automatically_failed", toDefault, "x", records);
    expect(chinese).toContain("切换到 ");
    expect(chinese).toContain("默认");
    expect(chinese).not.toContain("Default");
  });

  it("words the nested failure's reason from its own code and params", () => {
    expect(daemonParams("en", "account_switch_automatically_failed", params, records).reason).toBe(
      `Sharing for account ${isolate("Work")} is being changed.`,
    );
  });
});

it.each(["account_switched_automatically", "account_switch_automatically_failed"])(
  "sets the account names of %s apart as names, the default one under its localized name",
  (code) => {
    const params = { agent: "Claude Code", from: DEFAULT_ACCOUNT_NAME, account: "Work", reason: "why", reason_code: "newer" };
    const marked = daemonParams("zh-Hans", code, params, records, (value, monospace) => `[${monospace ? "code" : "name"}:${value}]`);
    expect(marked.account).toBe("[name:Work]");
    expect(marked.from).toMatch(/^\[name:/);
    expect(String(marked.from)).not.toContain("Default");
  },
);

it("words an exhausted account's quota, with the time left when the reset is known and the window in the UI's language", () => {
  const params = { agent: "Codex", account: "Work", window: "5-hour", window_kind: "session" };
  expect(daemonMessage("en", "account_quota_exhausted", params, "x", records)).toBe(
    `${isolate("Work")} (${isolate("Codex")}) cannot run a session because it has reached its 5h limit.`,
  );
  const resetsAt = String(Date.now() + 2 * 60 * 60_000 + 30_000);
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", { ...params, resets_at: resetsAt }, "x", records)).toContain("已达到5 小时限制");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, resets_at: resetsAt }, "x", records)).toContain("It resets in 2h");
});

it("words the session window as the agent's own interface names it", () => {
  const params = { agent: "Claude Code", account: "Work", window: "5-hour", window_kind: "session" };
  expect(daemonMessage("en", "account_quota_exhausted", params, "x", records)).toContain("has reached its session limit.");
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", params, "x", records)).toContain("已达到会话限制");
});

it("words Claude Code's weekly window for all models only beside a model's own weekly window", () => {
  const params = { agent: "Claude Code", account: "Work", window: "Weekly", window_kind: "weekly" };
  expect(daemonMessage("en", "account_quota_exhausted", params, "x", records)).toContain("has reached its weekly limit.");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, model_weekly: "true" }, "x", records)).toContain("has reached its weekly limit for all models.");
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", { ...params, model_weekly: "true" }, "x", records)).toContain("已达到所有模型的每周限制");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, agent: "Codex", model_weekly: "true" }, "x", records)).toContain("has reached its weekly limit.");
});

it("words a credit pool by its period or as credits, and a window without the structured kind by its own name", () => {
  const params = { agent: "Grok Build", account: "Work", window: "Weekly", window_kind: "credit_period" };
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, window_minutes: "10080" }, "x", records)).toContain("has reached its weekly limit.");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, window: "Monthly", window_minutes: "43200" }, "x", records)).toContain("has reached its monthly limit.");
  expect(daemonMessage("en", "account_quota_exhausted", { ...params, window: "Credits" }, "x", records)).toContain("has reached its credit limit.");
  expect(daemonMessage("en", "account_quota_exhausted", { agent: "Codex", account: "Work", window: "5-hour" }, "x", records)).toContain(`its ${isolate("5-hour")} limit.`);
  const codex = { agent: "Codex", account: "Work" };
  const credits = { ...codex, window: "Credits", window_kind: "credit_period" };
  expect(daemonMessage("en", "account_quota_exhausted", credits, "x", records)).toContain("has reached its credit limit.");
  expect(daemonMessage("zh-Hans", "account_quota_exhausted", credits, "x", records)).toContain("：已达到额度上限。");
  expect(daemonMessage("en", "account_quota_exhausted", { ...codex, window: "Opus weekly" }, "x", records)).toContain(`its ${isolate("Opus weekly")} limit.`);
  for (const unknown of [{ window_kind: "weekly_model" }, { agent: "Other" }]) {
    expect(daemonMessage("en", "account_quota_exhausted", { ...codex, window: "Weekly", window_kind: "weekly", ...unknown }, "x", records)).toContain(`its ${isolate("Weekly")} limit.`);
  }
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

it("words the account sharing errors and notices in every language, with the account's name and the failure's detail", () => {
  const params = { account: "a1", name: "Work", detail: "the default configuration moved", items: "[]" };
  const detailed = daemonMessage("en", "account_sharing_failed", params, "x", records);
  expect(detailed).toContain(isolate("Work"));
  expect(detailed).toMatch(/\nthe default configuration moved/);
  expect(daemonMessage("en", "account_still_sharing", { account: "a1", name: "Work" }, "x", records)).toContain(isolate("Work"));
  expect(daemonMessage("en", "unshared_state_database", { account: "a1", name: "Work", path: "/c/state_6.sqlite" }, "x", records)).toContain(isolate("/c/state_6.sqlite"));
  for (const language of Object.keys(CATALOGS) as Language[]) {
    for (const code of ["default_account_not_shareable", "account_sharing_failed", "account_still_sharing", "account_sharing_repair_failed", "unshared_state_database"]) {
      expect(daemonMessage(language, code, { ...params, path: "/c/state_6.sqlite" }, "fallback", records)).not.toBe("fallback");
    }
  }
});

it("words account_in_use by the count of running sessions and by whether the account is being signed in, in every language", () => {
  const named = { account: "a1", name: "Work" };
  const only = (sessions: string) => daemonMessage("en", "account_in_use", { ...named, sessions, signing_in: "false" }, "x", records);
  expect(only("1")).toBe(`Sharing for account ${isolate("Work")} cannot be changed while 1 of its sessions is working.`);
  expect(only("3")).toBe(`Sharing for account ${isolate("Work")} cannot be changed while 3 of its sessions are working.`);
  expect(daemonMessage("en", "account_in_use", { ...named, sessions: "0", signing_in: "true" }, "x", records)).toBe(`Sharing for account ${isolate("Work")} cannot be changed while it is being signed in.`);
  expect(daemonMessage("en", "account_in_use", { ...named, sessions: "2", signing_in: "true" }, "x", records)).toBe(`Sharing for account ${isolate("Work")} cannot be changed while 2 of its sessions are working and it is being signed in.`);
  for (const language of Object.keys(CATALOGS) as Language[]) {
    for (const [sessions, signing_in] of [["2", "false"], ["0", "true"], ["2", "true"]]) {
      const text = daemonMessage(language, "account_in_use", { ...named, sessions, signing_in }, "fallback", records);
      expect(text).not.toBe("fallback");
      expect(text).toContain(isolate("Work"));
      expect(text).not.toMatch(/\{\w+\}/);
    }
  }
});

it("words account_sharing_in_progress with the account's name in every language", () => {
  for (const language of Object.keys(CATALOGS) as Language[]) {
    const text = daemonMessage(language, "account_sharing_in_progress", { account: "a1", name: "Work" }, "fallback", records);
    expect(text).not.toBe("fallback");
    expect(text).toContain(isolate("Work"));
  }
});

it("says a failed sharing change rolled nothing back, and no longer calls it pending", () => {
  const text = daemonMessage("en", "account_sharing_failed", { account: "a1", name: "Work", detail: "x", items: "[]" }, "x", records);
  expect(text).toContain("Nothing was rolled back");
  expect(text).not.toContain("pending");
});

it("says why a sharing change left its entries, one short line per cause, in the UI's language", () => {
  const items = JSON.stringify([
    { item: "skills/", left: "unchanged", detail: "permission denied", cause: "permission_denied" },
    { item: "plugins/", left: "unchanged", detail: "permission denied", cause: "permission_denied" },
    { item: "abc", left: "unchanged", detail: "busy", sessions: ["s1"], cause: "busy" },
    { item: "hooks/", left: "linked", detail: "odd" },
    { item: "db", left: "absent", detail: "x", cause: "something_new" },
  ]);
  const params = { account: "a1", name: "Work", detail: "english summary", items };
  const english = daemonMessage("en", "account_sharing_failed", params, "x", records);
  expect(english).toContain(`You do not have permission to change ${isolate("skills/")} and 1 more.`);
  expect(english).toContain(`${isolate("Hub")} is in use.`);
  expect(english).toContain(`Could not change ${isolate("hooks/")} and 1 more.`);
  expect(english).toContain(`The conversation ${isolate("abc")}, recorded by ${isolate("Hub")}, was left in the default directory.`);
  expect(english).toContain(`${isolate("skills/")} was left unchanged.`);
  expect(english).not.toContain("permission denied");
  expect(english).not.toContain("english summary");
  const chinese = daemonMessage("zh-Hans", "account_sharing_repair_failed", params, "x", records);
  expect(chinese).toContain(`没有权限修改 ${isolate("skills/")} 和另外 1 项。`);
  expect(chinese).toContain(`${isolate("Hub")} 正在被使用。`);
  expect(chinese).toContain(`未能修改 ${isolate("hooks/")} 和另外 1 项。`);
});

it.each(["permission_denied", "busy", "not_found", "no_space", "conflict", "other"])("words the cause %s for one and for several entries in every language", (cause) => {
  for (const language of Object.keys(CATALOGS) as Language[]) {
    for (const count of [1, 3]) {
      const items = JSON.stringify(Array.from({ length: count }, (_, at) => ({ item: `e${at}`, left: "unchanged", detail: "d", cause })));
      const text = daemonMessage(language, "account_sharing_failed", { account: "a1", name: "Work", detail: "x", items }, "fallback", records);
      expect(text).toContain(isolate("e0"));
      expect(text).not.toMatch(/\{\w+\}/);
    }
  }
});

it("keeps a sharing repair's English reason when it names no entry, and leaves that reason out once the entries are listed", () => {
  const reason = "account `Work`'s links to the default configuration could not be checked: permission denied";
  const named = { account: "a1", name: "Work" };
  const text = daemonMessage("en", "account_sharing_repair_failed", { ...named, items: "[]" }, reason, records);
  expect(text).toContain("could not be brought up to date");
  expect(text).toMatch(new RegExp(`\\n${reason.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`));
  const items = JSON.stringify([{ item: "skills/", left: "unchanged", detail: "permission denied" }]);
  expect(daemonMessage("en", "account_sharing_repair_failed", { ...named, items }, reason, records)).not.toContain(reason);
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

it("words the notice that a session could not be resumed after a sharing change, in every language", () => {
  for (const language of Object.keys(CATALOGS) as Language[]) {
    const text = daemonMessage(language, "sharing_resume_failed", { account: "a1", name: "Work", detail: "no such file" }, "fallback", records);
    expect(text).not.toBe("fallback");
    expect(text).toContain(isolate("Work"));
    expect(text).toContain("no such file");
    expect(text).not.toMatch(/\{\w+\}/);
  }
});

it("says a sharing change did not start because a session did not stop, in every language", () => {
  const withSession = { ...records, sessions: new Map([["s1", { id: "s1", title: "Hub" } as Session]]) };
  expect(daemonMessage("en", "sharing_session_did_not_stop", { account: "a1", name: "Work", session: "s1" }, "x", withSession))
    .toBe(`${isolate("Hub")} did not stop in time, so sharing for account ${isolate("Work")} was not changed.`);
  for (const language of Object.keys(CATALOGS) as Language[]) {
    const text = daemonMessage(language, "sharing_session_did_not_stop", { account: "a1", name: "Work", session: "s1" }, "fallback", withSession);
    expect(text).not.toBe("fallback");
    expect(text).not.toMatch(/\{\w+\}/);
  }
});

it("words a session that did not stop as possibly still running, and a resume that failed as left interrupted", () => {
  const params = { account: "a1", name: "Work", detail: "timed out" };
  expect(daemonMessage("en", "sharing_resume_failed", { ...params, still_running: "true" }, "x", records)).toContain("may still be running; it was not restarted");
  expect(daemonMessage("en", "sharing_resume_failed", { ...params, still_running: "false" }, "x", records)).toContain("left interrupted");
  expect(daemonMessage("en", "sharing_resume_failed", params, "x", records)).toContain("left interrupted");
  expect(daemonMessage("zh-Hans", "sharing_resume_failed", { ...params, still_running: "true" }, "x", records)).toContain("可能仍在运行，未重新启动");
});

it.each([
  ["ambiguous", "is in the project’s folder in more than one worktree", "在多个工作树的项目文件夹中都存在"],
  ["launchable", "is something the system would run", "是系统会运行的内容"],
  ["not_private", "can be changed by other users of this machine", "可被此设备上的其他用户修改"],
  ["not_a_file_or_folder", "is not a file or folder", "不是文件或文件夹"],
  ["unreadable", "could not be read, so it is not opened", "无法读取"],
  ["other", "cannot be opened from a link", "无法通过链接打开"],
  ["a_future_cause", "cannot be opened from a link", "无法通过链接打开"],
])("words a path link refused for the cause %s in every language, an unknown one as other", (cause, english, chinese) => {
  const params = { path: "a.ts", cause };
  expect(daemonMessage("en", "path_link_refused", params, "x", records)).toContain(english);
  expect(daemonMessage("zh-Hans", "path_link_refused", params, "x", records)).toContain(chinese);
});

it("keeps the system's text of an unreadable path link as plain text, on a line of its own", () => {
  const params = { path: "a.ts", cause: "unreadable", detail: "Input/output error (os error 5)" };
  expect(daemonMessage("en", "path_link_refused", params, "x", records)).toMatch(/could not be read, so it is not opened\.\nInput\/output error \(os error 5\)$/);
  expect(daemonMessage("en", "path_link_refused", { path: "a.ts", cause: "other" }, "x", records)).not.toContain("\n");
});

it("words an outside_scope from a link click as not opened from a link, and any other as not shown", () => {
  expect(daemonMessage("en", "outside_scope", { path: "/x", action: "open" }, "x", records)).toContain("not opened from a link");
  expect(daemonMessage("zh-Hans", "outside_scope", { path: "/x", action: "open" }, "x", records)).toContain("不通过链接打开");
  expect(daemonMessage("en", "outside_scope", { path: "/x" }, "x", records)).toContain("is not shown");
});

it("words every path link refusal that carries the system's text, an unknown cause included, in each catalog", () => {
  for (const cause of ["unreadable", "other", "a_future_cause"]) {
    const key = `daemon.${richMessageCode("path_link_refused", { path: "a", cause, detail: "x" })}`;
    for (const catalog of Object.values(CATALOGS)) expect(key in catalog, `${key}`).toBe(true);
  }
});
