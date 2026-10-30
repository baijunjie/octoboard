// @vitest-environment jsdom
import { act } from "react";
import { isolate } from "../i18n/isolate";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { detailsButton, openDetails, press } from "../components/detailsTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { errorAlerts } from "../errorAlerts";
import { setLanguage } from "../i18n/language";
import type { Event, ImportedProject } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ImportResult } from "./ImportResult";
import { documentSettings, openTab, settingLinesOf } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type ImportedSettings = NonNullable<Extract<Event, { type: "configuration_imported" }>["settings_applied"]>;

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
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [
    { name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: Array.from({ length: 1200 }, (_, i) => project(i)) },
  ];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: null, settings_error: null, accounts: null }} /></DaemonProvider>));
  expect(container.textContent).toContain("1.200 projects created");
});

/** The items of a region's totals line, without the dot drawn between them. */
const totals = (container: Element, label = "Totals for consoles and projects") =>
  Array.from(container.querySelectorAll(`ul[aria-label="${label}"] > li`)).map((li) => li.textContent!.replace(/·$/, ""));

it("says nothing failed once rather than listing zeros, and lists a failure when there is one, each total as a list item", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (status: "created" | "failed") =>
    act(() =>
      root.render(
        <DaemonProvider value={daemon}>
          <ImportResult
            result={{
              type: "configuration_imported",
              id: null,
              consoles: [{ name: "C", status, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: status === "failed" ? "not_attempted" : "created" }] }], settings_applied: null, settings_error: null, accounts: null,
            }}
          />
        </DaemonProvider>,
      ),
    );
  render("created");
  expect(totals(container)).toEqual(["1 console created", "1 project created", "Nothing failed"]);
  expect(container.textContent).not.toContain("0 failed");
  render("failed");
  expect(totals(container)).toEqual(["1 failed", "1 project not attempted"]);
  expect(container.textContent).not.toContain("Nothing failed");
});

it("gives each console a card named by its heading with its own outcome, and a row for each project with its outcome as text", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [
    { name: "Fine", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1), { ...project(2), status: "left_out" as const, via: null }] },
    { name: "Broken", status: "failed" as const, console: null, error: { code: "path_not_found", message: "no", params: { path: "/missing" } }, account_fallbacks: [], projects: [{ ...project(3), status: "not_attempted" as const, via: null }] },
    { name: "Empty", status: "created" as const, console: "c-3", error: null, account_fallbacks: [], projects: [] },
  ];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: null, settings_error: null, accounts: null }} /></DaemonProvider>));
  expect(container.querySelector("table")).toBeNull();
  const cards = [...container.querySelectorAll("section")];
  expect(cards.map((card) => card.querySelector("h3")!.textContent)).toEqual(["Fine", "Broken", "Empty"]);
  for (const card of cards) {
    expect(document.getElementById(card.getAttribute("aria-labelledby")!)).toBe(card.querySelector("h3"));
  }
  // The console's own outcome is in its header, whatever it is, and each project's is a word in its row.
  expect(cards.map((card) => card.querySelector(":scope > div")!.textContent)).toEqual(["FineCreated", "BrokenFailed", "EmptyCreated"]);
  expect([...cards[0].querySelectorAll("li")].map((li) => li.textContent)).toEqual(["P1CreatedAt its exported path", "P2Left out"]);
  // A failed console's reason is under its header, and its project says it was not tried.
  expect(cards[1].textContent).toContain("/missing");
  expect(cards[1].textContent).toContain("Its console could not be created.");
  expect(cards[1].querySelectorAll("li")).toHaveLength(1);
  expect(cards[2].querySelectorAll("li")).toHaveLength(0);
  expect(cards[2].textContent).toContain("This console has no projects.");
});

it("colours a row's Details by its line: the warning's beside a reached outcome, danger under a failure", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const failure = { code: "internal_error", message: "boom", params: { detail: "raw diagnostic text" } };
  const accounts = {
    agents_added: [],
    agents: [{
      agent: "claude" as const,
      default_name_applied: false,
      accounts: [
        { name: "Work", status: "created" as const, account: "a1", error: null, sharing_warning: failure },
        { name: "Sandbox", status: "failed" as const, account: null, error: failure, sharing_warning: null },
      ],
    }],
  };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles: [], accounts, settings_applied: null, settings_error: null }} /></DaemonProvider>));
  const details = (name: string) => detailsButton([...container.querySelectorAll("li")].find((li) => li.textContent?.startsWith(name))!)!;
  expect(details("Work").className).toContain("alert-button--warning-soft");
  expect(details("Sandbox").className).toContain("button--danger-soft");
});

it("says nothing failed for the consoles and the accounts beside a failure to apply the settings", async () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const settings_error = { code: "path_not_found", params: { path: "/Users/dev/missing" }, message: "no" };
  const accounts = { agents_added: [], agents: [{ agent: "claude" as const, default_name_applied: false, accounts: [{ name: "Work", status: "created" as const, account: "a1", error: null, sharing_warning: null }] }] };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, accounts, settings_error, settings_applied: null }} /></DaemonProvider>));
  // The settings failure brings its tab forward; each other region's totals depend on its own failures alone.
  expect(container.textContent).toContain("The application settings could not be applied");
  expect(container.querySelector("[role=tab][aria-selected=true]")!.textContent).toBe("Application settings");
  // The failure's literal takes the danger tone of the line it is in.
  expect(container.querySelector("code")?.getAttribute("data-color")).toBe("danger");
  await openTab("Consoles and projects");
  expect(container.querySelector("ul[aria-label='Totals for consoles and projects']")!.textContent).toContain("Nothing failed");
  await openTab("Accounts");
  expect(container.querySelector("ul[aria-label='Totals for accounts']")!.textContent).toContain("Nothing failed");
});

it("lists the totals as items of their own in every language, with the dot between them hidden from assistive technology", () => {
  setLanguage("zh-Hans");
  onTestFinished(() => setLanguage("en"));
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: null, settings_error: null, accounts: null }} /></DaemonProvider>));
  expect(totals(container, "控制台和项目汇总")).toEqual(["已创建 1 个控制台", "已创建 1 个项目", "没有失败项"]);
  expect(container.querySelectorAll("ul[aria-label=控制台和项目汇总] [aria-hidden=true]")).toHaveLength(2);
});

it("announces a failure to apply the settings once focus has settled, with its title and description only", () => {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const settings_error = { code: "path_not_found", params: { path: "/Users/dev/missing" }, message: "no" };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_error, settings_applied: null, accounts: null }} /></DaemonProvider>));
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(0);
  act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(0);
  act(() => vi.advanceTimersByTime(1));
  const alerts = container.querySelectorAll('[role="alert"]');
  expect(alerts).toHaveLength(1);
  expect(alerts[0].textContent).toContain("The application settings could not be applied");
  expect(alerts[0].textContent).not.toContain("/Users/dev/missing is");
});

it("opens no error alert for partial import failures: the result screen already shows them", async () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const result = {
    type: "configuration_imported" as const,
    id: null,
    settings_applied: null,
    accounts: null,
    consoles: [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: "failed" as const, error: { code: "path_not_found", message: "Directory unavailable", params: { path: "/missing" } } }] }],
    settings_error: { code: "internal_error", message: "Settings unavailable", params: { detail: "Settings unavailable" } },
  };
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={result} /><ErrorAlerts daemon={daemon} /></DaemonProvider>));
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(totals(container)).toEqual(["1 console created", "1 failed"]);
  expect(container.querySelector("section li")!.textContent).toContain("/missing does not exist.");
  // The tab with the settings' failure is marked, and shows it once opened.
  expect(document.querySelector("[role=tab][aria-label='Application settings, needs attention']")).not.toBeNull();
  await openTab("Application settings");
  // The reason is the application's own sentence; what the system said is behind Details.
  expect(container.textContent).toContain("Something unexpected went wrong.");
  expect(container.textContent).not.toContain("Settings unavailable");
  expect(openDetails(container.querySelector('[data-slot="alert-root"]')!).textContent).toContain("Settings unavailable");
});

/** The lines of the settings card, by setting name. */
function settingLines(container: Element): ReturnType<typeof settingLinesOf> {
  return settingLinesOf(container.querySelector("section[aria-labelledby] > ul")!);
}

it("tells of each application setting that was applied, and of the git workflow", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (carried: Partial<ImportedSettings>) =>
    act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings(carried), settings_error: null, accounts: null }} /></DaemonProvider>));
  render({ auto_pull_repositories: true, auto_approve_session_requests: true, auto_approve_handoffs: true, ask_questions_as_choices: false });
  expect(container.querySelector("section > h3")?.textContent).toBe("Application settings applied");
  expect(settingLines(container)).toEqual({
    "Automatically pull repositories": { badge: "On" },
    "Automatically delete archived sessions whose conversation is gone": { badge: "Off" },
    "Automatically select an account": { badge: "Off" },
    "Automatically switch accounts at the usage limit": { badge: "Off" },
    "Order the sidebar by status": { badge: "Off" },
    "Start usage windows automatically": { badge: "Off" },
    "Automatically approve session requests": { badge: "On" },
    "Automatically approve resume requests": { badge: "Off" },
    "Automatically approve account switches": { badge: "Off" },
    "Automatically approve handoffs": { badge: "On" },
    "Ask questions as choices": { badge: "Off" },
    "Git workflow": { badge: "Matches the file" },
  });
});

it("tells what came of the keyboard shortcuts in the settings card, and the ones left out under them", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (shortcuts: { applied: number; skipped: number }) =>
    act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles: [], settings_applied: documentSettings({ auto_pull_repositories: true }), settings_error: null, accounts: null }} shortcuts={shortcuts} /></DaemonProvider>));
  render({ applied: 2, skipped: 0 });
  expect(settingLines(container)["Keyboard shortcuts"]).toEqual({ badge: "2 changes from the defaults" });
  render({ applied: 1, skipped: 2 });
  expect(settingLines(container)["Keyboard shortcuts"]).toEqual({ badge: "1 change from the default", note: "2 shortcuts in the file could not be applied here and were left out." });
  render({ applied: 0, skipped: 0 });
  expect(settingLines(container)["Keyboard shortcuts"]).toEqual({ badge: "Defaults" });
});

it("marks a console and a project whose account fell back with a warning mark after the name, the sentence in its popover, and lists no section of them", async () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [
    { name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [{ agent: "claude" as const, name: "Work" }], projects: [{ ...project(1), account_fallbacks: [{ agent: "codex" as const, name: "Personal" }] }, { ...project(2), status: "left_out" as const, via: null, account_fallbacks: [{ agent: "codex" as const, name: "Other" }] }] },
    { name: "Broken", status: "failed" as const, console: null, error: null, account_fallbacks: [{ agent: "claude" as const, name: "Gone" }], projects: [] },
  ];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, settings_applied: documentSettings({ auto_pull_repositories: true }), settings_error: null, accounts: null }} /></DaemonProvider>));
  expect([...container.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["C", "Broken"]);
  const marks = [...container.querySelectorAll("section button[data-status-mark]")].filter((button) => button.getAttribute("aria-label")!.startsWith("Accounts not set up for"));
  expect(marks.map((mark) => mark.getAttribute("aria-label"))).toEqual(["C", "P1", "P2", "Broken"].map((name) => `Accounts not set up for ${isolate(name)}`));
  // Each sits right after the name it qualifies.
  expect(marks[0].parentElement!.previousElementSibling?.textContent).toBe("C");
  expect(marks[2].parentElement!.previousElementSibling?.textContent).toBe("P2");
  expect(container.querySelector("section")!.textContent).not.toContain("is not set up");
  press(marks[1]);
  expect(document.querySelector("[data-status-mark-popover]")?.textContent).toBe("Codex account Personal is not set up in Octoboard, so the default account is used.");
  await openTab("Application settings");
  expect([...container.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["Application settings applied"]);
});

it("writes a failed line's reason under its row, with the raw text behind Details and not in the row", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const result = {
    type: "configuration_imported" as const,
    id: null,
    settings_applied: null,
    settings_error: null,
    accounts: null,
    consoles: [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: "failed" as const, error: { code: "internal_error", message: "boom", params: { detail: "raw diagnostic text" } } }] }],
  };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={result} /></DaemonProvider>));
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
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const tabs = () => Array.from(container.querySelectorAll("[role=tab]")).map((tab) => tab.textContent);
  const accounts = { agents_added: [], agents: [] };
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles, accounts, settings_applied: null, settings_error: null }} /></DaemonProvider>));
  expect(tabs()).toEqual(["Consoles and projects", "Accounts"]);
  // Imported without the consoles, only the accounts are left: one panel, no tab bar.
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", id: null, consoles: [], accounts, settings_applied: null, settings_error: null }} /></DaemonProvider>));
  expect(tabs()).toEqual([]);
  expect(container.querySelector("section")).toBeNull();
});
