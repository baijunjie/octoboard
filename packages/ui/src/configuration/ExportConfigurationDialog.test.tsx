// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, vi } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { SavedConfigurationFile } from "../platform";
import type { ConfigurationDocument, Event, RequestBody } from "../protocol";
import { ExportConfigurationDialog, exportFileName } from "./ExportConfigurationDialog";
import { mountDialog, press } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-main", "Main");
const DOCUMENT: ConfigurationDocument = { format: "octoboard-configuration", version: 1, consoles: [] };
const exported = (_body: RequestBody): Promise<Event> => Promise.resolve({ type: "configuration_exported", document: DOCUMENT });

type Save = (name: string, text: string) => Promise<SavedConfigurationFile | undefined>;

const mount = (save = vi.fn<Save>(async (name) => ({ kind: "saved", name })), inHand: typeof main | null = main) => {
  const mounted = mountDialog(
    (onClose) => <ExportConfigurationDialog console={inHand ?? undefined} onClose={onClose} />,
    exported,
    { save, open: vi.fn() },
  );
  return { ...mounted, save };
};

const text = () => document.body.textContent ?? "";

it("opens with focus on the Export button, since it has no text field", () => {
  act(() => setInteractionModality("pointer"));
  mount();
  expect(document.activeElement?.textContent).toBe("Export");
});

it("states that the file carries project paths, and whose, while the scope is still being chosen, before any request is made", () => {
  const { sent } = mount();
  expect(text()).toContain("The file contains project paths");
  expect(text()).toContain("the machine running Octoboard, which may not be the one this window is on");
  expect(sent()).toEqual([]);
});

it.each([
  { scope: undefined, sent: { type: "export_configuration", console: "c-main" } },
  { scope: "Everything", sent: { type: "export_configuration" } },
])("asks for the document of the scope chosen: $scope", async ({ scope, sent }) => {
  const mounted = mount();
  if (scope) {
    const radio = Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=radio]")).find(
      (input) => input.labels?.[0]?.textContent === scope,
    );
    await act(async () => radio?.click());
  }
  await press("Export");
  expect(mounted.sent()).toEqual([sent]);
});

it("offers only everything where there is no console in hand", () => {
  mount(undefined, null);
  expect(document.body.querySelectorAll("input[type=radio]")).toHaveLength(0);
});

it("saves the document as the daemon sent it, under a name for the console, and says where it went", async () => {
  const mounted = mount();
  await press("Export");
  expect(mounted.save).toHaveBeenCalledExactlyOnceWith("octoboard-Main.json", `${JSON.stringify(DOCUMENT, null, 2)}\n`);
  expect(mounted.notices()).toEqual([`Configuration saved as ${isolate("octoboard-Main.json")}.`]);
  expect(mounted.closed()).toBe(1);
});

it("says only that the configuration was downloaded when the platform hands the file over without saying where", async () => {
  const mounted = mount(vi.fn<Save>(async () => ({ kind: "handedOver" })));
  await press("Export");
  expect(mounted.notices()).toEqual(["Configuration downloaded."]);
  expect(mounted.closed()).toBe(1);
});

it("stays open, with no notice, when the save dialog is dismissed", async () => {
  const mounted = mount(vi.fn<Save>(async () => undefined));
  await press("Export");
  expect(mounted.notices()).toEqual([]);
  expect(mounted.closed()).toBe(0);
});

it("shows why a file could not be written and stays open", async () => {
  const mounted = mount(vi.fn<Save>(async () => Promise.reject(new Error("Permission denied"))));
  await press("Export");
  expect(text().replace(/[\u2068\u2069]/g, "")).toContain("Could not save the file: Permission denied");
  expect(mounted.closed()).toBe(0);
});

it("names the file for the application, or for the console with what a file name cannot hold taken out", () => {
  expect(exportFileName()).toBe("octoboard-configuration.json");
  expect(exportFileName({ name: "Work / Home: 2\u0007" })).toBe("octoboard-Work Home 2.json");
});
