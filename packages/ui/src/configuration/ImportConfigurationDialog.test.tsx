// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { MIXED_RESULT, REVIEW } from "../gallery/fixtures/configuration";
import { press as pressElement } from "../components/detailsTestSupport";
import { accessibleName } from "../dialogs/dialogTestSupport";
import { isolate } from "../i18n/isolate";
import { ConfigurationFileError } from "../platform";
import type { Event, GitWorkflowSettings, RequestBody } from "../protocol";
import { bindingFor, shortcutOverrides } from "../shortcuts/store";
import { ImportConfigurationDialog } from "./ImportConfigurationDialog";
import { mountDialog, openTab, press, settingLinesOf } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const DOCUMENT = { format: "octoboard-configuration", version: 1, settings: { auto_pull_repositories: true }, consoles: [{ name: "Work laptop" }] };

/** A daemon that inspects as `REVIEW` and imports as `MIXED_RESULT`, unless a test
 * says otherwise. */
function reply(body: RequestBody): Promise<Event> {
  if (body.type === "inspect_configuration") return Promise.resolve({ type: "configuration_inspected", ...REVIEW });
  if (body.type === "import_configuration") return Promise.resolve({ type: "configuration_imported", ...MIXED_RESULT });
  return Promise.reject(new Error(`unexpected ${body.type}`));
}

function mount(options: { reply?: typeof reply; text?: string | undefined; unanswered?: boolean } = {}) {
  const open = vi.fn(async () => (options.unanswered ? new Promise<never>(() => {}) : options.text === undefined && "text" in options ? undefined : { name: "mine.json", text: options.text ?? JSON.stringify(DOCUMENT) }));
  const mounted = mountDialog(
    (onClose) => <ImportConfigurationDialog onClose={onClose} />,
    options.reply ?? reply,
    { save: vi.fn(), open },
  );
  return { ...mounted, open };
}

const text = () => document.body.textContent ?? "";
const row = (project: string) => Array.from(document.body.querySelectorAll("li")).find((li) => li.textContent?.startsWith(project))!;
/** The labelled path fields of the project's row, by label. */
const fields = (project: string) =>
  Object.fromEntries(Array.from(row(project).querySelectorAll("dl > dt")).map((dt) => [dt.textContent, dt.nextElementSibling?.textContent]));
const consoleCard = (name: string) => Array.from(document.body.querySelectorAll("section")).find((s) => s.querySelector(":scope > div h3")?.textContent === name)!;
/** The checkbox that includes a region of the file in the import. */
const regionBox = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((input) => input.labels?.[0]?.textContent === label)!;
const settingsBox = () => regionBox("Application settings");
const tabNames = () => Array.from(document.body.querySelectorAll("[role=tab]")).map((tab) => tab.textContent);
/** The lines of the settings card of the result, by setting name. */
function resultSettingLines(): ReturnType<typeof settingLinesOf> {
  const card = Array.from(document.body.querySelectorAll("section")).find((s) => s.querySelector(":scope > h3")?.textContent === "Application settings applied")!;
  return settingLinesOf(card.querySelector("ul")!);
}

/** The lines of the settings card of the review, by setting name. */
function settingLines(): ReturnType<typeof settingLinesOf> {
  return settingLinesOf(document.body.querySelector("[role=tabpanel] ul")!);
}

/** Picks `option` in the select of the project's row. */
async function choose(project: string, option: string): Promise<void> {
  await act(async () => row(project).querySelector<HTMLButtonElement>("button[aria-haspopup=listbox]")!.click());
  const item = Array.from(document.body.querySelectorAll<HTMLElement>("[role=option]")).find((o) => o.textContent === option);
  await act(async () => item!.click());
}

function type(input: HTMLInputElement, value: string): void {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function reviewing(options: Parameters<typeof mount>[0] = {}) {
  const mounted = mount(options);
  await press("Choose file");
  return mounted;
}

/** Reviewing with the application settings included, which they are not by default, and their tab open. */
async function reviewingSettings(options: Parameters<typeof mount>[0] = {}) {
  const mounted = await reviewing(options);
  await act(async () => settingsBox().click());
  await openTab("Application settings");
  return mounted;
}

it("opens with focus on the Choose file button, since the first step has no text field", () => {
  act(() => setInteractionModality("pointer"));
  mount();
  expect(document.activeElement?.textContent).toBe("Choose file");
});

it("inspects the document as it was read, and creates nothing", async () => {
  const mounted = await reviewing();
  expect(mounted.sent()).toEqual([{ type: "inspect_configuration", document: DOCUMENT }]);
  expect(text()).toContain("Work laptop");
});

it("groups the projects under a card for their console, with the console's name and how many projects it has", async () => {
  await reviewing();
  expect(consoleCard("Work laptop").querySelector(":scope > div")!.textContent).toContain("4 projects");
  expect(consoleCard("Work laptop").querySelectorAll("li")).toHaveLength(4);
  expect(consoleCard("Side projects").querySelector(":scope > div")!.textContent).toContain("1 project");
  expect(consoleCard("Scratch").textContent).toContain("0 projects");
  expect(consoleCard("Scratch").textContent).toContain("This console has no projects.");
});

/** Opens the warning mark inside `scope` the way a press from assistive technology does, gives the popover's
 * text and closes it again. */
async function fallbackSentences(scope: Element): Promise<string> {
  const mark = scope.querySelector<HTMLElement>("button[data-status-mark]")!;
  pressElement(mark);
  const dialog = document.body.querySelector(`[role="dialog"][aria-label="${mark.getAttribute("aria-label")}"]`)!;
  const sentences = dialog.textContent ?? "";
  await act(async () => void dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  return sentences;
}

it("marks a console and a project whose account is not set up with a warning mark after the name, the sentence in its popover", async () => {
  await reviewing();
  const heading = consoleCard("Work laptop").querySelector("h3")!;
  // No loose sentence beside the name; the mark follows the heading directly.
  expect(consoleCard("Work laptop").querySelector(":scope > div")!.textContent).not.toContain("is not set up");
  expect(heading.nextElementSibling?.firstElementChild?.getAttribute("aria-label")).toBe(`Accounts not set up for ${isolate("Work laptop")}`);
  expect(await fallbackSentences(heading.parentElement!)).toContain("Claude Code account Work is not set up in Octoboard, so the default account is used.");
  expect(row("Search API").textContent).not.toContain("is not set up");
  expect(await fallbackSentences(row("Search API"))).toContain("Codex account Personal is not set up in Octoboard");
  expect(consoleCard("Side projects").querySelector("button[data-status-mark]")).toBeNull();
});

it("shows a row for every project with where it was found and the labelled paths its choice would use", async () => {
  await reviewing();
  // Placed at its exported path, one field says both.
  expect(fields("Website")).toEqual({ "Placed in": "/Users/ana/code/website" });
  expect(fields("Search API")).toEqual({ "Exported path": "/Users/ana/work/search-api", "Placed in": "/Users/dev/Projects/search-api" });
  expect(fields("Design System")).toEqual({
    "Exported path": "/Users/ana/work/design-system",
    "Clone from": "https://github.com/acme/design-system.git",
    "Clone into": "/Users/dev/Projects/design-system",
  });
  // Where each was found is said by its status line, whatever the choice made since.
  expect(row("Website").textContent).toContain("Found at its exported path");
  expect(row("Search API").textContent).toContain("Found in the default clone directory");
  expect(row("Design System").textContent).toContain("Not found; can be cloned from its remote");
  // A project found nowhere has no status line: a warning mark after its name says so. Notes also names an
  // account that is not set up: the same point, so one mark, which says both.
  expect(row("Notes").textContent).not.toContain("Not found on the machine");
  expect(row("Notes").querySelectorAll("button[data-status-mark]")).toHaveLength(1);
  const label = `${isolate("Notes")} not found, and accounts not set up`;
  const mark = row("Notes").querySelector<HTMLButtonElement>(`button[aria-label='${label}']`)!;
  expect(mark).not.toBeNull();
  act(() => mark.click());
  const popover = document.querySelector(`[role=dialog][aria-label='${label}']`)?.textContent;
  expect(popover).toContain("Not found on the machine running Octoboard.");
  expect(popover).toContain("It is skipped unless you choose a directory for it.");
  expect(popover).toContain("Codex account Personal is not set up in Octoboard");
  // Leaving a project out places it nowhere: only the exported path is left to show.
  expect(fields("Notes")).toEqual({ "Exported path": "/Users/ana/Documents/Work/Clients/Northwind/Archive/2019/Quarterly-Reports/final-notes" });
});

it("names only the missing project in its mark when no account of it is missing", async () => {
  const plain = (body: RequestBody): Promise<Event> =>
    body.type === "inspect_configuration"
      ? Promise.resolve({ type: "configuration_inspected", ...REVIEW, consoles: REVIEW.consoles.map((c) => ({ ...c, projects: c.projects.map((p) => ({ ...p, account_fallbacks: [] })) })) })
      : reply(body);
  await reviewing({ reply: plain });
  expect(row("Notes").querySelectorAll("button[data-status-mark]")).toHaveLength(1);
  expect(row("Notes").querySelector(`button[aria-label='${isolate("Notes")} not found']`)).not.toBeNull();
});

it("shows no exported path for a project the export recorded by its directory name alone, and still says where it was found", async () => {
  const bare = (body: RequestBody): Promise<Event> =>
    body.type === "inspect_configuration"
      ? Promise.resolve({ type: "configuration_inspected", ...REVIEW, consoles: REVIEW.consoles.map((c) => ({ ...c, projects: c.projects.map((p) => ({ ...p, path: p.name.toLowerCase().replace(/\s+/g, "-") })) })) })
      : reply(body);
  await reviewing({ reply: bare });
  expect(fields("Search API")).toEqual({ "Placed in": "/Users/dev/Projects/search-api" });
  expect(row("Search API").textContent).toContain("Found in the default clone directory");
});

it("keeps the dialog on the file step, with the daemon's reason, for a document it refuses", async () => {
  const refuse = (body: RequestBody) =>
    body.type === "inspect_configuration"
      ? Promise.reject(new DaemonRequestError("This is not a configuration file.", "configuration_not_recognised", {}))
      : reply(body);
  await reviewing({ reply: refuse });
  const acknowledgement = document.body.querySelector('[role="alertdialog"]')!;
  expect(acknowledgement.textContent).toContain("This is not an Octoboard configuration file.");
  const product = acknowledgement.querySelector("[data-inline-reference]")!;
  expect(product.textContent).toBe("Octoboard");
  expect(product.getAttribute("dir")).toBe("auto");
  expect(product.textContent).not.toMatch(/[\u2066-\u2069]/);
  expect(text()).toContain("Choose an exported configuration file");
});

it("refuses a file that is not JSON without asking the daemon", async () => {
  const mounted = await reviewing({ text: "not json" });
  expect(text()).toContain("mine.json is not a JSON file.");
  expect(document.body.querySelector('[role="alertdialog"] h2')?.textContent).toBe("Could not import the configuration");
  expect(document.body.querySelector('[role="alertdialog"] code')?.textContent).toBe("mine.json");
  expect(mounted.sent()).toEqual([]);
});

it("does nothing when the open dialog is dismissed", async () => {
  const mounted = await reviewing({ text: undefined });
  expect(mounted.sent()).toEqual([]);
  expect(text()).toContain("Choose an exported configuration file");
});

it("changes only the row whose choice was changed", async () => {
  await reviewing();
  await choose("Website", "Default clone directory");
  expect(fields("Website")).toEqual({ "Exported path": "/Users/ana/code/website", "Placed in": "/Users/dev/Projects/website" });
  expect(fields("Search API")["Placed in"]).toBe("/Users/dev/Projects/search-api");
  expect(fields("Design System")["Clone from"]).toBe("https://github.com/acme/design-system.git");
});

it("applies one choice to every project it is available for and leaves the others as they were", async () => {
  await reviewing();
  await choose("Website", "Default clone directory");
  await press("Exported path (1)");
  expect(fields("Website")).toEqual({ "Placed in": "/Users/ana/code/website" });
  // Search API offers no exported path, so it keeps the choice it had.
  expect(fields("Search API")["Placed in"]).toBe("/Users/dev/Projects/search-api");
});

it("sends the document unchanged with a resolution for every project that is placed, in one request, a project left out among them", async () => {
  const mounted = await reviewing();
  await press("Import");
  const imports = mounted.sent().filter((body) => body.type === "import_configuration");
  expect(imports).toEqual([
    {
      type: "import_configuration",
      document: DOCUMENT,
      resolutions: [
        { console: 0, project: 0, choice: { kind: "exported_path" } },
        { console: 0, project: 1, choice: { kind: "default_clone_dir" } },
        { console: 0, project: 2, choice: { kind: "clone" } },
        { console: 1, project: 0, choice: { kind: "clone" } },
      ],
      apply_settings: false,
    },
  ]);
});

it("applies the application settings only once they are included", async () => {
  const mounted = await reviewingSettings();
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: true });
});

const LOCK = { code: "KeyK", primary: true, shift: true };
/** A daemon that, when asked for the settings, applies them and echoes the document's keyboard shortcuts as they came
 * (or as `echo` has it, where given); asked for no settings, it reports none applied. */
const echoing = (settings: Record<string, unknown> = {}) => (body: RequestBody): Promise<Event> => {
  if (body.type !== "import_configuration") return reply(body);
  const carried = (body.document as { settings?: { keyboard_shortcuts?: unknown } }).settings?.keyboard_shortcuts;
  const applied = { ...MIXED_RESULT.settings_applied!, ...(carried !== undefined && carried !== null && { keyboard_shortcuts: carried as never }), ...settings };
  return Promise.resolve({ type: "configuration_imported", ...MIXED_RESULT, settings_applied: body.apply_settings ? applied : undefined });
};
const PIN = { code: "KeyJ", primary: true, shift: true };
const withShortcuts = (shortcuts: unknown) => JSON.stringify({ ...DOCUMENT, settings: { ...DOCUMENT.settings, keyboard_shortcuts: shortcuts } });

it("says nothing of keyboard shortcuts when the document carries none", async () => {
  await reviewingSettings();
  expect(text()).not.toContain("Keyboard shortcuts");
});

it("says what the document's keyboard shortcuts would do on this device", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewingSettings({ text: withShortcuts({ lock: LOCK, bad: 1 }) });
  expect(settingLines()["Keyboard shortcuts"]).toEqual({ badge: "Set to the file's", note: "1 of the file's shortcuts differs from the default. They apply to this device or browser only." });
});

it("replaces this device's keyboard shortcuts with the document's once the settings are applied, and reports it", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ pin: { code: "KeyJ", primary: true, shift: true } });
  await reviewingSettings({ text: withShortcuts({ lock: LOCK, bad: 1 }), reply: echoing() });
  await press("Import");
  expect(bindingFor("lock")).toEqual(LOCK);
  expect(bindingFor("pin")).toEqual({ code: "KeyP", primary: true, shift: true });
  await openTab("Application settings");
  const applied = resultSettingLines();
  expect(applied["Keyboard shortcuts"]).toEqual({ badge: "1 change from the default", note: "1 shortcut in the file could not be applied here and was left out." });
});

it("leaves this device's keyboard shortcuts alone unless the settings are included", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewing({ text: withShortcuts({ lock: LOCK }), reply: echoing() });
  await press("Import");
  expect(bindingFor("lock")).not.toEqual(LOCK);
  expect(text()).not.toContain("Keyboard shortcuts");
});

async function importing(shortcuts: unknown, settings?: Record<string, unknown>) {
  shortcutOverrides.set({ pin: PIN });
  await reviewingSettings({ text: withShortcuts(shortcuts), reply: echoing(settings) });
  await press("Import");
}

it("puts every shortcut back to its default when the document carries an empty set, and says so", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ pin: PIN });
  await reviewingSettings({ text: withShortcuts({}), reply: echoing() });
  expect(settingLines()["Keyboard shortcuts"]).toEqual({ badge: "Back to defaults", note: "They apply to this device or browser only." });
  expect(text()).not.toContain("0 of them");
  await press("Import");
  expect(bindingFor("pin")).toEqual({ code: "KeyP", primary: true, shift: true });
  await openTab("Application settings");
  expect(resultSettingLines()["Keyboard shortcuts"]).toEqual({ badge: "Defaults" });
  expect(text()).not.toContain("0 changes");
});

it("leaves this device's keyboard shortcuts alone when the settings could not be applied", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  const failing = (body: RequestBody): Promise<Event> =>
    body.type === "import_configuration"
      ? Promise.resolve({ type: "configuration_imported", ...MIXED_RESULT, settings_applied: null, settings_error: { code: "import_settings_missing", params: {}, message: "no settings" } as never })
      : reply(body);
  shortcutOverrides.set({ pin: PIN });
  await reviewingSettings({ text: withShortcuts({ lock: LOCK }), reply: failing });
  await act(async () => settingsBox().click());
  await press("Import");
  expect(bindingFor("pin")).toEqual(PIN);
  expect(bindingFor("lock")).not.toEqual(LOCK);
});

it.each([[[LOCK]], ["lock"], [null], [7]])("ignores a keyboard_shortcuts echo that is not an object (%j)", async (echo) => {
  onTestFinished(() => shortcutOverrides.set({}));
  await importing({ lock: LOCK }, { keyboard_shortcuts: echo });
  expect(bindingFor("pin")).toEqual(PIN);
  expect(bindingFor("lock")).not.toEqual(LOCK);
});

it("leaves this device's keyboard shortcuts alone when the document carries none, whatever else is applied", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ pin: { code: "KeyJ", primary: true, shift: true } });
  await reviewingSettings();
  await press("Import");
  expect(bindingFor("pin")).toEqual({ code: "KeyJ", primary: true, shift: true });
});

it("offers the regions of the file as checkboxes above the tabs, the settings unchecked, and a tab only for a checked region", async () => {
  await reviewing();
  expect(regionBox("Consoles and projects").checked).toBe(true);
  expect(settingsBox().checked).toBe(false);
  // One checked region is one panel, without a tab bar.
  expect(tabNames()).toEqual([]);
  await act(async () => settingsBox().click());
  expect(tabNames()).toEqual(["Consoles and projects", "Application settings"]);
  await act(async () => regionBox("Consoles and projects").click());
  expect(tabNames()).toEqual([]);
  expect(text()).toContain("Automatically pull repositories");
  expect(text()).not.toContain("Set every project that offers it to");
  expect(document.body.textContent).not.toContain("Also apply the application settings");
});

it("keeps Import unavailable while no region is checked", async () => {
  await reviewing();
  const importButton = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Import")!;
  await act(async () => regionBox("Consoles and projects").click());
  expect(importButton().disabled).toBe(true);
  expect(text()).not.toContain("Set every project that offers it to");
  await act(async () => regionBox("Consoles and projects").click());
  expect(importButton().disabled).toBe(false);
});

it("shows no checkbox for a file that carries one region, and imports that region", async () => {
  const only = { ...REVIEW, consoles: [], accounts: undefined };
  const mounted = await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...only }) : reply(body)) });
  expect(document.body.querySelectorAll("input[type=checkbox]")).toHaveLength(0);
  expect(text()).toContain("Automatically pull repositories");
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: true, resolutions: [] });
  expect(mounted.sent().at(-1)).not.toHaveProperty("apply_consoles");
});

it("imports without the consoles when their region is unchecked: none is asked for and no resolution is sent", async () => {
  const mounted = await reviewing();
  await act(async () => settingsBox().click());
  await act(async () => regionBox("Consoles and projects").click());
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", resolutions: [], apply_settings: true, apply_consoles: false });
});

it("notes what applying each application setting would change with a badge, and what follows only for one it turns on", async () => {
  await reviewingSettings();
  const lines = settingLines();
  expect(lines["Automatically pull repositories"]).toEqual({ badge: "Turns on", note: "Octoboard then pulls your repositories by itself." });
  // Turning a setting off, or leaving it as it is, has no consequence line.
  expect(lines["Automatically delete archived sessions whose conversation is gone"]).toEqual({ badge: "Turns off" });
  expect(lines["Automatically select an account"]).toEqual({ badge: "Turns on", note: "A new session then uses the account whose weekly limit resets soonest." });
  expect(lines["Order the sidebar by status"]).toEqual({ badge: "Turns off" });
  expect(lines["Start usage windows automatically"]?.badge).toBe("Turns on");
  expect(lines["Start usage windows automatically"]?.note).toContain("counts a little toward the account's usage limits");
});

it("says a setting the file shares with this machine changes nothing", async () => {
  const same = { ...REVIEW, settings: { ...REVIEW.settings!, current_auto_pull_repositories: true } };
  await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...same }) : reply(body)) });
  expect(settingLines()["Automatically pull repositories"]).toEqual({ badge: "No change" });
});

it("states what turning on the automatic approvals lets agents do", async () => {
  await reviewingSettings();
  const lines = settingLines();
  expect(lines["Automatically approve session requests"]).toEqual({ badge: "Turns on", note: "A session's request to start a session that reports to you is then approved without asking you first." });
  expect(lines["Automatically approve resume requests"]?.badge).toBe("Turns on");
  expect(lines["Automatically approve resume requests"]?.note).toContain("request to reopen or resume a session");
  expect(lines["Automatically approve account switches"]).toEqual({ badge: "Turns on", note: "A session's request to switch the account of one of its sessions is then approved without asking you first." });
  expect(lines["Automatically approve handoffs"]).toEqual({ badge: "Turns on", note: "A session's request to hand one of its sessions off to another agent is then approved without asking you first." });
});

it("says applying the file turns off asking questions as choices", async () => {
  await reviewingSettings();
  expect(settingLines()["Ask questions as choices"]).toEqual({ badge: "Turns off" });
});

it("states what turning on asking questions as choices does", async () => {
  const turningOn = { ...REVIEW, settings: { ...REVIEW.settings!, ask_questions_as_choices: true, current_ask_questions_as_choices: false } };
  await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...turningOn }) : reply(body)) });
  expect(settingLines()["Ask questions as choices"]).toEqual({ badge: "Turns on", note: "Octoboard asks sessions started afterwards to put a question that needs your decision as options you can pick." });
});

it("leaves out the automatic approvals when the file does not carry them", async () => {
  const without = {
    ...REVIEW,
    settings: {
      ...REVIEW.settings!,
      auto_approve_session_requests: null,
      auto_approve_resume_requests: null,
      auto_approve_account_switches: null,
      auto_approve_handoffs: null,
    },
  };
  await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...without }) : reply(body)) });
  expect(text()).not.toContain("Automatically approve");
});

it("leaves out the setting a file from before it existed does not carry", async () => {
  const older = { ...REVIEW, settings: { ...REVIEW.settings!, auto_delete_unresumable_archives: null, auto_select_account: null } };
  await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...older }) : reply(body)) });
  expect(settingLines()["Automatically pull repositories"]?.badge).toBe("Turns on");
  expect(text()).not.toContain("archived sessions whose conversation is gone");
  expect(text()).not.toContain("Automatically select an account");
});

describe("the git workflow settings", () => {
  const FILE: GitWorkflowSettings = { development: "in_place", squash_before_delivery: false, rebase_before_merge: true, delivery: "merge", revert_gate: true };
  const inspecting = (git_workflow: Omit<typeof FILE, "squash_before_delivery"> & { squash_before_delivery?: boolean | null } | undefined, current: typeof FILE | undefined) => {
    const { git_workflow: _, current_git_workflow: __, ...rest } = REVIEW.settings!;
    const settings = { ...rest, ...(git_workflow && { git_workflow }), ...(current && { current_git_workflow: current }) };
    return (body: RequestBody) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected" as const, ...REVIEW, settings }) : reply(body));
  };

  it("notes that applying changes them when the file's differ from this machine's", async () => {
    await reviewingSettings({ reply: inspecting(FILE, { ...FILE, development: "worktree" }) });
    expect(settingLines()["Git workflow"]).toEqual({ badge: "Set to the file's" });
  });

  it("notes that applying changes nothing when they are the same", async () => {
    await reviewingSettings({ reply: inspecting(FILE, FILE) });
    expect(settingLines()["Git workflow"]).toEqual({ badge: "No change" });
  });

  it("keeps the current squash value when an older workflow omits it", async () => {
    const { squash_before_delivery: _, ...older } = FILE;
    await reviewingSettings({ reply: inspecting(older, { ...FILE, squash_before_delivery: true }) });
    expect(settingLines()["Git workflow"]).toEqual({ badge: "No change" });
  });

  it("leaves them out when the file does not carry them", async () => {
    await reviewingSettings({ reply: inspecting(undefined, FILE) });
    expect(text()).not.toContain("Git workflow");
  });
});

it("stops before any request when a project is to go to a directory that is missing or not a path", async () => {
  const mounted = await reviewing();
  await choose("Notes", "Another directory");
  await press("Import");
  expect(text()).toContain("Enter a directory, or leave the project out.");
  // The field shows "Directory" as its label while its name, for assistive technology, says which project it is for.
  const input = row("Notes").querySelector("input")!;
  expect(accessibleName(input)).toBe(`Directory for ${isolate("Notes")}`);
  expect(Array.from(row("Notes").querySelectorAll("label")).map((label) => label.querySelector("[aria-hidden]")?.textContent)).toContain("Directory");
  type(input, "relative/dir");
  expect(text()).toContain("Enter an absolute path, or one starting with ~/.");
  expect(mounted.sent().filter((body) => body.type === "import_configuration")).toEqual([]);
  type(row("Notes").querySelector("input")!, "~/notes");
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({
    resolutions: expect.arrayContaining([{ console: 0, project: 3, choice: { kind: "directory", path: "~/notes" } }]),
  });
});

it("tells each project's result in a row of its console's card, writes a failure's reason under the row, and marks the accounts that fell back after the names", async () => {
  await reviewing();
  await press("Import");
  const cards = Array.from(document.body.querySelectorAll("section:not([data-slot=modal-dialog])")).filter((section) => section.querySelector("h3"));
  expect(cards.map((card) => card.querySelector("h3")!.textContent)).toEqual(["Work laptop", "Side projects", "Scratch"]);
  expect(document.body.querySelector("table")).toBeNull();
  // The console's own outcome is always in its header, and its name is said once.
  const resultCard = (name: string) => cards.find((card) => card.querySelector("h3")!.textContent === name)!;
  const header = (name: string) => resultCard(name).querySelector(":scope > div")!;
  expect(header("Work laptop").textContent).toContain("Created");
  expect(header("Scratch").textContent).toContain("Created");
  expect(header("Side projects").textContent).toContain("Failed");
  expect(resultCard("Work laptop").textContent!.split("Work laptop")).toHaveLength(2);
  expect(resultCard("Work laptop").contains(row("Search API"))).toBe(true);
  expect(row("Search API").textContent).toContain("Created");
  expect(row("Search API").textContent).toContain("In the default clone directory");
  // A failure is the glyph and the word, and its reason is written under the row, not behind a mark.
  expect(row("Website").textContent).toContain("Failed");
  expect(row("Website").textContent).toContain("/Users/ana/code/website does not exist.");
  expect(row("Website").querySelector("button[data-status-mark]")).toBeNull();
  expect(row("Notes").textContent).toContain("Left out");
  // The raw text is behind the row's Details button, in a diagnostic block, never in the row.
  const failedRow = row("Design System");
  expect(failedRow.textContent).toContain("Git could not clone this repository.");
  expect(failedRow.querySelector("pre")).toBeNull();
  const failure = MIXED_RESULT.consoles.flatMap((console) => console.projects).find((project) => project.name === "Design System")!.error!;
  expect(failedRow.textContent).not.toContain(failure.params.detail);
  pressElement(Array.from(failedRow.querySelectorAll("button")).find((button) => button.textContent === "Details")!);
  const details = document.body.querySelector('[role="dialog"][aria-label="Details"]')!;
  expect(details.querySelector("pre")!.textContent).toBe(failure.params.detail);
  expect(details.querySelector("pre")!.getAttribute("dir")).toBe("ltr");
  const product = failedRow.querySelector("[data-inline-reference]")!;
  expect(product.textContent).toBe("Git");
  await act(async () => void details.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  // A console that failed says why under its header, before its projects.
  expect(resultCard("Side projects").textContent).toContain("Something unexpected went wrong.");
  expect(resultCard("Side projects").contains(row("Dotfiles"))).toBe(true);
  expect(row("Dotfiles").textContent).toContain("Not attempted");
  expect(row("Dotfiles").textContent).toContain("Its console could not be created.");
  // One that came out empty says so in its card.
  expect(resultCard("Scratch").textContent).toContain("This console has no projects.");
  // The accounts that fell back are marks after the names of the console and of the project, not a section.
  expect(document.body.textContent).not.toContain("Accounts that fell back");
  expect(await fallbackSentences(resultCard("Work laptop").querySelector("h3")!.parentElement!)).toContain("Claude Code account Work is not set up in Octoboard");
  expect(await fallbackSentences(row("Search API"))).toContain("Codex account Personal is not set up in Octoboard");
  await openTab("Application settings");
  expect(resultSettingLines()["Automatically pull repositories"]).toEqual({ badge: "On" });
});

it("names the result step in its title and takes focus to the dialog, wherever focus was while it ran", async () => {
  let answer!: (event: Event) => void;
  const { pastMountRefocus } = await reviewing({
    reply: (body) => body.type === "import_configuration" ? new Promise((resolve) => (answer = resolve)) : reply(body),
  });
  await pastMountRefocus();
  const dialog = document.body.querySelector<HTMLElement>("[role=dialog]")!;
  expect(dialog.querySelector("h2")?.textContent).toBe("Import configuration");
  await press("Import");
  const close = dialog.querySelector<HTMLElement>("button[slot=close], button[aria-label=Close]")!;
  act(() => close.focus());
  await act(async () => answer({ type: "configuration_imported", ...MIXED_RESULT }));
  expect(dialog.querySelector("h2")?.textContent).toBe("Import finished");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.activeElement).toBe(dialog);
});

// A browser without the file input's `cancel` event never answers for a dismissed picker.
it("can be dismissed while the open dialog has not answered", async () => {
  const mounted = mount({ unanswered: true });
  await press("Choose file");
  await act(async () => {
    document.body.querySelector("[role=dialog]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(mounted.closed()).toBe(1);
});

it("announces which choice was applied to how many, so two presses with the same count are told apart", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  await reviewing();
  await act(async () => void (await vi.advanceTimersByTimeAsync(200)));
  const announced = () => document.body.querySelector("[role=status]")?.textContent;
  await press("Default clone directory (2)");
  const first = announced();
  await press("Clone from remote (2)");
  expect([first, announced()]).toEqual([
    "Applied Default clone directory to 2 projects.",
    "Applied Clone from remote to 2 projects.",
  ]);
});

it("ignores another press of Choose file while the open dialog has not answered, and shows that it waits", async () => {
  const mounted = mount({ unanswered: true });
  const choose = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Choose file")!;
  // Two presses before the render that shows the first: only the dialog's own guard can stop the
  // second.
  await act(async () => {
    choose().click();
    choose().click();
  });
  await press("Choose file");
  expect(mounted.open).toHaveBeenCalledOnce();
  expect(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Choose file")?.hasAttribute("data-pending")).toBe(true);
});

it.each([
  { error: new ConfigurationFileError("too_large"), shown: "The file is too large to be a configuration file.", raw: undefined },
  { error: new ConfigurationFileError("not_text"), shown: "The file is not a text file.", raw: undefined },
  { error: new ConfigurationFileError("io", "Permission denied"), shown: "Could not read the file.", raw: "Permission denied" },
])("words a file that could not be read from its kind, the system's text only behind Error details: $shown", async ({ error, shown, raw }) => {
  const mounted = mountDialog((onClose) => <ImportConfigurationDialog onClose={onClose} />, reply, {
    save: vi.fn(),
    open: vi.fn(async () => Promise.reject(error)),
  });
  await press("Choose file");
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.querySelector("h2")?.textContent).toBe("Could not import the configuration");
  expect(alert.querySelector("details pre")?.textContent).toBe(raw);
  const explanation = alert.cloneNode(true) as Element;
  explanation.querySelector("details")?.remove();
  expect(explanation.textContent).toContain(shown);
  if (raw) expect(explanation.textContent).not.toContain(raw);
  expect(mounted.sent()).toEqual([]);
});

it.each([
  { error: new ConfigurationFileError("io", "Permission denied (os error 13)"), remedy: "Make sure the file still exists and that Octoboard may read it" },
  { error: new ConfigurationFileError("too_large"), remedy: "Choose a configuration file exported by a compatible Octoboard version" },
])("gives a file that could not be read a reading remedy, never one about writing: $remedy", async ({ error, remedy }) => {
  mountDialog((onClose) => <ImportConfigurationDialog onClose={onClose} />, reply, {
    save: vi.fn(),
    open: vi.fn(async () => Promise.reject(error)),
  });
  await press("Choose file");
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain(remedy);
  expect(alert.textContent).not.toMatch(/writable|disk space/);
});


it("cannot be dismissed while the import is running", async () => {
  const mounted = await reviewing({ reply: (body) => (body.type === "import_configuration" ? new Promise<Event>(() => {}) : reply(body)) });
  await press("Import");
  await act(async () => {
    document.body.querySelector("[role=dialog]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(mounted.closed()).toBe(0);
});

it("takes the window regaining focus, with no file chosen, as the picker dismissed, so the step can be tried again", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const mounted = mount({ unanswered: true });
  const choose = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Choose file")!;
  await press("Choose file");
  expect(choose().hasAttribute("data-pending")).toBe(true);
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(600);
  });
  expect(choose().hasAttribute("data-pending")).toBe(false);
  await press("Choose file");
  expect(mounted.open).toHaveBeenCalledTimes(2);
});

it("ignores the answer of a picker it gave up on once a newer one has been opened", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const answers: ((file: { name: string; text: string }) => void)[] = [];
  const mounted = mountDialog((onClose) => <ImportConfigurationDialog onClose={onClose} />, reply, {
    save: vi.fn(),
    open: vi.fn(() => new Promise<{ name: string; text: string }>((resolve) => answers.push(resolve))),
  });
  await press("Choose file");
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(600);
  });
  await press("Choose file");
  // The first picker answers late, while the second is the one being waited for.
  await act(async () => answers[0]({ name: "late.json", text: JSON.stringify(DOCUMENT) }));
  expect(mounted.sent()).toEqual([]);
  await act(async () => answers[1]({ name: "mine.json", text: JSON.stringify(DOCUMENT) }));
  expect(mounted.sent()).toEqual([{ type: "inspect_configuration", document: DOCUMENT }]);
});
