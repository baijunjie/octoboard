// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { MIXED_RESULT, REVIEW } from "../gallery/fixtures/configuration";
import { accessibleName } from "../dialogs/dialogTestSupport";
import { isolate } from "../i18n/isolate";
import { ConfigurationFileError } from "../platform";
import type { Event, GitWorkflowSettings, RequestBody } from "../protocol";
import { bindingFor, shortcutOverrides } from "../shortcuts/store";
import { ImportConfigurationDialog } from "./ImportConfigurationDialog";
import { mountDialog, press, settingLinesOf } from "./testSupport";

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
const settingsBox = () => document.body.querySelector<HTMLInputElement>("input[type=checkbox]")!;
/** The lines of the settings card of the result, by setting name. */
function resultSettingLines(): ReturnType<typeof settingLinesOf> {
  const card = Array.from(document.body.querySelectorAll("section")).find((s) => s.querySelector(":scope > h3")?.textContent === "Application settings applied")!;
  return settingLinesOf(card.querySelector("ul")!);
}

/** The lines of the settings list the checkbox is described by, by setting name. */
function settingLines(): ReturnType<typeof settingLinesOf> {
  return settingLinesOf(document.getElementById(settingsBox().getAttribute("aria-describedby")!)!);
}

/** Picks `option` in the select of the project's row. */
async function choose(project: string, option: string): Promise<void> {
  await act(async () => row(project).querySelector("button")!.click());
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
  // The console's account fallback sits under its name, the project's in its own row.
  expect(consoleCard("Work laptop").querySelector(":scope > div")!.textContent).toContain("Claude Code account Work is not set up in Octoboard");
  expect(row("Search API").textContent).toContain("Codex account Personal is not set up in Octoboard");
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
  expect(row("Notes").textContent).toContain("Not found on the machine running Octoboard");
  // Leaving a project out places it nowhere: only the exported path is left to show.
  expect(fields("Notes")).toEqual({ "Exported path": "/Users/ana/Documents/Work/Clients/Northwind/Archive/2019/Quarterly-Reports/final-notes" });
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

it("applies the application settings only once they are asked for", async () => {
  const mounted = await reviewing();
  await act(async () => settingsBox().click());
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
  await reviewing();
  expect(text()).not.toContain("Keyboard shortcuts");
});

it("says what the document's keyboard shortcuts would do on this device", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewing({ text: withShortcuts({ lock: LOCK, bad: 1 }) });
  expect(settingLines()["Keyboard shortcuts"]).toEqual({ badge: "Set to the file's", note: "1 of the file's shortcuts differs from the default. They apply to this device or browser only." });
});

it("replaces this device's keyboard shortcuts with the document's once the settings are applied, and reports it", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ pin: { code: "KeyJ", primary: true, shift: true } });
  await reviewing({ text: withShortcuts({ lock: LOCK, bad: 1 }), reply: echoing() });
  await act(async () => settingsBox().click());
  await press("Import");
  expect(bindingFor("lock")).toEqual(LOCK);
  expect(bindingFor("pin")).toEqual({ code: "KeyP", primary: true, shift: true });
  const applied = resultSettingLines();
  expect(applied["Keyboard shortcuts"]).toEqual({ badge: "1 change from the default", note: "1 shortcut in the file could not be applied here and was left out." });
});

it("leaves this device's keyboard shortcuts alone unless the settings are asked for", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewing({ text: withShortcuts({ lock: LOCK }), reply: echoing() });
  await press("Import");
  expect(bindingFor("lock")).not.toEqual(LOCK);
  expect(text()).not.toContain("Keyboard shortcuts");
});

async function importing(shortcuts: unknown, settings?: Record<string, unknown>) {
  shortcutOverrides.set({ pin: PIN });
  await reviewing({ text: withShortcuts(shortcuts), reply: echoing(settings) });
  await act(async () => settingsBox().click());
  await press("Import");
}

it("puts every shortcut back to its default when the document carries an empty set, and says so", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ pin: PIN });
  await reviewing({ text: withShortcuts({}), reply: echoing() });
  expect(settingLines()["Keyboard shortcuts"]).toEqual({ badge: "Back to defaults", note: "They apply to this device or browser only." });
  expect(text()).not.toContain("0 of them");
  await act(async () => settingsBox().click());
  await press("Import");
  expect(bindingFor("pin")).toEqual({ code: "KeyP", primary: true, shift: true });
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
  await reviewing({ text: withShortcuts({ lock: LOCK }), reply: failing });
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
  await reviewing();
  await act(async () => settingsBox().click());
  await press("Import");
  expect(bindingFor("pin")).toEqual({ code: "KeyJ", primary: true, shift: true });
});

it("places the application settings after the consoles, in a card whose checkbox the list of lines describes", async () => {
  await reviewing();
  const box = settingsBox();
  const list = document.getElementById(box.getAttribute("aria-describedby")!)!;
  expect(list.tagName).toBe("UL");
  expect(box.closest("div")!.textContent).toBe("Also apply the application settings");
  expect(box.checked).toBe(false);
  // The card follows the last console's.
  const last = consoleCard("Scratch");
  expect(last.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("notes what applying each application setting would change with a badge, and what follows only for one it turns on", async () => {
  await reviewing();
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
  await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...same }) : reply(body)) });
  expect(settingLines()["Automatically pull repositories"]).toEqual({ badge: "No change" });
});

it("states what turning on the automatic approvals lets agents do", async () => {
  await reviewing();
  const lines = settingLines();
  expect(lines["Automatically approve session requests"]).toEqual({ badge: "Turns on", note: "A session's request to start a session that reports to you is then approved without asking you first." });
  expect(lines["Automatically approve resume requests"]?.badge).toBe("Turns on");
  expect(lines["Automatically approve resume requests"]?.note).toContain("request to reopen or resume a session");
  expect(lines["Automatically approve account switches"]).toEqual({ badge: "Turns on", note: "A session's request to switch the account of one of its sessions is then approved without asking you first." });
  expect(lines["Automatically approve handoffs"]).toEqual({ badge: "Turns on", note: "A session's request to hand one of its sessions off to another agent is then approved without asking you first." });
});

it("says applying the file turns off asking questions as choices", async () => {
  await reviewing();
  expect(settingLines()["Ask questions as choices"]).toEqual({ badge: "Turns off" });
});

it("states what turning on asking questions as choices does", async () => {
  const turningOn = { ...REVIEW, settings: { ...REVIEW.settings!, ask_questions_as_choices: true, current_ask_questions_as_choices: false } };
  await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...turningOn }) : reply(body)) });
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
  await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...without }) : reply(body)) });
  expect(text()).not.toContain("Automatically approve");
});

it("leaves out the setting a file from before it existed does not carry", async () => {
  const older = { ...REVIEW, settings: { ...REVIEW.settings!, auto_delete_unresumable_archives: null, auto_select_account: null } };
  await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...older }) : reply(body)) });
  expect(settingLines()["Automatically pull repositories"]?.badge).toBe("Turns on");
  expect(text()).not.toContain("archived sessions whose conversation is gone");
  expect(text()).not.toContain("Automatically select an account");
});

describe("the git workflow settings", () => {
  const FILE: GitWorkflowSettings = { development: "in_place", rebase_before_merge: true, delivery: "merge", revert_gate: true };
  const inspecting = (git_workflow: typeof FILE | undefined, current: typeof FILE | undefined) => {
    const { git_workflow: _, current_git_workflow: __, ...rest } = REVIEW.settings!;
    const settings = { ...rest, ...(git_workflow && { git_workflow }), ...(current && { current_git_workflow: current }) };
    return (body: RequestBody) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected" as const, ...REVIEW, settings }) : reply(body));
  };

  it("notes that applying changes them when the file's differ from this machine's", async () => {
    await reviewing({ reply: inspecting(FILE, { ...FILE, development: "worktree" }) });
    expect(settingLines()["Git workflow"]).toEqual({ badge: "Set to the file's" });
  });

  it("notes that applying changes nothing when they are the same", async () => {
    await reviewing({ reply: inspecting(FILE, FILE) });
    expect(settingLines()["Git workflow"]).toEqual({ badge: "No change" });
  });

  it("leaves them out when the file does not carry them", async () => {
    await reviewing({ reply: inspecting(undefined, FILE) });
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

it("tells each project's result in a table row, with a mark and a popover for a failure, and the accounts that fell back apart", async () => {
  await reviewing();
  await press("Import");
  // A console heads its own group of rows; a project's row, or the console's own line when it failed or came out empty, is found by its name.
  const rowOf = (name: string) =>
    Array.from(document.body.querySelectorAll("tbody tr")).find((tr) => tr.querySelector("th")?.textContent === name || tr.querySelector("td")?.textContent?.endsWith(name))!;
  const line = (name: string) => rowOf(name)?.textContent ?? "";
  const groups = Array.from(document.body.querySelectorAll("tbody"));
  expect(groups.map((group) => group.querySelector("th")!.textContent)).toEqual(["Work laptop", "Side projects", "Scratch"]);
  // The console's name is said once, as the header of its group, not on every project's row.
  expect(groups[0].querySelector("th")!.getAttribute("colspan")).toBe("3");
  expect(Array.from(groups[0].querySelectorAll("tr")).map((tr) => tr.querySelectorAll("th")).map((ths) => ths.length)).toEqual([1, 0, 0, 0, 0]);
  expect(groups[0].textContent!.split("Work laptop")).toHaveLength(2);
  expect(rowOf("Search API").closest("tbody")).toBe(groups[0]);
  expect(line("Search API")).toContain("Created");
  expect(line("Search API")).toContain("In the default clone directory");
  // A failed line is the glyph and the word "Failed", not a sentence: the reason opens from them in a popover.
  const popover = async (project: string) => {
    const mark = rowOf(project).querySelector<HTMLElement>("button[aria-label$=' failed']")!;
    expect(mark).not.toBeNull();
    expect(mark.textContent).toBe("Failed");
    await act(async () => {
      for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) mark.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    });
    const dialog = document.body.querySelector(`[role="dialog"][aria-label="${mark.getAttribute("aria-label")}"]`)!;
    expect(dialog).not.toBeNull();
    return dialog;
  };
  const closePopover = () => act(async () => void document.body.querySelector("[role=\"dialog\"][aria-label$=' failed']")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(line("Website")).not.toContain("does not exist");
  expect((await popover("Website")).textContent).toContain("/Users/ana/code/website does not exist.");
  await closePopover();
  expect(line("Notes")).toContain("Left out");
  expect(line("Design System")).not.toContain("Git could not clone");
  const failedRow = rowOf("Design System");
  expect(failedRow.querySelector("pre")).toBeNull();
  const reason = await popover("Design System");
  expect(reason.textContent).toContain("Git could not clone this repository.");
  const diagnostic = reason.querySelector("pre")!;
  const failure = MIXED_RESULT.consoles.flatMap((console) => console.projects).find((project) => project.name === "Design System")!.error!;
  expect(diagnostic.textContent).toBe(failure.params.detail);
  expect(diagnostic.getAttribute("dir")).toBe("ltr");
  expect(reason.querySelectorAll("pre")).toHaveLength(1);
  const summary = Array.from(reason.querySelectorAll("span")).find((node) => node.textContent === "Git could not clone this repository.")!;
  expect(summary.contains(diagnostic)).toBe(false);
  const product = summary.querySelector("[data-inline-reference]")!;
  expect(product.textContent).toBe("Git");
  expect(product.getAttribute("dir")).toBe("auto");
  expect(product.textContent).not.toMatch(/[\u2066-\u2069]/);
  await closePopover();
  // A console that failed has a line of its own, as the group's row header followed by its result.
  const failedConsole = rowOf("Side projects");
  expect(failedConsole.querySelector("th")!.getAttribute("role")).toBe("rowheader");
  expect(failedConsole.querySelector("th")!.getAttribute("scope")).toBe("rowgroup");
  expect((await popover("Side projects")).textContent).toContain("the data directory cannot be written");
  await closePopover();
  expect(rowOf("Dotfiles").closest("tbody")).toBe(groups[1]);
  expect(line("Dotfiles")).toContain("Not attempted");
  expect(line("Dotfiles")).toContain("Its console could not be created.");
  // So does one that came out empty.
  expect(line("Scratch")).toContain("Created");
  const section = (title: string) => Array.from(document.body.querySelectorAll("section")).find((s) => s.querySelector(":scope > h3")?.textContent === title)?.textContent ?? "";
  expect(section("Accounts that fell back to the default account")).toContain("Claude Code account Work, named by the console Work laptop");
  expect(section("Accounts that fell back to the default account")).toContain("Codex account Personal, named by the project Search API in Work laptop");
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
