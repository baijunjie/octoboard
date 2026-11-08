// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { DEFAULT_SESSION_ORDER } from "../sessions/order";
import { sessionOrderPreference } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { SessionOrderSettings } from "./SessionOrderSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => sessionOrderPreference.set(DEFAULT_SESSION_ORDER));

function mount() {
  const container = mountHost(
    <LanguageProvider>
      <SessionOrderSettings />
    </LanguageProvider>,
    { daemon: testDaemon({}) },
  );
  return container;
}

function pick(select: HTMLSelectElement, value: string) {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

/** The select of the row labelled `label`. */
function selectOf(container: HTMLElement, label: string): HTMLSelectElement {
  const row = Array.from(container.querySelectorAll<HTMLElement>("div")).find((div) => div.textContent?.startsWith(label) && div.querySelector("select"));
  return row!.querySelector("select")!;
}

it("shows the three rows with the default choices selected", () => {
  const container = mount();
  const rows = ["Sessions", "Active projects", "Inactive projects"].map((label) => selectOf(container, label));
  expect(rows.map((select) => [select.value, Array.from(select.options).map((o) => o.value).filter(Boolean)])).toEqual([
    ["status", ["status", "started"]],
    ["status", ["status", "name", "used"]],
    ["name", ["name", "used"]],
  ]);
});

it("stores each pick in its own part of the order and keeps the others", () => {
  const container = mount();
  pick(selectOf(container, "Sessions"), "started");
  pick(selectOf(container, "Active projects"), "used");
  pick(selectOf(container, "Inactive projects"), "used");
  expect(sessionOrderPreference.get()).toEqual({ sessions: "started", activeProjects: "used", inactiveProjects: "used" });
  expect(selectOf(container, "Inactive projects").value).toBe("used");
});
