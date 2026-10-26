// @vitest-environment jsdom
import { act } from "react";
import { isolate } from "../i18n/isolate";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { openDetails } from "../components/detailsTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { errorAlerts } from "../errorAlerts";
import { setLanguage } from "../i18n/language";
import type { Event, ImportedProject } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ImportResult } from "./ImportResult";
import { settingLinesOf } from "./testSupport";

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
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles }} /></DaemonProvider>));
  expect(container.textContent).toContain("1.200 projects created");
});

it("says nothing failed once rather than listing zeros, and lists a failure when there is one", () => {
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
              consoles: [{ name: "C", status, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: status === "failed" ? "not_attempted" : "created" }] }],
            }}
          />
        </DaemonProvider>,
      ),
    );
  render("created");
  expect(container.textContent).toContain("1 console created. 1 project created. Nothing failed.");
  expect(container.textContent).not.toContain("0 failed");
  render("failed");
  expect(container.textContent).toContain("1 item failed. 1 project not attempted.");
  expect(container.textContent).not.toContain("Nothing failed");
});

// Stacked, a row is a block of values and the header is not drawn; the label in front of each value is
// all that says what it is the value of, and it is what a later restyle drops without anyone noticing.
it("keeps a label in front of every value of a row, for when the rows are stacked, a header for each column, and the console's name once as its group's header", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [
    { name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1), { ...project(2), status: "failed" as const, via: null }] },
  ];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles }} /></DaemonProvider>));
  const headers = [...container.querySelectorAll("thead th")].map((th) => th.textContent);
  expect(headers).toEqual(["Project", "Result", "Detail"]);
  // Each value is found by what it is and must carry the name of the column it belongs to: the
  // labels and the headers are written from one list, so only a known pair can show them swapped.
  const labelOf = (value: string) =>
    [...container.querySelectorAll("tbody td")].find((td) => td.textContent?.endsWith(value))?.querySelector("span[aria-hidden=true]")
      ?.textContent;
  expect(labelOf("P1")).toBe("Project");
  expect(labelOf("Created")).toBe("Result");
  expect(labelOf("At its exported path")).toBe("Detail");
  expect(container.querySelector("table")?.getAttribute("role")).toBe("table");
  // The console is named once, by the header cell that spans the group, which no value label stands in front of.
  const group = container.querySelector("tbody")!;
  const header = group.querySelector("th")!;
  expect(header.textContent).toBe("C");
  expect(header.getAttribute("role")).toBe("rowheader");
  expect(header.getAttribute("scope")).toBe("rowgroup");
  expect(header.getAttribute("colspan")).toBe("3");
  expect(group.querySelectorAll("th")).toHaveLength(1);
  expect([...group.querySelectorAll("tr")].map((tr) => tr.querySelectorAll("td").length)).toEqual([0, 3, 3]);
});

it("gives a console that failed or came out empty a line of its own in its group, headed by its name, and one group to each console", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [
    { name: "Fine", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] },
    { name: "Broken", status: "failed" as const, console: null, error: { code: "path_not_found", message: "no", params: { path: "/missing" } }, account_fallbacks: [], projects: [{ ...project(2), status: "not_attempted" as const, via: null }] },
    { name: "Empty", status: "created" as const, console: "c-3", error: null, account_fallbacks: [], projects: [] },
  ];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles }} /></DaemonProvider>));
  const groups = [...container.querySelectorAll("tbody")];
  expect(groups.map((group) => group.querySelector("th")!.textContent)).toEqual(["Fine", "Broken", "Empty"]);
  // A console with nothing of its own to say only heads its group.
  expect(groups[0].querySelector("th")!.getAttribute("scope")).toBe("rowgroup");
  expect(groups[0].querySelector("th")!.getAttribute("colspan")).toBe("3");
  expect(groups[0].querySelector("tr")!.querySelectorAll("td")).toHaveLength(0);
  // A failed or empty one is the row header of a line with its result.
  for (const [index, result] of [[1, "Failed"], [2, "Created"]] as const) {
    const own = groups[index].querySelector("tr")!;
    expect(own.querySelector("th")!.getAttribute("role")).toBe("rowheader");
    expect(own.querySelector("th")!.getAttribute("scope")).toBe("rowgroup");
    expect(own.querySelector("th")!.getAttribute("colspan")).toBeNull();
    expect(own.querySelectorAll("td")[0].textContent).toContain(result);
  }
  expect(groups[1].querySelector("tr button")!.getAttribute("aria-label")).toBe(`${isolate("Broken")} failed`);
  expect(groups[1].textContent).toContain("Its console could not be created.");
  expect(groups[1].querySelectorAll("tr")).toHaveLength(2);
  expect(groups[2].querySelectorAll("tr")).toHaveLength(1);
});

it("does not say nothing failed beside a failure to apply the settings", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [project(1)] }];
  const settings_error = { code: "path_not_found", params: { path: "/Users/dev/missing" }, message: "no" };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles, settings_error }} /></DaemonProvider>));
  expect(container.textContent).toContain("The application settings could not be applied");
  expect(container.textContent).not.toContain("Nothing failed");
  // The failure's literal takes the danger tone of the line it is in.
  expect(container.querySelector("code")?.getAttribute("data-color")).toBe("danger");
});

it("puts no space between the sentences of the summary in a language that has none after its full stop", () => {
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
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles }} /></DaemonProvider>));
  expect(container.textContent).toContain("已创建 1 个控制台。已创建 1 个项目。没有失败项。");
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
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles, settings_error }} /></DaemonProvider>));
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(0);
  act(() => vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(container.querySelectorAll('[role="alert"]')).toHaveLength(0);
  act(() => vi.advanceTimersByTime(1));
  const alerts = container.querySelectorAll('[role="alert"]');
  expect(alerts).toHaveLength(1);
  expect(alerts[0].textContent).toContain("The application settings could not be applied");
  expect(alerts[0].textContent).not.toContain("/Users/dev/missing is");
});

it("opens no error alert for partial import failures: the result screen already shows them", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const result = {
    type: "configuration_imported" as const,
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
  expect(container.textContent).toContain("1 item failed");
  expect(container.querySelector("table button")).not.toBeNull();
  // The reason is the application's own sentence; what the system said is behind Details.
  expect(container.textContent).toContain("Something unexpected went wrong.");
  expect(container.textContent).not.toContain("Settings unavailable");
  expect(openDetails(container.querySelector('[data-slot="alert-root"]')!).textContent).toContain("Settings unavailable");
});

/** The lines of the settings card, by setting name. */
function settingLines(container: Element): ReturnType<typeof settingLinesOf> {
  return settingLinesOf(container.querySelector("section[aria-labelledby] > ul")!);
}

it("tells of each application setting that was applied, and of none the file did not carry", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (settings_applied: { auto_pull_repositories: boolean; auto_delete_unresumable_archives?: boolean; auto_select_account?: boolean; auto_approve_session_requests?: boolean; auto_approve_resume_requests?: boolean; auto_approve_account_switches?: boolean; auto_approve_handoffs?: boolean; ask_questions_as_choices?: boolean; reply_in_interface_language?: boolean }) =>
    act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles: [], settings_applied }} /></DaemonProvider>));
  render({ auto_pull_repositories: true, auto_delete_unresumable_archives: false, auto_select_account: false });
  expect(container.querySelector("section > h3")?.textContent).toBe("Application settings applied");
  expect(settingLines(container)).toEqual({
    "Automatically pull repositories": { badge: "On" },
    "Automatically delete archived sessions whose conversation is gone": { badge: "Off" },
    "Automatically select an account": { badge: "Off" },
  });
  render({ auto_pull_repositories: true, auto_approve_session_requests: true, auto_approve_resume_requests: false });
  expect(settingLines(container)).toMatchObject({
    "Automatically approve session requests": { badge: "On" },
    "Automatically approve resume requests": { badge: "Off" },
  });
  render({ auto_pull_repositories: true, auto_approve_account_switches: true, auto_approve_handoffs: false });
  expect(settingLines(container)).toMatchObject({
    "Automatically approve account switches": { badge: "On" },
    "Automatically approve handoffs": { badge: "Off" },
  });
  render({ auto_pull_repositories: true, ask_questions_as_choices: false });
  expect(settingLines(container)).toMatchObject({ "Ask questions as choices": { badge: "Off" } });
  render({ auto_pull_repositories: true, reply_in_interface_language: false });
  expect(settingLines(container)).toMatchObject({ "Talk to me in the interface language": { badge: "Off" } });
  render({ auto_pull_repositories: true });
  expect(settingLines(container)).toEqual({ "Automatically pull repositories": { badge: "On" } });
});

it("tells of the worktree settings when they were applied, and not otherwise", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (settings_applied: ImportedSettings) =>
    act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles: [], settings_applied }} /></DaemonProvider>));
  render({ auto_pull_repositories: true, worktrees: { use_worktrees: true, rebase_before_merge: true, delivery: "ask", revert_gate: true } });
  expect(settingLines(container)["Worktrees"]).toEqual({ badge: "Matches the file" });
  render({ auto_pull_repositories: true });
  expect(container.textContent).not.toContain("Worktrees");
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
    act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles: [], settings_applied: { auto_pull_repositories: true } }} shortcuts={shortcuts} /></DaemonProvider>));
  render({ applied: 2, skipped: 0 });
  expect(settingLines(container)["Keyboard shortcuts"]).toEqual({ badge: "2 changes from the defaults" });
  render({ applied: 1, skipped: 2 });
  expect(settingLines(container)["Keyboard shortcuts"]).toEqual({ badge: "1 change from the default", note: "2 shortcuts in the file could not be applied here and were left out." });
  render({ applied: 0, skipped: 0 });
  expect(settingLines(container)["Keyboard shortcuts"]).toEqual({ badge: "Defaults" });
});

it("places the settings card after the table and the fallbacks", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const consoles = [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [{ agent: "claude" as const, name: "Work" }], projects: [project(1)] }];
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles, settings_applied: { auto_pull_repositories: true } }} /></DaemonProvider>));
  const sectionTitled = (title: string) => [...container.querySelectorAll("section")].find((section) => section.querySelector(":scope > h3")?.textContent === title)!;
  const order = [container.querySelector("table")!, sectionTitled("Accounts that fell back to the default account"), sectionTitled("Application settings applied")];
  const headings = [...container.querySelectorAll("h3")].map((h) => h.textContent);
  expect(headings).toEqual(["Accounts that fell back to the default account", "Application settings applied"]);
  expect(order[0].compareDocumentPosition(order[1]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(order[1].compareDocumentPosition(order[2]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("marks a failed line with the glyph and the word Failed, named after what failed, whose popover holds the reason, and writes no error text into the table", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const result = {
    type: "configuration_imported" as const,
    consoles: [{ name: "C", status: "created" as const, console: "c-1", error: null, account_fallbacks: [], projects: [{ ...project(1), status: "failed" as const, error: { code: "path_not_found", message: "Directory unavailable", params: { path: "/missing" } } }] }],
  };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={result} /></DaemonProvider>));
  const table = container.querySelector("table")!;
  const mark = table.querySelector("button")!;
  // Each failed row's mark names what failed, so several of them can be told apart.
  expect(mark.getAttribute("aria-label")).toBe(`${isolate("P1")} failed`);
  // The mark reads like the other outcomes: the glyph, then the word.
  expect(mark.textContent).toBe("Failed");
  expect(mark.querySelector("svg")).not.toBeNull();
  expect(table.textContent).not.toContain("/missing");
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) mark.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
  });
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("/missing");
});
