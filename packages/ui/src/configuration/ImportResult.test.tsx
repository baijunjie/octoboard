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
import type { ImportedProject } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ImportResult } from "./ImportResult";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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
it("keeps a label in front of every value of a row, for when the rows are stacked, and a header for each column", () => {
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
  expect(headers).toEqual(["Console", "Project", "Result", "Detail"]);
  // Each value is found by what it is and must carry the name of the column it belongs to: the
  // labels and the headers are written from one list, so only a known pair can show them swapped.
  const labelOf = (value: string) =>
    [...container.querySelectorAll("tbody td")].find((td) => td.textContent?.endsWith(value))?.querySelector("span[aria-hidden=true]")
      ?.textContent;
  expect(labelOf("C")).toBe("Console");
  expect(labelOf("P1")).toBe("Project");
  expect(labelOf("Created")).toBe("Result");
  expect(labelOf("At its exported path")).toBe("Detail");
  expect(container.querySelector("table")?.getAttribute("role")).toBe("table");
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

it("tells of each application setting that was applied, and of none the file did not carry", () => {
  const daemon = { store: createStateStore({}) } as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (settings_applied: { auto_pull_repositories: boolean; auto_delete_unresumable_archives?: boolean; auto_select_account?: boolean; auto_approve_session_requests?: boolean; auto_approve_resume_requests?: boolean; auto_approve_account_switches?: boolean; auto_approve_handoffs?: boolean }) =>
    act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles: [], settings_applied }} /></DaemonProvider>));
  render({ auto_pull_repositories: true, auto_delete_unresumable_archives: false, auto_select_account: false });
  expect(container.textContent).toContain("Automatically pull repositories is now on");
  expect(container.textContent).toContain("Automatically delete archived sessions whose conversation is gone is now off");
  expect(container.textContent).toContain("Automatically select an account is now off");
  expect(container.textContent).not.toContain("Automatically approve");
  render({ auto_pull_repositories: true, auto_approve_session_requests: true, auto_approve_resume_requests: false });
  expect(container.textContent).toContain("Automatically approve session requests is now on");
  expect(container.textContent).toContain("Automatically approve resume requests is now off");
  render({ auto_pull_repositories: true, auto_approve_account_switches: true, auto_approve_handoffs: false });
  expect(container.textContent).toContain("Automatically approve account switches is now on");
  expect(container.textContent).toContain("Automatically approve handoffs is now off");
  render({ auto_pull_repositories: true });
  expect(container.textContent).toContain("Automatically pull repositories is now on");
  expect(container.textContent).not.toContain("Automatically approve");
  expect(container.textContent).not.toContain("archived sessions");
  expect(container.textContent).not.toContain("Automatically select an account");
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
