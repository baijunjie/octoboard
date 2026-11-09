// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { consoleOf } from "../fixtures/builders";
import { isolate } from "../i18n/isolate";
import { setLanguage } from "../i18n/language";
import { keepAwake, keepAwakeWhen } from "../lifecycle/keepAwake";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import type { SavedConfigurationFile } from "../platform";
import type { ConfigurationDocument, Event, ClientRequestBody } from "../protocol";
import { shortcutOverrides } from "../shortcuts/store";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { ExportConfigurationDialog, exportFileName } from "./ExportConfigurationDialog";
import { documentSettings, mountDialog, press } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-main", "Main");
const DOCUMENT: ConfigurationDocument = { format: "octoboard-configuration", version: 1, exported_by: null, settings: null, consoles: [] };
const exported = (_body: ClientRequestBody): Promise<Event> => Promise.resolve({ type: "configuration_exported", id: null, document: DOCUMENT });

type Save = (name: string, text: string) => Promise<SavedConfigurationFile | undefined>;

const mount = (save = vi.fn<Save>(async (name) => ({ kind: "saved", name })), inHand: typeof main | null = main, capabilities: Parameters<typeof mountDialog>[4] = {}) => {
  const mounted = mountDialog(
    (onClose) => <ExportConfigurationDialog console={inHand ?? undefined} onClose={onClose} />,
    exported,
    { save, open: vi.fn() },
    {},
    capabilities,
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

it("warns under the option, while it is checked, that the file carries project paths and what they may reveal", async () => {
  mount();
  await act(async () => pathsBox().click());
  const alert = document.body.querySelector<HTMLElement>("[data-slot=alert-root]")!;
  expect(alert).not.toBeNull();
  expect(alert.textContent).toContain("The file contains project paths");
  expect(alert.textContent).toContain("Project paths can reveal details such as your user name and folder names.");
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
  expect(spoken.textContent).toContain("Project paths can reveal details such as your user name and folder names.");
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
  expect(mounted.sent()).toEqual([{ type: "export_configuration", console: "c-main", consoles: "one", settings_sections: [], include_accounts: false, include_paths: true }]);
});

it.each([
  { scope: undefined, sent: { type: "export_configuration", console: "c-main", consoles: "one", settings_sections: [], include_accounts: false, include_paths: false } },
  { scope: "All consoles and their projects", sent: { type: "export_configuration", consoles: "all", settings_sections: [], include_accounts: false, include_paths: false } },
  // With no console, something else must be chosen for there to be anything to export.
  { scope: "Do not export consoles", sent: { type: "export_configuration", consoles: "none", settings_sections: ["general"], include_accounts: false } },
])("asks for the document of the scope chosen: $scope", async ({ scope, sent }) => {
  const mounted = mount();
  if (scope) {
    const radio = Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=radio]")).find(
      (input) => input.labels?.[0]?.textContent === scope,
    );
    await act(async () => radio?.click());
  }
  if (sent.consoles === "none") await act(async () => box("General").click());
  await press("Export");
  expect(mounted.sent()).toEqual([sent]);
});

it("offers no single console where there is none in hand", () => {
  mount(undefined, null);
  const labels = Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=radio]")).map((input) => input.labels?.[0]?.textContent);
  expect(labels).toEqual(["All consoles and their projects", "Do not export consoles"]);
  expect(radio("All consoles and their projects").checked).toBe(true);
});

const radio = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=radio]")).find((input) => input.labels?.[0]?.textContent === label)!;
const box = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((input) => input.labels?.[0]?.textContent === label)!;
const exportButton = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Export")!;

const SECTIONS = ["General", "App behavior", "Git", "Agent behavior", "Keyboard shortcuts", "Cleanup"];
const OPTIONS = [...SECTIONS, "Include agent accounts"];

it("offers a checkbox for each Settings section that carries something, named as Settings names it and in Settings order, then one for the Agent accounts, all unchecked", () => {
  mount();
  expect(Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]"), (input) => input.labels?.[0]?.textContent)).toEqual(["Include full project paths", ...OPTIONS]);
  for (const label of OPTIONS) expect(box(label).checked, label).toBe(false);
});

it("notes under each section what it includes", () => {
  mount();
  const note = (label: string) => document.getElementById(box(label).getAttribute("aria-describedby")!)!.textContent;
  expect(note("General")).toBe("Includes appearance, language, Markdown style and sorting.");
  expect(note("Git")).toContain("default clone directory");
  expect(note("Keyboard shortcuts")).toBe("Includes the keyboard shortcuts you have changed.");
  expect(note("Include agent accounts")).toContain("login credentials are not included");
});

it("notes under App behavior only the settings its platform can hold", () => {
  const note = () => document.getElementById(box("App behavior").getAttribute("aria-describedby")!)!.textContent;
  mount();
  expect(note()).toBe("Includes resuming sessions on start and closing idle terminals.");
});

it("notes under App behavior the background run, the host's name for its icon and preventing sleep where its platform has them", () => {
  const statusItem = { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  mount(undefined, main, { backgroundRun: { set: vi.fn(async () => {}) }, statusItem, keepAwake: { set: vi.fn(async () => {}) } });
  expect(document.getElementById(box("App behavior").getAttribute("aria-describedby")!)!.textContent).toBe(
    "Includes resuming sessions on start, running in the background, the notification area icon, preventing sleep and closing idle terminals.",
  );
});

it("sends the sections checked, in Settings order, and the accounts on their own flag", async () => {
  const mounted = mount();
  await act(async () => box("Cleanup").click());
  await act(async () => box("Include agent accounts").click());
  await act(async () => box("General").click());
  await press("Export");
  expect(mounted.sent()).toEqual([{ type: "export_configuration", console: "c-main", consoles: "one", settings_sections: ["general", "cleanup"], include_accounts: true, include_paths: false }]);
});

it("keeps Export unavailable while no console is exported and no section is checked, whichever one it is", async () => {
  mount();
  await act(async () => radio("Do not export consoles").click());
  expect(exportButton().disabled).toBe(true);
  for (const label of OPTIONS) {
    await act(async () => box(label).click());
    expect(exportButton().disabled, label).toBe(false);
    await act(async () => box(label).click());
    expect(exportButton().disabled, label).toBe(true);
  }
});

it("keeps the paths option about project paths alone, whatever the accounts choice, and shows and sends it only while consoles are exported", async () => {
  const mounted = mount();
  const wording = () => hint().textContent;
  const before = wording();
  await act(async () => box("Include agent accounts").click());
  expect(wording()).toBe(before);
  expect(wording()).not.toContain("account");
  await act(async () => pathsBox().click());
  const alert = document.body.querySelector("[data-slot=alert-root]")!.textContent;
  expect(alert).toContain("The file contains project paths");
  await act(async () => box("Include agent accounts").click());
  expect(document.body.querySelector("[data-slot=alert-root]")!.textContent).toBe(alert);

  // Not shown while no console is exported, with or without accounts, and then not sent.
  await act(async () => radio("Do not export consoles").click());
  expect(box("Include full project paths")).toBeUndefined();
  expect(document.body.querySelector("[data-slot=alert-root]")).toBeNull();
  await act(async () => box("Include agent accounts").click());
  expect(box("Include full project paths")).toBeUndefined();
  await press("Export");
  expect(mounted.sent()).toEqual([{ type: "export_configuration", consoles: "none", settings_sections: [], include_accounts: true }]);
});

it("saves the document as the daemon sent it, under a name for the console, and says where it went", async () => {
  const mounted = mount();
  await press("Export");
  expect(mounted.save).toHaveBeenCalledExactlyOnceWith("octoboard-Main.json", `${JSON.stringify(DOCUMENT, null, 2)}\n`);
  expect(mounted.notices()).toEqual([`Configuration saved as ${isolate("octoboard-Main.json")}.`]);
  expect(mounted.closed()).toBe(1);
});

const FOCUS = { code: "KeyK", primary: true, shift: true };

/** Exports a document whose settings are the daemon's own (it holds the language `daemon` reports), from a client with
 * `capabilities` having checked the `sections` (every one by default), and returns the settings that were saved. */
async function savedSettings(daemonLanguage: string | undefined, capabilities: Parameters<typeof mountDialog>[4] = {}, sections: string[] = SECTIONS) {
  const full: ConfigurationDocument = { ...DOCUMENT, settings: documentSettings({ auto_pull_repositories: true, ...(daemonLanguage && { interface_language: daemonLanguage }) }) };
  const save = vi.fn<Save>(async (name) => ({ kind: "saved", name }));
  mountDialog((onClose) => <ExportConfigurationDialog onClose={onClose} />, () => Promise.resolve({ type: "configuration_exported", id: null, document: full }), { save, open: vi.fn() }, {}, capabilities);
  // By position: the labels follow the language the test has set. The first checkbox is the paths option.
  const boxes = Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).slice(1);
  for (const label of sections) await act(async () => boxes[SECTIONS.indexOf(label)].click());
  // By its type: the label follows the language the test has set.
  await act(async () => document.body.querySelector<HTMLButtonElement>("button[type=submit]")!.click());
  return { saved: JSON.parse(save.mock.calls[0][1]).settings as Record<string, unknown>, full };
}

it("adds this client's changed keyboard shortcuts to a full export, and to nothing else", async () => {
  shortcutOverrides.set({ focusMode: FOCUS });
  onTestFinished(() => shortcutOverrides.set({}));
  const { saved, full } = await savedSettings(undefined);
  expect(saved).toMatchObject({ ...full.settings, keyboard_shortcuts: { focusMode: FOCUS } });
});

it("adds the client's settings of the sections checked only, each under its own section", async () => {
  shortcutOverrides.set({ focusMode: FOCUS });
  runInBackground.set(true);
  markdownStyle.set("github");
  onTestFinished(() => {
    shortcutOverrides.set({});
    runInBackground.set(false);
    markdownStyle.set("tailwind");
  });
  const capabilities = { backgroundRun: { set: vi.fn(async () => {}) } };
  const general = (await savedSettings("de", capabilities, ["General"])).saved;
  expect(general).toMatchObject({ markdown_style: "github", interface_language: "de", session_order: { sessions: "status", active_projects: "status", inactive_projects: "name" } });
  expect(general).not.toHaveProperty("run_in_background");
  expect(general).not.toHaveProperty("keyboard_shortcuts");
});

it("adds the app behavior settings of its platform and the keyboard shortcuts only with their sections", async () => {
  shortcutOverrides.set({ focusMode: FOCUS });
  runInBackground.set(true);
  onTestFinished(() => {
    shortcutOverrides.set({});
    runInBackground.set(false);
  });
  const behavior = (await savedSettings(undefined, { backgroundRun: { set: vi.fn(async () => {}) } }, ["App behavior"])).saved;
  expect(behavior.run_in_background).toBe(true);
  expect(behavior).not.toHaveProperty("appearance");
  expect(behavior).not.toHaveProperty("session_order");
  expect(behavior).not.toHaveProperty("keyboard_shortcuts");
});

it("adds the keyboard shortcuts, an empty set where none is changed, with the Keyboard shortcuts section alone", async () => {
  const { saved } = await savedSettings(undefined, {}, ["Keyboard shortcuts"]);
  expect(saved.keyboard_shortcuts).toEqual({});
  expect(saved).not.toHaveProperty("appearance");
});

it("adds the appearance and Markdown style this client has to a full export and leaves the language as the daemon holds it", async () => {
  markdownStyle.set("github");
  setLanguage("ja");
  localStorage.setItem("heroui-theme", "dark");
  onTestFinished(() => {
    markdownStyle.set("tailwind");
    setLanguage("en");
    localStorage.removeItem("heroui-theme");
  });
  const { saved } = await savedSettings("de");
  expect(saved).toMatchObject({ appearance: "dark", markdown_style: "github", interface_language: "de" });
  // A browser has no background running to report.
  expect(saved).not.toHaveProperty("run_in_background");
});

it("adds whether this client runs in the background only where its platform has that", async () => {
  runInBackground.set(true);
  onTestFinished(() => runInBackground.set(false));
  const { saved } = await savedSettings(undefined, { backgroundRun: { set: vi.fn(async () => {}) } });
  expect(saved.run_in_background).toBe(true);
});

it("adds no menu bar icon choice where its platform has no icon", async () => {
  showMenuBarIcon.set(false);
  onTestFinished(() => showMenuBarIcon.set(true));
  expect((await savedSettings(undefined)).saved).not.toHaveProperty("show_menu_bar_icon");
});

it("adds whether this client shows its menu bar icon where its platform has one", async () => {
  showMenuBarIcon.set(false);
  onTestFinished(() => showMenuBarIcon.set(true));
  const statusItem = { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  expect((await savedSettings(undefined, { statusItem })).saved.show_menu_bar_icon).toBe(false);
});

it("adds no Prevent sleep choice where its platform cannot hold the computer awake", async () => {
  const { saved } = await savedSettings(undefined);
  expect(saved).not.toHaveProperty("keep_awake");
  expect(saved).not.toHaveProperty("keep_awake_when");
});

it("adds whether this client prevents sleep, and when, where its platform can", async () => {
  keepAwake.set(true);
  keepAwakeWhen.set("always");
  onTestFinished(() => {
    keepAwake.set(false);
    keepAwakeWhen.set("working");
  });
  const { saved } = await savedSettings(undefined, { keepAwake: { set: vi.fn(async () => {}) } });
  expect(saved).toMatchObject({ keep_awake: true, keep_awake_when: "always" });
});

it("leaves a single console's export as the daemon sent it, whatever this client has changed", async () => {
  shortcutOverrides.set({ focusMode: FOCUS });
  markdownStyle.set("github");
  onTestFinished(() => {
    shortcutOverrides.set({});
    markdownStyle.set("tailwind");
  });
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
