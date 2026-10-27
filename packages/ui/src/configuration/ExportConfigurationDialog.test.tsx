// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { consoleOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { SavedConfigurationFile } from "../platform";
import type { ConfigurationDocument, Event, RequestBody } from "../protocol";
import { shortcutOverrides } from "../shortcuts/store";
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

const pathsBox = () => document.body.querySelector<HTMLInputElement>("input[type=checkbox]")!;

it("leaves full project paths out by default, with no warning about them, before any request is made", () => {
  const { sent } = mount();
  expect(pathsBox().checked).toBe(false);
  expect(pathsBox().labels?.[0]?.textContent).toBe("Include full project paths");
  expect(text()).toContain("Once checked, the file records each project's full directory path");
  expect(text()).not.toContain("The file contains project paths");
  expect(document.body.querySelector("[data-slot=alert-root]")).toBeNull();
  expect(sent()).toEqual([]);
});

const hint = () => document.getElementById(pathsBox().getAttribute("aria-describedby")!.split(" ")[0]!)!;

it("warns under the option, while it is checked, that the file carries project paths and whose", async () => {
  mount();
  await act(async () => pathsBox().click());
  const alert = document.body.querySelector<HTMLElement>("[data-slot=alert-root]")!;
  expect(alert).not.toBeNull();
  expect(alert.textContent).toContain("The file contains project paths");
  expect(alert.textContent).toContain("the machine running Octoboard, which may not be the device you are using now");
  expect(alert.textContent).toContain("Share the file only with people you trust.");
  // A warning about the choice, not about state: no mark beside the option, and the checkbox is named by its label
  // alone.
  expect(document.body.querySelector("button[data-status-mark]")).toBeNull();
  expect(pathsBox().labels?.[0]?.textContent).toBe("Include full project paths");
  // Under the option: after its note.
  expect(hint().compareDocumentPosition(alert) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  await act(async () => pathsBox().click());
  expect(document.body.querySelector("[data-slot=alert-root]")).toBeNull();
});

it("reaches a screen-reader user: described by the checkbox while checked, and spoken once after the tick", async () => {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  mount();
  const box = pathsBox();
  expect(box.getAttribute("aria-describedby")!.split(" ")).toHaveLength(1);
  await act(async () => box.click());
  expect(pathsBox()).toBe(box);
  const alert = document.body.querySelector<HTMLElement>("[data-slot=alert-root]")!.parentElement!;
  expect(box.getAttribute("aria-describedby")!.split(" ")).toEqual([hint().id, alert.id]);
  // Held back while the checkbox's own new state is spoken.
  expect(alert.querySelector("[role=alert]")).toBeNull();
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  const spoken = alert.querySelector("[role=alert]")!;
  expect(spoken.textContent).toContain("The file contains project paths.");
  expect(spoken.textContent).toContain("which may not be the device you are using now");
  expect(spoken.textContent).not.toContain("Share the file only with people you trust.");
  await act(async () => box.click());
  expect(box.getAttribute("aria-describedby")!.split(" ")).toEqual([hint().id]);
});

it("describes the option by its state, the note outside the checkbox's label", async () => {
  mount();
  expect(hint().textContent).toContain("Once checked, the file records each project's full directory path");
  expect(pathsBox().closest("label")!.contains(hint())).toBe(false);
  await act(async () => pathsBox().click());
  expect(hint().textContent).toBe("An import first looks for each project at its recorded full path.");
});

it("asks for the paths only when the box is checked", async () => {
  const mounted = mount();
  await act(async () => pathsBox().click());
  await press("Export");
  expect(mounted.sent()).toEqual([{ type: "export_configuration", console: "c-main", include_paths: true }]);
});

it.each([
  { scope: undefined, sent: { type: "export_configuration", console: "c-main", include_paths: false } },
  { scope: "Everything", sent: { type: "export_configuration", include_paths: false } },
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

it("adds this client's changed keyboard shortcuts to a full export, and to nothing else", async () => {
  const lock = { code: "KeyK", primary: true, shift: true };
  shortcutOverrides.set({ lock });
  onTestFinished(() => shortcutOverrides.set({}));
  const full: ConfigurationDocument = { ...DOCUMENT, settings: { auto_pull_repositories: true } };
  const save = vi.fn<Save>(async (name) => ({ kind: "saved", name }));
  mountDialog((onClose) => <ExportConfigurationDialog onClose={onClose} />, () => Promise.resolve({ type: "configuration_exported", document: full }), { save, open: vi.fn() });
  await press("Export");
  expect(JSON.parse(save.mock.calls[0][1]).settings).toEqual({ auto_pull_repositories: true, keyboard_shortcuts: { lock } });
});

it("leaves a single console's export as the daemon sent it, whatever shortcuts this client has changed", async () => {
  shortcutOverrides.set({ lock: { code: "KeyK", primary: true, shift: true } });
  onTestFinished(() => shortcutOverrides.set({}));
  const mounted = mount();
  await press("Export");
  expect(mounted.save).toHaveBeenCalledExactlyOnceWith("octoboard-Main.json", `${JSON.stringify(DOCUMENT, null, 2)}\n`);
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

it("shows why a file could not be written in its own words, the system's text behind Error details, and stays open", async () => {
  const mounted = mount(vi.fn<Save>(async () => Promise.reject(new Error("Permission denied (os error 13)"))));
  await press("Export");
  const alert = document.querySelector('[role="alertdialog"]')!;
  // The alert is titled for what failed, not the generic "Error".
  expect(alert.querySelector("h2")?.textContent).toBe("Could not export the configuration");
  const details = alert.querySelector("details")!;
  expect(details.querySelector("summary")?.textContent).toBe("Error details");
  expect(details.querySelector("pre")?.textContent).toBe("Permission denied (os error 13)");
  const explanation = alert.cloneNode(true) as Element;
  explanation.querySelector("details")!.remove();
  expect(explanation.textContent).toContain("Could not save the file.");
  expect(explanation.textContent).not.toContain("Permission denied");
  // The remedy is still the one the system's error names.
  expect(explanation.textContent).toContain("Grant Octoboard access");
  expect(mounted.closed()).toBe(0);
});

it("names the file for the application, or for the console with what a file name cannot hold taken out", () => {
  expect(exportFileName()).toBe("octoboard-configuration.json");
  expect(exportFileName({ name: "Work / Home: 2\u0007" })).toBe("octoboard-Work Home 2.json");
});
