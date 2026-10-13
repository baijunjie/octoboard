// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

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
  const settings_error = { code: "settings_failed", params: {}, message: "no" };
  act(() => root.render(<DaemonProvider value={daemon}><ImportResult result={{ type: "configuration_imported", consoles, settings_error }} /></DaemonProvider>));
  expect(container.textContent).toContain("The application settings could not be applied");
  expect(container.textContent).not.toContain("Nothing failed");
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
