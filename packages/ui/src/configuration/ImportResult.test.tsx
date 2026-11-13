// @vitest-environment jsdom
import { act } from "react";
import { isolate } from "../i18n/isolate";
import { expect, it, onTestFinished, vi } from "vitest";

import { detailsButton, openDetails, press } from "../components/alerts/detailsTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { ErrorAlerts } from "../shell/ErrorAlerts";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { messageText } from "../i18n/catalog";
import { setLanguage } from "../i18n/language";
import type { Event, ImportedProject } from "../protocol";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import type { Overrides } from "../shortcuts/store";
import { APP_BEHAVIOR_SWITCHES } from "./booleanSettings";
import { ImportResult } from "./ImportResult";
import { documentSettings, openTab, settingGroupsOf, settingLinesAcrossGroups, settingLinesOf } from "./testSupport";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type ImportedSettings = NonNullable<Extract<Event, { type: "configuration_imported" }>["settings_applied"]>;

/** What the accounts of a result hold when no account, agent or setting was dealt with. */
const NO_ACCOUNTS = { agents_added: [], agents: [], agent_settings_applied: null, agent_settings_error: null };

const project = (index: number): ImportedProject => ({
  name: `P${index}`,
  status: "created",
  via: "exported_path",
  project: `p-${index}`,
  error: null,
  account_fallbacks: [],
});

it("groups a count by the language of the application, not the browser's", () => {
  setLanguage("de");
  onTestFinished(() => setLanguage("en"));
  const consoles = [
    { name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: Array.from({ length: 1200 }, (_, i) => project(i)) },
  ];
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: null, settings_error: null, accounts: null }} />);
  expect(container.textContent).toContain("1.200 Projekte erstellt");
});

/** The items of a region's totals line, without the dot drawn between them. */
const totals = (container: Element, label = "Totals for consoles and projects") =>
  Array.from(container.querySelectorAll(`ul[aria-label="${label}"] > li`)).map((li) => li.textContent!.replace(/·$/, ""));

it("says nothing failed once rather than listing zeros, and lists a failure when there is one, each total as a list item", () => {
  const container = mountHost(null);
  const render = (status: "created" | "failed") =>
    container.render(
      <ImportResult
        result={{
          type: "configuration_imported",
          id: null,
          consoles: [{ name: "C", status, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: status === "failed" ? "not_attempted" : "created" }] }], settings_applied: null, settings_error: null, accounts: null,
        }}
      />,
    );
  render("created");
  expect(totals(container)).toEqual(["1 console created", "1 project created", "Nothing failed"]);
  expect(container.textContent).not.toContain("0 failed");
  render("failed");
  expect(totals(container)).toEqual(["1 failed", "1 project not attempted"]);
  expect(container.textContent).not.toContain("Nothing failed");
});

it("gives each console a card named by its heading with its own outcome, and a row for each project with its outcome as text", () => {
  const consoles = [
    { name: "Fine", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1), { ...project(2), status: "left_out" as const, via: null }] },
    { name: "Broken", status: "failed" as const, console: null, error: { code: "path_not_found", message: "no", params: { path: "/missing" } }, account_fallbacks: [], projects: [{ ...project(3), status: "not_attempted" as const, via: null }] },
    { name: "Empty", status: "created" as const, console: "c-3", error: null, account_fallbacks: [], projects: [] },
  ];
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: null, settings_error: null, accounts: null }} />);
  expect(container.querySelector("table")).toBeNull();
  const cards = [...container.querySelectorAll("section")];
  const headingOf = (card: Element) => document.getElementById(card.getAttribute("aria-labelledby")!)!;
  expect(cards.map((card) => headingOf(card).textContent)).toEqual(["Fine", "Broken", "Empty"]);
  // The console's own outcome is in its header, whatever it is, and each project's is a word in its row.
  const expectedHeaders = ["FineCreated", "BrokenFailed", "EmptyCreated"];
  expect(cards.map((card, i) => card.textContent!.slice(0, expectedHeaders[i].length))).toEqual(expectedHeaders);
  expect([...cards[0].querySelectorAll("li")].map((li) => li.textContent)).toEqual(["P1CreatedAt its exported path", "P2Left out"]);
  // A failed console's reason is in its header, under its name, and its project says it was not tried.
  expect(cards[1].textContent).toContain("/missing");
  expect(cards[1].textContent).toContain("Its console could not be created.");
  expect(cards[1].querySelectorAll("li")).toHaveLength(1);
  expect(cards[2].querySelectorAll("li")).toHaveLength(0);
  expect(cards[2].textContent).toContain("This console has no projects.");
});

it("colours a failed row's Details by its line, danger", () => {
  const failure = { code: "internal_error", message: "boom", params: { detail: "raw diagnostic text" } };
  const accounts = {
    ...NO_ACCOUNTS,
    agents: [{
      agent: "claude" as const,
      default_name_applied: false,
      default_disabled_applied: null,
      accounts: [
        { name: "Work", status: "created" as const, account: "a1", error: null },
        { name: "Sandbox", status: "failed" as const, account: null, error: failure },
      ],
    }],
  };
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], accounts, settings_applied: null, settings_error: null }} />);
  const details = (name: string) => detailsButton([...container.querySelectorAll("li")].find((li) => li.textContent?.startsWith(name))!)!;
  expect(details("Work")).toBeUndefined();
  expect(details("Sandbox").className).toContain("button--danger-soft");
});

it("tells of the default accounts switched and of the Agent accounts settings, which can fail on their own", async () => {
  const settingsError = { code: "internal_error", message: "boom", params: { detail: "raw diagnostic text" } };
  const agents = [
    { agent: "claude" as const, default_name_applied: false, default_disabled_applied: true, accounts: [] },
    { agent: "codex" as const, default_name_applied: false, default_disabled_applied: false, accounts: [] },
    { agent: "grok" as const, default_name_applied: false, default_disabled_applied: null, accounts: [] },
  ];
  const container = mountHost(
    <ImportResult
      result={{
        type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error: null,
        accounts: { ...NO_ACCOUNTS, agents, agent_settings_applied: { auto_select_account: true, auto_switch_on_usage_limit: false, auto_start_quota_windows: false, provider_settings: {}, agent_tags: {} }, agent_settings_error: null },
      }}
    />,
  );
  expect(container.textContent).toContain("The default account is now disabled.");
  expect(container.textContent).toContain("The default account is now enabled.");
  expect(container.textContent).not.toContain("The default account of Grok");
  expect(container.querySelector("section > div > h3")?.textContent).toBe("Agent account settings");
  expect(settingLinesOf(container.querySelector("section[aria-labelledby] > ul")!)).toEqual({
    "Automatically select an account": { badge: "On" },
    "Automatically switch accounts at the usage limit": { badge: "Off" },
    "Start usage windows automatically": { badge: "Off" },
  });
  container.render(
    <ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error: null, accounts: { ...NO_ACCOUNTS, agent_settings_error: settingsError } }} />,
  );
  expect(container.textContent).toContain("The agent account settings could not be applied");
  expect(container.textContent).not.toContain("Agent account settings");
  // The alert and "Nothing failed" do not both stand.
  expect(container.textContent).not.toContain("Nothing failed");
});

it("says nothing failed for the consoles and the accounts beside a failure to apply the settings", async () => {
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const settings_error = { code: "path_not_found", params: { path: "/Users/dev/missing" }, message: "no" };
  const accounts = { ...NO_ACCOUNTS, agents: [{ agent: "claude" as const, default_name_applied: false, default_disabled_applied: null, accounts: [{ name: "Work", status: "created" as const, account: "a1", error: null }] }] };
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, accounts, settings_error, settings_applied: null }} />);
  // The settings failure brings its tab forward; each other region's totals depend on its own failures alone.
  expect(container.textContent).toContain("The settings could not be applied");
  expect(container.querySelector("[role=tab][aria-selected=true]")!.textContent).toBe("Settings");
  // The failure's literal takes the danger tone of the line it is in.
  expect(container.querySelector("code")?.getAttribute("data-color")).toBe("danger");
  await openTab("Consoles and projects");
  expect(container.querySelector("ul[aria-label='Totals for consoles and projects']")!.textContent).toContain("Nothing failed");
  await openTab("Agent accounts");
  expect(container.querySelector("ul[aria-label='Totals for accounts']")!.textContent).toContain("Nothing failed");
});

it("lists the totals as items of their own in every language, with the dot between them hidden from assistive technology", () => {
  setLanguage("zh-Hans");
  onTestFinished(() => setLanguage("en"));
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: null, settings_error: null, accounts: null }} />);
  expect(totals(container, "控制台和项目汇总")).toEqual(["已创建 1 个控制台", "已创建 1 个项目", "没有失败项"]);
  expect(container.querySelectorAll("ul[aria-label=控制台和项目汇总] [aria-hidden=true]")).toHaveLength(2);
});

it("announces a failure to apply the settings once focus has settled, with its title and description only", () => {
  fakeTimers();
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const settings_error = { code: "path_not_found", params: { path: "/Users/dev/missing" }, message: "no" };
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_error, settings_applied: null, accounts: null }} />);
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(0);
  act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(0);
  act(() => vi.advanceTimersByTime(1));
  const alerts = container.querySelectorAll('[role="alert"]');
  expect(alerts).toHaveLength(1);
  expect(alerts[0].textContent).toContain("The settings could not be applied");
  expect(alerts[0].textContent).not.toContain("/Users/dev/missing is");
});

it("opens no error alert for partial import failures: the result screen already shows them", async () => {
  const daemon = testDaemon();
  const result = {
    type: "configuration_imported" as const,
    id: null,
    settings_applied: null,
    accounts: null,
    consoles: [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: "failed" as const, error: { code: "path_not_found", message: "Directory unavailable", params: { path: "/missing" } } }] }],
    settings_error: { code: "internal_error", message: "Settings unavailable", params: { detail: "Settings unavailable" } },
  };
  const container = mountHost(<><ImportResult result={result} /><ErrorAlerts daemon={daemon} /></>, { daemon });
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(totals(container)).toEqual(["1 console created", "1 failed"]);
  expect(container.querySelector("section li")!.textContent).toContain("/missing does not exist.");
  // The tab with the settings' failure is marked, and shows it once opened.
  expect(document.querySelector("[role=tab][aria-label='Settings, needs attention']")).not.toBeNull();
  await openTab("Settings");
  // The reason is the application's own sentence; what the system said is behind Details.
  expect(container.textContent).toContain("Something unexpected went wrong.");
  expect(container.textContent).not.toContain("Settings unavailable");
  expect(openDetails(container.querySelector('[data-slot="alert-root"]')!).textContent).toContain("Settings unavailable");
});

/** The lines of the settings cards, by setting name. */
function settingLines(container: Element): ReturnType<typeof settingLinesOf> {
  return settingLinesAcrossGroups(container);
}

it("gives every switch of the App behavior section a line in its card", () => {
  const every = Object.fromEntries(APP_BEHAVIOR_SWITCHES.map(({ field }) => [field, true]));
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings(every), settings_error: null, accounts: null }} />);
  const lines = settingGroupsOf(container).find((group) => group.title === "App behavior")!.lines;
  expect(lines).toEqual(APP_BEHAVIOR_SWITCHES.map(({ label }) => messageText("en", label)));
});

it("tells of each application setting that was applied, and lists the values of the git workflow", () => {
  const container = mountHost(null);
  const render = (carried: Partial<ImportedSettings>) =>
    container.render(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings(carried), settings_error: null, accounts: null }} />);
  render({ auto_pull_repositories: true, auto_approve_session_requests: true, auto_approve_handoffs: true, ask_questions_as_choices: false, keep_sessions_in_own_project: true });
  expect(settingGroupsOf(container)).toEqual([
    { title: "App behavior", lines: ["Close idle terminals when closing the panel"] },
    {
      title: "Agent behavior",
      lines: [
        "Automatically approve session requests",
        "Automatically approve resume requests",
        "Automatically approve account switches",
        "Automatically approve handoffs",
        "Ask questions as choices",
        "Keep sessions to their own project",
      ],
    },
    { title: "Git", lines: ["Automatically pull repositories", "Git workflow"] },
    { title: "Cleanup", lines: ["Automatically delete archived sessions whose conversation is gone"] },
  ]);
  expect(settingLines(container)).toEqual({
    "Close idle terminals when closing the panel": { badge: "Off" },
    "Automatically pull repositories": { badge: "On" },
    "Automatically delete archived sessions whose conversation is gone": { badge: "Off" },
    "Automatically approve session requests": { badge: "On" },
    "Automatically approve resume requests": { badge: "Off" },
    "Automatically approve account switches": { badge: "Off" },
    "Automatically approve handoffs": { badge: "On" },
    "Ask questions as choices": { badge: "Off" },
    "Keep sessions to their own project": { badge: "On" },
    "Git workflow": {
      badge: "Matches the file",
      values: [
        "Where sessions develop: In a worktree of its own",
        "Squash commits before delivery: Off",
        "Rebase before delivery: On",
        "Delivery: Ask each time",
        "Revert gate: Off",
      ],
    },
  });
});

it("tells of the default clone directory only where it was applied, and of the client's own settings in the card", () => {
  const container = mountHost(null);
  const render = (carried: Partial<ImportedSettings>, clientSettings?: Parameters<typeof ImportResult>[0]["clientSettings"]) =>
    container.render(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings(carried), settings_error: null, accounts: null }} clientSettings={clientSettings} />);
  render({});
  expect(Object.keys(settingLines(container))).not.toContain("Default clone directory");
  render({ default_clone_dir: "/Users/dev/Code" }, [{ field: "appearance", label: "settings.appearance.label", value: "Dark" }, { field: "interface_language", label: "settings.language.label", value: "日本語", lang: "ja" }]);
  expect(settingLines(container)["Default clone directory"]).toEqual({ badge: "Matches the file", note: "/Users/dev/Code" });
  expect(settingLines(container)["Appearance"]).toEqual({ badge: "Dark" });
  expect(settingLines(container)["Language"]).toEqual({ badge: "日本語" });
  // The language's name is marked as that language, and the directory stands as plain monospace text, not a chip.
  expect(container.querySelector("[lang=ja]")?.textContent).toBe("日本語");
  const directory = Array.from(container.querySelectorAll("div.font-mono")).find((node) => node.textContent === "/Users/dev/Code");
  expect(directory?.getAttribute("dir")).toBe("ltr");
  expect(directory?.classList.contains("align-match-parent")).toBe(true);
  expect(directory?.querySelector("[data-color]")).toBeNull();
});

it("keeps the clone directory that was written when the settings could not be applied, in a card beside the alert", () => {
  const settings_error = { code: "path_not_found", params: { path: "/Users/dev/missing" }, message: "no" };
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error, clone_dir_applied: "/Users/dev/Code", accounts: null }} />);
  expect(container.textContent).toContain("The settings could not be applied");
  expect(container.querySelector("[role=tab][aria-selected=true]")).toBeNull();
  // The one region stands without a tab bar, its alert first and the directory in the Git card, as the settings'.
  expect(settingGroupsOf(container)).toEqual([{ title: "Git", lines: ["Default clone directory"] }]);
  // The line says the directory was set before the rest failed, so it does not read as contradicting the alert.
  expect(settingLines(container)["Default clone directory"]).toEqual({ badge: "Matches the file", note: "Set before the other settings failed to apply./Users/dev/Code" });
  const text = container.textContent!;
  expect(text.indexOf("could not be applied")).toBeLessThan(text.indexOf("Default clone directory"));
});

it("tells of the clone directory once where the settings were applied too", () => {
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings({ default_clone_dir: "/Users/dev/Code" }), settings_error: null, clone_dir_applied: "/Users/dev/Code", accounts: null }} />);
  expect(container.textContent!.split("Default clone directory")).toHaveLength(2);
});

it("tells what came of the keyboard shortcuts in the settings card, and the ones left out under them", () => {
  const container = mountHost(null);
  const render = (shortcuts: { applied: number; skipped: number; overrides: Overrides }) =>
    container.render(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings({ auto_pull_repositories: true }), settings_error: null, accounts: null }} shortcuts={shortcuts} />);
  render({ applied: 2, skipped: 0, overrides: { focusMode: { code: "KeyK", primary: true, shift: true }, quickCreate: { code: "KeyJ", primary: true, shift: true } } });
  const listed = settingLines(container)["Changed shortcuts"];
  expect(listed.badge).toBe("2 changes from the defaults");
  expect(listed.values).toHaveLength(2);
  render({ applied: 1, skipped: 2, overrides: { focusMode: { code: "KeyK", primary: true, shift: true } } });
  expect(settingLines(container)["Changed shortcuts"]).toMatchObject({ badge: "1 change from the default", note: "2 shortcuts in the file could not be applied here and were left out." });
  expect(settingLines(container)["Changed shortcuts"].values).toHaveLength(1);
  render({ applied: 0, skipped: 0, overrides: {} });
  expect(settingLines(container)["Changed shortcuts"]).toEqual({ badge: "Defaults" });
});

it("marks a console and a project whose account fell back with a warning mark after the name, the sentence in its popover, and lists no section of them", async () => {
  const consoles = [
    { name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [{ agent: "claude" as const, name: "Work" }], projects: [{ ...project(1), account_fallbacks: [{ agent: "codex" as const, name: "Personal" }] }, { ...project(2), status: "left_out" as const, via: null, account_fallbacks: [{ agent: "codex" as const, name: "Other" }] }] },
    { name: "Broken", status: "failed" as const, console: null, error: null, account_fallbacks: [{ agent: "claude" as const, name: "Gone" }], projects: [] },
  ];
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: documentSettings({ auto_pull_repositories: true }), settings_error: null, accounts: null }} />);
  expect([...container.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["C", "Broken"]);
  const marks = [...container.querySelectorAll("section button[data-status-mark]")].filter((button) => button.getAttribute("aria-label")!.startsWith("Accounts not set up for"));
  expect(marks.map((mark) => mark.getAttribute("aria-label"))).toEqual(["C", "P1", "P2", "Broken"].map((name) => `Accounts not set up for ${isolate(name)}`));
  // Each sits right after the name it qualifies.
  expect(marks[0].parentElement!.previousElementSibling?.textContent).toBe("C");
  expect(marks[2].parentElement!.previousElementSibling?.textContent).toBe("P2");
  expect(container.querySelector("section")!.textContent).not.toContain("is not set up");
  press(marks[1]);
  expect(document.querySelector("[data-status-mark-popover]")?.textContent).toBe("The Codex account Personal is not set up here, so the default account is used.");
  await openTab("Settings");
  expect([...container.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["App behavior", "Agent behavior", "Git", "Cleanup"]);
});

it("writes a failed line's reason under its row, with the raw text behind Details and not in the row", () => {
  const result = {
    type: "configuration_imported" as const,
    id: null,
    settings_applied: null,
    settings_error: null,
    accounts: null,
    consoles: [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: "failed" as const, error: { code: "internal_error", message: "boom", params: { detail: "raw diagnostic text" } } }] }],
  };
  const container = mountHost(<ImportResult result={result} />);
  const row = container.querySelector("section li")!;
  // The row reads like the others: the glyph, then the word; the reason is a paragraph of its own under it.
  expect(row.querySelector("svg")).not.toBeNull();
  expect(row.textContent).toContain("Failed");
  expect(row.textContent).toContain("Something unexpected went wrong.");
  expect(row.querySelector("button[data-status-mark]")).toBeNull();
  expect(row.textContent).not.toContain("raw diagnostic text");
  expect(openDetails(row).textContent).toContain("raw diagnostic text");
});

it("shows tabs only for the regions that were imported", () => {
  const tabs = () => Array.from(container.querySelectorAll("[role=tab]")).map((tab) => tab.textContent);
  const accounts = NO_ACCOUNTS;
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles, accounts, settings_applied: null, settings_error: null }} />);
  expect(tabs()).toEqual(["Consoles and projects", "Agent accounts"]);
  // Imported without the consoles, only the accounts are left: one panel, no tab bar.
  container.render(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], accounts, settings_applied: null, settings_error: null }} />);
  expect(tabs()).toEqual([]);
  expect(container.querySelector("section")).toBeNull();
});

const conflict = { code: "write_conflict", message: "changed elsewhere", params: { fields: "auto_pull_repositories", versions: '{"auto_pull_repositories":9}' } };

it("words a settings write refused as a conflict by its cause and remedy, not as nothing saved, since the rest was created", () => {
  const container = mountHost(<ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error: conflict, accounts: null }} />);
  expect(container.textContent).toContain("Settings changed while the configuration was being imported.");
  expect(container.textContent).toContain("Check them in Settings and change any that should match the file.");
  expect(container.textContent).not.toContain("set them again");
  expect(container.textContent).not.toContain("nothing was saved");
});

it("words the agent account settings refused as a conflict the same way", () => {
  const container = mountHost(
    <ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: null, settings_error: null, accounts: { ...NO_ACCOUNTS, agent_settings_error: conflict } }} />,
  );
  expect(container.textContent).toContain("Settings changed while the configuration was being imported.");
  expect(container.textContent).toContain("Check them in Settings and change any that should match the file.");
  expect(container.textContent).not.toContain("nothing was saved");
});
