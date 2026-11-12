// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../input/focusGuard";
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { MIXED_RESULT, REVIEW } from "../fixtures/configurationReview";
import { press as pressElement } from "../components/alerts/detailsTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { accessibleName } from "../components/dialog/dialogTestSupport";
import { press as pressWithMouse } from "../components/hint/tooltipTestSupport";
import { isolate } from "../i18n/isolate";
import { currentLanguage, setLanguage } from "../i18n/language";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { ConfigurationFileError } from "../platform";
import type { Event, GitWorkflowSettings, ClientRequestBody } from "../protocol";
import { bindingFor, shortcutOverrides } from "../shortcuts/store";
import { sessionOrderPreference } from "../store";
import { markdownStyle } from "../viewer/markdown/markdownStyle";
import { FOCUS_GRACE_MS, ImportConfigurationDialog } from "./ImportConfigurationDialog";
import { agentSettingLinesOf, cardTitled, mountDialog, openTab, press, settingGroupsOf, settingLinesAcrossGroups, settingLinesOf } from "./testSupport";
import { fakeTimers, runOutTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const DOCUMENT = { format: "octoboard-configuration", version: 1, settings: { auto_pull_repositories: true }, consoles: [{ name: "Work laptop" }] };

/** A daemon that inspects as `REVIEW` and imports as `MIXED_RESULT`, unless a test
 * says otherwise. */
function reply(body: ClientRequestBody): Promise<Event> {
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
const SECTIONS = ["General", "App behavior", "Agent behavior", "Git", "Keyboard shortcuts", "Cleanup"];
/** Checks the Settings, which brings every section the file carries with it. */
async function checkSettings(): Promise<void> {
  await act(async () => regionBox("Settings").click());
}
const tabNames = () => Array.from(document.body.querySelectorAll("[role=tab]")).map((tab) => tab.textContent);
/** The lines of the settings cards of the result, by setting name. */
function resultSettingLines(): ReturnType<typeof settingLinesOf> {
  return settingLinesAcrossGroups(document.body.querySelector("[role=tabpanel]")!);
}

/** The lines of the settings cards of the review, by setting name. */
function settingLines(): ReturnType<typeof settingLinesOf> {
  return settingLinesAcrossGroups(document.body.querySelector("[role=tabpanel]")!);
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
  await checkSettings();
  await openTab("Settings");
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
async function fallbackSentences(scope: Element, selector = "button[data-status-mark]"): Promise<string> {
  const mark = scope.querySelector<HTMLElement>(selector)!;
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
  expect(await fallbackSentences(heading.parentElement!)).toContain("The Claude Code account Work is not set up here, so the default account is used.");
  expect(row("Search API").textContent).not.toContain("is not set up");
  expect(await fallbackSentences(row("Search API"))).toContain("The Codex account Personal is not set up here");
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
  expect(row("Notes").textContent).not.toContain("No matching project folder was found.");
  expect(row("Notes").querySelectorAll("button[data-status-mark]")).toHaveLength(1);
  const label = `${isolate("Notes")} not found, and accounts not set up`;
  const mark = row("Notes").querySelector<HTMLButtonElement>(`button[aria-label='${label}']`)!;
  expect(mark).not.toBeNull();
  act(() => mark.click());
  const popover = document.querySelector(`[role=dialog][aria-label='${label}']`)?.textContent;
  expect(popover).toContain("No matching project folder was found.");
  expect(popover).toContain("It is skipped unless you choose a directory for it.");
  expect(popover).toContain("The Codex account Personal is not set up here");
  // Leaving a project out places it nowhere: only the exported path is left to show.
  expect(fields("Notes")).toEqual({ "Exported path": "/Users/ana/Documents/Work/Clients/Northwind/Archive/2019/Quarterly-Reports/final-notes" });
});

it("tells a project whose path is from another operating system so, whatever else its row shows, and no other", async () => {
  const foreign = (body: ClientRequestBody): Promise<Event> =>
    body.type === "inspect_configuration"
      ? Promise.resolve({
        type: "configuration_inspected",
        ...REVIEW,
        consoles: REVIEW.consoles.map((c) => ({
          ...c,
          projects: c.projects.map((p) =>
            p.name === "Notes" ? { ...p, path: "C:\\Users\\ana\\notes", foreign_path: true } : p.name === "Design System" ? { ...p, foreign_path: true } : p,
          ),
        })),
      })
      : reply(body);
  await reviewing({ reply: foreign });
  const notice = "The exported path was written on another operating system, so it cannot be used here.";
  const note = (name: string) => `button[aria-label='Exported path of ${isolate(name)} cannot be used']`;
  // One problem, one presentation: never a loose line in the row.
  for (const name of ["Notes", "Design System", "Website", "Search API"]) expect(row(name).textContent).not.toContain(notice);
  // Found nowhere: the not-found mark says why, with its remedy.
  const notFound = await fallbackSentences(row("Notes"));
  expect(notFound).toContain(notice);
  expect(notFound).toContain("It is skipped unless you choose a directory for it.");
  expect(notFound).not.toContain("No matching project folder was found.");
  expect(row("Notes").querySelector(note("Notes"))).toBeNull();
  // Cloned from its remote: a note after the exported path, with no remedy, as nothing more is needed.
  expect(await fallbackSentences(row("Design System"), note("Design System"))).toBe(notice);
  expect(row("Website").querySelector(note("Website"))).toBeNull();
  expect(row("Search API").querySelector(note("Search API"))).toBeNull();
});

it("names only the missing project in its mark when no account of it is missing", async () => {
  const plain = (body: ClientRequestBody): Promise<Event> =>
    body.type === "inspect_configuration"
      ? Promise.resolve({ type: "configuration_inspected", ...REVIEW, consoles: REVIEW.consoles.map((c) => ({ ...c, projects: c.projects.map((p) => ({ ...p, account_fallbacks: [] })) })) })
      : reply(body);
  await reviewing({ reply: plain });
  expect(row("Notes").querySelectorAll("button[data-status-mark]")).toHaveLength(1);
  expect(row("Notes").querySelector(`button[aria-label='${isolate("Notes")} not found']`)).not.toBeNull();
});

it("shows no exported path for a project the export recorded by its directory name alone, and still says where it was found", async () => {
  const bare = (body: ClientRequestBody): Promise<Event> =>
    body.type === "inspect_configuration"
      ? Promise.resolve({ type: "configuration_inspected", ...REVIEW, consoles: REVIEW.consoles.map((c) => ({ ...c, projects: c.projects.map((p) => ({ ...p, path: p.name.toLowerCase().replace(/\s+/g, "-") })) })) })
      : reply(body);
  await reviewing({ reply: bare });
  expect(fields("Search API")).toEqual({ "Placed in": "/Users/dev/Projects/search-api" });
  expect(row("Search API").textContent).toContain("Found in the default clone directory");
});

it("keeps the dialog on the file step, with the daemon's reason, for a document it refuses", async () => {
  const refuse = (body: ClientRequestBody) =>
    body.type === "inspect_configuration"
      ? Promise.reject(new CodedError("This is not a configuration file.", "configuration_not_recognised", {}))
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
      import_id: expect.stringMatching(/^[0-9a-f]{32}$/),
      document: DOCUMENT,
      resolutions: [
        { console: 0, project: 0, choice: { kind: "exported_path" } },
        { console: 0, project: 1, choice: { kind: "default_clone_dir" } },
        { console: 0, project: 2, choice: { kind: "clone" } },
        { console: 1, project: 0, choice: { kind: "clone" } },
      ],
      apply_settings: [],
      settings_versions: REVIEW.settings_versions,
    },
  ]);
});

it("is back at choosing the file when the import conflicts with a setting changed since the preview", async () => {
  const conflict = new CodedError("changed elsewhere", "write_conflict", { fields: "auto_pull_repositories", versions: '{"auto_pull_repositories":9}' });
  const mounted = await reviewing({ reply: (body) => (body.type === "import_configuration" ? Promise.reject(conflict) : reply(body)) });
  await press("Import");
  expect(mounted.sent().filter((body) => body.type === "import_configuration")).toHaveLength(1);
  expect(document.body.querySelector('[role="alertdialog"]')!.textContent).toContain("Settings changed after the file was read");
  expect(text()).toContain("Choose an exported configuration file");
  expect(text()).not.toContain("Work laptop");
});

/** A daemon whose later inspections read the setting `auto_pull_repositories` at version 2. */
function inspectingNewerSettings() {
  let inspections = 0;
  return (body: ClientRequestBody): Promise<Event> => {
    if (body.type !== "inspect_configuration") return reply(body);
    inspections += 1;
    const versions = { ...REVIEW.settings_versions, ...(inspections > 1 && { auto_pull_repositories: 2 }) };
    return Promise.resolve({ type: "configuration_inspected", ...REVIEW, settings_versions: versions });
  };
}
const movePastPreview = (store: ReturnType<typeof mount>["store"]) =>
  act(() => store.setState({ settings: { ...store.getState().settings, versions: { ...store.getState().settings.versions, auto_pull_repositories: 2 } } }));

it("reads the preview again, keeping the review's choices, when a setting the import writes moves past the preview", async () => {
  const mounted = await reviewing({ reply: inspectingNewerSettings() });
  await choose("Website", "Default clone directory");
  await checkSettings();
  const before = fields("Website")["Placed in"];
  movePastPreview(mounted.store);
  await act(async () => {});
  expect(mounted.sent().filter((body) => body.type === "inspect_configuration")).toEqual([
    { type: "inspect_configuration", document: DOCUMENT },
    { type: "inspect_configuration", document: DOCUMENT },
  ]);
  expect(fields("Website")["Placed in"]).toBe(before);
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", settings_versions: { auto_pull_repositories: 2 } });
});

const moveTo = (store: ReturnType<typeof mount>["store"], version: number) =>
  act(() => store.setState({ settings: { ...store.getState().settings, versions: { ...store.getState().settings.versions, auto_pull_repositories: version } } }));
// An unavailable button carries its reason as hidden text, so its text is more than its name.
const importButton = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent?.endsWith("Import"))!;

it("reads the preview again when a setting moves while a refresh is in flight, holding Import pending meanwhile", async () => {
  const pending: Array<(versions: number) => void> = [];
  let inspections = 0;
  const mounted = await reviewing({
    reply: (body) => {
      if (body.type !== "inspect_configuration") return reply(body);
      inspections += 1;
      if (inspections === 1) return reply(body);
      return new Promise((resolve) => pending.push((version) => resolve({ type: "configuration_inspected", ...REVIEW, settings_versions: { ...REVIEW.settings_versions, auto_pull_repositories: version } })));
    },
  });
  await checkSettings();
  moveTo(mounted.store, 2);
  expect(pending).toHaveLength(1);
  expect(importButton().getAttribute("aria-busy")).toBe("true");
  // The setting moves again while the second read is out: it reads version 2, the store is at 3.
  moveTo(mounted.store, 3);
  expect(pending).toHaveLength(1);
  await act(async () => pending[0]!(2));
  expect(pending).toHaveLength(2);
  expect(importButton().getAttribute("aria-busy")).toBe("true");
  await act(async () => pending[1]!(3));
  expect(inspections).toBe(3);
  expect(importButton().getAttribute("aria-busy")).not.toBe("true");
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", settings_versions: { auto_pull_repositories: 3 } });
});

it("shows a refresh that fails as an Alert in the review with its own Try again, and reads again on it", async () => {
  fakeTimers();
  let inspections = 0;
  const mounted = await reviewing({
    reply: (body) => {
      if (body.type !== "inspect_configuration") return reply(body);
      inspections += 1;
      if (inspections === 2) return Promise.reject(new Error("offline"));
      return Promise.resolve({ type: "configuration_inspected", ...REVIEW, settings_versions: { ...REVIEW.settings_versions, ...(inspections > 2 && { auto_pull_repositories: 2 }) } });
    },
  });
  await checkSettings();
  moveTo(mounted.store, 2);
  await act(async () => {});
  expect(text()).toContain("Could not read the file again");
  expect(document.body.querySelector('[role="alertdialog"]')).toBeNull();
  // Import waits while the review is behind: unavailable, and pressing it sends nothing.
  expect(importButton().getAttribute("aria-disabled")).toBe("true");
  await act(async () => importButton().click());
  expect(mounted.sent().some((body) => body.type === "import_configuration")).toBe(false);
  // The press explained by taking focus to the Alert's button.
  expect(document.activeElement?.textContent).toBe("Try again");
  // It is not tried again by itself.
  expect(inspections).toBe(2);
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Try again")!.click());
  expect(inspections).toBe(3);
  expect(text()).not.toContain("Could not read the file again");
  expect(importButton().getAttribute("aria-disabled")).toBeNull();
  // The file's new reading is said once focus has settled.
  await act(async () => void (await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS)));
  expect(Array.from(document.body.querySelectorAll("[role=status]")).map((region) => region.textContent)).toContain("The file was read again because settings changed.");
});

/** A review whose second reading fails (the settings moved on while it was open), so Import is unavailable and the
 * Alert offers Try again; the third reading succeeds. */
async function reviewWithFailedRefresh() {
  fakeTimers();
  let inspections = 0;
  const mounted = await reviewing({
    reply: (body) => {
      if (body.type !== "inspect_configuration") return reply(body);
      inspections += 1;
      if (inspections === 2) return Promise.reject(new Error("offline"));
      return Promise.resolve({ type: "configuration_inspected", ...REVIEW, settings_versions: { ...REVIEW.settings_versions, ...(inspections > 2 && { auto_pull_repositories: 2 }) } });
    },
  });
  await checkSettings();
  moveTo(mounted.store, 2);
  await act(async () => {});
  expect(text()).toContain("Could not read the file again");
  return mounted;
}
/** Enter on a focused button: the keydown, the click the browser makes of it, and the keyup. */
const pressWithKey = (button: HTMLElement) =>
  act(() => {
    button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    button.click();
    button.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  });
const tryAgain = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Try again")!;

it.each([
  ["a mouse press", "pointer", (button: HTMLElement) => pressWithMouse(button, "mouse")],
  ["a key", "keyboard", pressWithKey],
] as const)("takes focus to the Alert's Try again from a press of the unavailable Import, ringed only after a key: %s", async (_, modality, activate) => {
  await reviewWithFailedRefresh();
  runOutTimers();
  act(() => importButton().focus());

  activate(importButton());
  await act(async () => {});
  expect(document.activeElement).toBe(tryAgain());
  expect(getInteractionModality()).toBe(modality);
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(modality === "keyboard");
});

it.each([
  ["a mouse press", "pointer", (button: HTMLElement) => pressWithMouse(button, "mouse")],
  ["a key", "keyboard", pressWithKey],
] as const)("hands focus from Try again back to Import when the read succeeds and the Alert goes, ringed only after a key: %s", async (_, modality, activate) => {
  await reviewWithFailedRefresh();
  runOutTimers();
  const retry = tryAgain();
  act(() => retry.focus());

  activate(retry);
  await act(async () => {});
  expect(text()).not.toContain("Could not read the file again");
  expect(document.activeElement).toBe(importButton());
  expect(getInteractionModality()).toBe(modality);
  expect(document.activeElement?.hasAttribute("data-focus-visible")).toBe(modality === "keyboard");
});

it("offers to choose the file again where a failed refresh cannot be retried", async () => {
  let inspections = 0;
  const mounted = await reviewing({
    reply: (body) => {
      if (body.type !== "inspect_configuration") return reply(body);
      inspections += 1;
      if (inspections === 2) return Promise.reject(new CodedError("Not a valid path.", "invalid_path", {}));
      return reply(body);
    },
  });
  await checkSettings();
  moveTo(mounted.store, 2);
  await act(async () => {});
  expect(text()).toContain("Choose the file again.");
  expect(Array.from(document.body.querySelectorAll("button")).some((b) => b.textContent === "Try again")).toBe(false);
  expect(importButton().getAttribute("aria-disabled")).toBe("true");
});

/** Commits each pending timer separately, so focus feedback settles before the failure becomes an alert. */
async function refreshFailureDelay(): Promise<void> {
  const deadline = Date.now() + AFTER_FOCUS_MOVE_DELAY_MS;
  while (Date.now() < deadline) {
    expect(document.body.querySelector('[role="alert"]')).toBeNull();
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
  }
  expect(Date.now()).toBe(deadline);
}

it.each([
  { code: "configuration_version_unsupported", action: "Choose file", other: "Try again" },
  { code: "connection_closed", action: "Try again", other: "Choose file" },
])("waits for Import's unavailable feedback before announcing a $code refresh failure with $action", async ({ code, action, other }) => {
  fakeTimers();
  let inspections = 0;
  const mounted = await reviewing({
    reply: (body) => {
      if (body.type !== "inspect_configuration" || ++inspections === 1) return reply(body);
      return Promise.reject(new CodedError("Could not read the configuration", code, {}));
    },
  });
  await checkSettings();
  runOutTimers();
  const importing = importButton();
  act(() => importing.focus());
  runOutTimers();
  moveTo(mounted.store, 2);
  await act(async () => {});
  expect(text()).toContain("Could not read the file again");
  expect(importing.getAttribute("aria-disabled")).toBe("true");
  const labels = Array.from(document.body.querySelectorAll("button"), (button) => button.textContent);
  expect(labels).toContain(action);
  expect(labels).not.toContain(other);
  await refreshFailureDelay();
  expect(document.body.querySelector('[role="alert"]')?.textContent).toContain("Could not read the file again");
  expect(document.activeElement).toBe(importing);
});

it("waits afresh before announcing a repeated refresh failure while Try again stays mounted and keeps focus", async () => {
  fakeTimers();
  const fail: Array<(error: CodedError) => void> = [];
  let inspections = 0;
  const mounted = await reviewing({
    reply: (body) => {
      if (body.type !== "inspect_configuration" || ++inspections === 1) return reply(body);
      return new Promise((_, reject) => fail.push(reject));
    },
  });
  await checkSettings();
  runOutTimers();
  moveTo(mounted.store, 2);
  const failure = new CodedError("Connection closed", "connection_closed", {});
  await act(async () => fail[0]!(failure));
  await refreshFailureDelay();
  const firstAnnouncement = document.body.querySelector('[role="alert"]');
  expect(firstAnnouncement?.textContent).toContain("Could not read the file again");
  const retry = Array.from(document.body.querySelectorAll("button")).find((button) => button.textContent === "Try again")!;
  act(() => retry.focus());
  runOutTimers();
  await act(async () => retry.click());
  expect(retry.hasAttribute("data-pending")).toBe(true);
  expect(text()).toContain("Could not read the file again");
  expect(document.body.querySelector('[role="alert"]')).toBe(firstAnnouncement);
  await act(async () => fail[1]!(failure));
  expect(retry.hasAttribute("data-pending")).toBe(false);
  await refreshFailureDelay();
  expect(document.body.querySelector('[role="alert"]')).not.toBe(firstAnnouncement);
  expect(document.body.querySelector('[role="alert"]')?.textContent).toContain("Could not read the file again");
  expect(document.activeElement).toBe(retry);
});

it("does not read the preview again for a setting the import does not write", async () => {
  const mounted = await reviewing({ reply: inspectingNewerSettings() });
  movePastPreview(mounted.store);
  await act(async () => {});
  expect(mounted.sent().filter((body) => body.type === "inspect_configuration")).toHaveLength(1);
});

it("applies the settings of the sections checked and no other", async () => {
  const mounted = await reviewingSettings();
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: ["general", "app_behavior", "agent_behavior", "git", "keyboard_shortcuts", "cleanup"] });
});

it("sends the sections still checked in Settings order, whichever order the others were unchecked in", async () => {
  const mounted = await reviewingSettings();
  for (const label of ["Keyboard shortcuts", "Git", "Agent behavior", "App behavior"]) await act(async () => regionBox(label).click());
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: ["general", "cleanup"] });
  expect(mounted.sent().at(-1)).not.toHaveProperty("apply_clone_dir");
});

const FOCUS = { code: "KeyK", primary: true, shift: true };
/** A daemon that, when asked for the settings, applies them and echoes the document's keyboard shortcuts and the
 * settings only a client holds as they came (or as `settings` has it, where given); asked for no settings, it reports
 * none applied. */
const echoing = (settings: Record<string, unknown> = {}) => (body: ClientRequestBody): Promise<Event> => {
  if (body.type !== "import_configuration") return reply(body);
  const document = (body.document as { settings?: Record<string, unknown> }).settings ?? {};
  const passedThrough = Object.fromEntries(["keyboard_shortcuts", "appearance", "markdown_style", "run_in_background", "show_menu_bar_icon", "interface_language", "session_order"].flatMap((field) => (document[field] === undefined || document[field] === null ? [] : [[field, document[field]]])));
  const applied = { ...MIXED_RESULT.settings_applied!, ...passedThrough, ...settings };
  return Promise.resolve({ type: "configuration_imported", ...MIXED_RESULT, settings_applied: body.apply_settings?.length ? applied : null });
};
const QUICK_CREATE = { code: "KeyJ", primary: true, shift: true };
const withShortcuts = (shortcuts: unknown) => JSON.stringify({ ...DOCUMENT, settings: { ...DOCUMENT.settings, keyboard_shortcuts: shortcuts } });

it("says nothing of keyboard shortcuts when the document carries none", async () => {
  await reviewingSettings();
  expect(settingLines()["Changed shortcuts"]).toBeUndefined();
});

it("says what the document's keyboard shortcuts would do on this device", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewingSettings({ text: withShortcuts({ focusMode: FOCUS, bad: 1 }) });
  expect(settingLines()["Changed shortcuts"]).toEqual({
    badge: "Set to the file's",
    note: "1 of the file's shortcuts differs from the default. Shortcut changes apply only to this browser.",
    values: ["Toggle focus mode: CtrlShiftK (now CtrlShiftF)"],
  });
});

it("groups the application settings as the Settings dialog does, each group in the order of its section", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewingSettings({ text: withShortcuts({ focusMode: FOCUS }) });
  const groups = settingGroupsOf(document.body.querySelector("[role=tabpanel]")!);
  expect(groups.map((group) => group.title)).toEqual(["General", "App behavior", "Agent behavior", "Git", "Keyboard shortcuts", "Cleanup"]);
  expect(groups[0].lines).toEqual([]);
  expect(groups[1].lines).toEqual(["Resume sessions on start", "Prevent sleep", "When to prevent sleep", "Close idle terminals when closing the panel"]);
  expect(groups[2].lines).toEqual([
    "Automatically approve session requests",
    "Automatically approve resume requests",
    "Automatically approve account switches",
    "Automatically approve handoffs",
    "Ask questions as choices",
  ]);
  expect(groups[3].lines).toEqual(["Default clone directory", "Automatically pull repositories", "Git workflow"]);
  expect(groups[4].lines).toEqual(["Changed shortcuts"]);
  expect(groups[5].lines).toEqual(["Automatically delete archived sessions whose conversation is gone"]);
});

it("lists the App behavior card in the order of the section on a desktop app, the settings this client holds among the daemon's", async () => {
  onTestFinished(() => localStorage.clear());
  mountDialog(
    (onClose) => <ImportConfigurationDialog onClose={onClose} />,
    echoing(),
    {
      save: vi.fn(),
      open: vi.fn(async () => ({
        name: "mine.json",
        text: JSON.stringify({ ...DOCUMENT, settings: { ...DOCUMENT.settings, run_in_background: true, show_menu_bar_icon: false } }),
      })),
    },
    {},
    {
      backgroundRun: { set: vi.fn(async () => {}) },
      statusItem: { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} },
    },
  );
  await press("Choose file");
  await checkSettings();
  await openTab("Settings");
  const appBehavior = settingGroupsOf(document.body.querySelector("[role=tabpanel]")!).find((group) => group.title === "App behavior");
  expect(appBehavior!.lines).toEqual([
    "Resume sessions on start",
    "Keep running in the background when the window is closed",
    "Show notification area icon",
    "Prevent sleep",
    "When to prevent sleep",
    "Close idle terminals when closing the panel",
  ]);
});

it("replaces this device's keyboard shortcuts with the document's once the settings are applied, and reports it", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ quickCreate: { code: "KeyJ", primary: true, shift: true } });
  await reviewingSettings({ text: withShortcuts({ focusMode: FOCUS, bad: 1 }), reply: echoing() });
  await press("Import");
  expect(bindingFor("focusMode")).toEqual(FOCUS);
  expect(bindingFor("quickCreate")).toEqual({ code: "KeyN", primary: true, shift: true });
  await openTab("Settings");
  const applied = resultSettingLines();
  expect(applied["Changed shortcuts"]).toEqual({ badge: "1 change from the default", note: "1 shortcut in the file could not be applied here and was left out.", values: ["Toggle focus mode: CtrlShiftK"] });
});

it("leaves this device's keyboard shortcuts alone unless the settings are included", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  await reviewing({ text: withShortcuts({ focusMode: FOCUS }), reply: echoing() });
  await press("Import");
  expect(bindingFor("focusMode")).not.toEqual(FOCUS);
  expect(text()).not.toContain("Changed shortcuts");
});

async function importing(shortcuts: unknown, settings?: Record<string, unknown>) {
  shortcutOverrides.set({ quickCreate: QUICK_CREATE });
  await reviewingSettings({ text: withShortcuts(shortcuts), reply: echoing(settings) });
  await press("Import");
}

it("puts every shortcut back to its default when the document carries an empty set, and says so", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ quickCreate: QUICK_CREATE });
  await reviewingSettings({ text: withShortcuts({}), reply: echoing() });
  expect(settingLines()["Changed shortcuts"]).toEqual({ badge: "Back to defaults", note: "Shortcut changes apply only to this browser.", values: ["New session with defaults: CtrlShiftN (now CtrlShiftJ)"] });
  expect(text()).not.toContain("0 of them");
  await press("Import");
  expect(bindingFor("quickCreate")).toEqual({ code: "KeyN", primary: true, shift: true });
  await openTab("Settings");
  expect(resultSettingLines()["Changed shortcuts"]).toEqual({ badge: "Defaults" });
  expect(text()).not.toContain("0 changes");
});

it("leaves this device's keyboard shortcuts alone when the settings could not be applied", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  const failing = (body: ClientRequestBody): Promise<Event> =>
    body.type === "import_configuration"
      ? Promise.resolve({ type: "configuration_imported", ...MIXED_RESULT, settings_applied: null, settings_error: { code: "import_settings_missing", params: {}, message: "no settings" } as never })
      : reply(body);
  shortcutOverrides.set({ quickCreate: QUICK_CREATE });
  await reviewingSettings({ text: withShortcuts({ focusMode: FOCUS }), reply: failing });
  await checkSettings();
  await press("Import");
  expect(bindingFor("quickCreate")).toEqual(QUICK_CREATE);
  expect(bindingFor("focusMode")).not.toEqual(FOCUS);
});

it.each([[[FOCUS]], ["focusMode"], [null], [7]])("ignores a keyboard_shortcuts echo that is not an object (%j)", async (echo) => {
  onTestFinished(() => shortcutOverrides.set({}));
  await importing({ focusMode: FOCUS }, { keyboard_shortcuts: echo });
  expect(bindingFor("quickCreate")).toEqual(QUICK_CREATE);
  expect(bindingFor("focusMode")).not.toEqual(FOCUS);
});

it("leaves this device's keyboard shortcuts alone when the document carries none, whatever else is applied", async () => {
  onTestFinished(() => shortcutOverrides.set({}));
  shortcutOverrides.set({ quickCreate: { code: "KeyJ", primary: true, shift: true } });
  await reviewingSettings();
  await press("Import");
  expect(bindingFor("quickCreate")).toEqual({ code: "KeyJ", primary: true, shift: true });
});

describe("the settings only this client holds, and the language", () => {
  const carrying = (settings: Record<string, unknown>) => JSON.stringify({ ...DOCUMENT, settings: { ...DOCUMENT.settings, ...settings } });
  const FILE = { appearance: "dark", markdown_style: "github", interface_language: "ja", run_in_background: true, show_menu_bar_icon: false };
  const restoreOnFinish = () =>
    onTestFinished(() => {
      markdownStyle.set("tailwind");
      setLanguage("en");
      runInBackground.set(false);
      showMenuBarIcon.set(true);
      localStorage.removeItem("heroui-theme");
    });

  it("are reviewed against what this client has, and the one its platform lacks is left out", async () => {
    restoreOnFinish();
    await reviewingSettings({ text: carrying(FILE) });
    const lines = settingLines();
    expect(lines["Appearance"]).toEqual({ badge: "Set to the file's", note: "Dark (now System)" });
    expect(lines["Markdown style"]).toEqual({ badge: "Set to the file's", note: "GitHub (now Tailwind)" });
    expect(lines["Language"]).toEqual({ badge: "Set to the file's", note: "日本語 (now English)" });
    // Each name is marked as the language it is written in; the Markdown style names are English.
    expect(Array.from(document.body.querySelectorAll("[lang]"), (node) => `${node.getAttribute("lang")}:${node.textContent}`)).toEqual(expect.arrayContaining(["ja:日本語", "en:English"]));
    // A browser has no background running.
    expect(lines["Keep running in the background when the window is closed"]).toBeUndefined();
    expect(Object.keys(lines).filter((label) => /^Show .* icon$/.test(label))).toEqual([]);
  });

  it("say they change nothing where they are what this client has, and are not listed where the file has none, or one this client does not know", async () => {
    restoreOnFinish();
    await reviewingSettings({ text: carrying({ markdown_style: "tailwind", appearance: "sepia", interface_language: "tlh" }) });
    const lines = settingLines();
    expect(lines["Markdown style"]).toEqual({ badge: "No change" });
    for (const unlisted of ["Appearance", "Language", "Keep running in the background when the window is closed"]) expect(lines[unlisted]).toBeUndefined();
    expect(Object.keys(lines).filter((label) => /^Show .* icon$/.test(label))).toEqual([]);
  });

  it("are applied to this client from what the daemon says it applied, and told in the result", async () => {
    restoreOnFinish();
    await reviewingSettings({ text: carrying(FILE), reply: echoing() });
    await press("Import");
    expect(localStorage.getItem("heroui-theme")).toBe("dark");
    expect(markdownStyle.get()).toBe("github");
    // The language is the daemon's to apply, which the window then follows (see `useInterfaceLanguage`): this client
    // leaves its own as it is.
    expect(currentLanguage()).toBe("en");
    await openTab("Settings");
    // Not a platform that runs in the background: the value is not taken.
    expect(runInBackground.get()).toBe(false);
    const lines = resultSettingLines();
    expect(lines["Appearance"]).toEqual({ badge: "Dark" });
    expect(lines["Markdown style"]).toEqual({ badge: "GitHub" });
    expect(lines["Language"]).toEqual({ badge: "日本語" });
    expect(Object.keys(lines)).not.toContain("Keep running in the background when the window is closed");
    expect(Object.keys(lines).filter((label) => /^Show .* icon$/.test(label))).toEqual([]);
    expect(showMenuBarIcon.get()).toBe(true);
  });

  describe("the sorting", () => {
    const restoreOrder = () => onTestFinished(() => sessionOrderPreference.set({ sessions: "status", activeProjects: "status", inactiveProjects: "name" }));

    it("is reviewed as a setting of several items, listing only the items that would change", async () => {
      restoreOrder();
      await reviewingSettings({ text: carrying({ session_order: { sessions: "started", active_projects: "status", inactive_projects: "used" } }) });
      expect(settingLines()["Sorting"]).toEqual({ badge: "Set to the file's", values: ["Sessions: Recently started (now Status)", "Inactive projects: Recently used (now Name)"] });
    });

    it("says nothing changes where the file's order is this client's", async () => {
      restoreOrder();
      await reviewingSettings({ text: carrying({ session_order: { sessions: "status", inactive_projects: "name" } }) });
      expect(settingLines()["Sorting"]).toEqual({ badge: "No change" });
    });

    it("is not listed where the file carries no part this client knows", async () => {
      restoreOrder();
      await reviewingSettings({ text: carrying({ session_order: { sessions: "oldest" } }) });
      expect(settingLines()["Sorting"]).toBeUndefined();
    });

    it("is applied part by part from what the daemon says it applied, and told in the result", async () => {
      restoreOrder();
      await reviewingSettings({ text: carrying({ session_order: { sessions: "started", active_projects: "used", inactive_projects: "oldest" } }), reply: echoing() });
      await press("Import");
      expect(sessionOrderPreference.get()).toEqual({ sessions: "started", activeProjects: "used", inactiveProjects: "name" });
      await openTab("Settings");
      expect(resultSettingLines()["Sorting"]).toEqual({ badge: "Matches the file", values: ["Sessions: Recently started", "Active projects: Recently used"] });
    });
  });

  it("take the background running where the platform has it", async () => {
    restoreOnFinish();
    mountDialog(
      (onClose) => <ImportConfigurationDialog onClose={onClose} />,
      echoing(),
      { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: carrying(FILE) })) },
      {},
      { backgroundRun: { set: vi.fn(async () => {}) } },
    );
    await press("Choose file");
    await checkSettings();
    await openTab("Settings");
    expect(settingLines()["Keep running in the background when the window is closed"]).toEqual({ badge: "Set to the file's", note: "On (now Off)" });
    await press("Import");
    act(() => setLanguage("en"));
    await openTab("Settings");
    expect(runInBackground.get()).toBe(true);
    expect(resultSettingLines()["Keep running in the background when the window is closed"]).toEqual({ badge: "On" });
  });

  it("take the menu bar icon where the platform has one", async () => {
    restoreOnFinish();
    const statusItem = { setMenu: async () => {}, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
    mountDialog(
      (onClose) => <ImportConfigurationDialog onClose={onClose} />,
      echoing(),
      { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: carrying(FILE) })) },
      {},
      { statusItem },
    );
    await press("Choose file");
    await checkSettings();
    await openTab("Settings");
    // The test host is a Windows one, which names the icon as the notification area's.
    expect(settingLines()["Show notification area icon"]).toEqual({ badge: "Set to the file's", note: "Off (now On)" });
    await press("Import");
    act(() => setLanguage("en"));
    await openTab("Settings");
    expect(showMenuBarIcon.get()).toBe(false);
    expect(resultSettingLines()["Show notification area icon"]).toEqual({ badge: "Off" });
  });

  it("are left alone when the settings are not imported", async () => {
    restoreOnFinish();
    await reviewing({ text: carrying(FILE), reply: echoing() });
    await press("Import");
    expect(markdownStyle.get()).toBe("tailwind");
    expect(currentLanguage()).toBe("en");
  });

  it("are left alone when the daemon could not apply the settings", async () => {
    restoreOnFinish();
    const failing = (body: ClientRequestBody): Promise<Event> =>
      body.type === "import_configuration"
        ? Promise.resolve({ type: "configuration_imported", ...MIXED_RESULT, settings_applied: null, settings_error: { code: "internal_error", params: {}, message: "no" } })
        : reply(body);
    await reviewingSettings({ text: carrying(FILE), reply: failing });
    await press("Import");
    expect(markdownStyle.get()).toBe("tailwind");
    expect(currentLanguage()).toBe("en");
  });
});

it("leaves Prevent sleep to the daemon: its lines come from the inspection and the reply, and the file's own text does not make them", async () => {
  onTestFinished(() => setLanguage("en"));
  // The file's own text says the opposite of what the daemon's inspection and reply do.
  const text = JSON.stringify({ ...DOCUMENT, settings: { ...DOCUMENT.settings, keep_awake: false, keep_awake_when: "working" } });
  mountDialog(
    (onClose) => <ImportConfigurationDialog onClose={onClose} />,
    echoing({ keep_awake: true, keep_awake_when: "always" }),
    { save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text })) },
  );
  await press("Choose file");
  await checkSettings();
  await openTab("Settings");
  expect(settingLines()["Prevent sleep"]).toEqual({ badge: "Turns on", note: undefined });
  expect(settingLines()["When to prevent sleep"]).toEqual({ badge: "Set to the file's", note: "Always (now While sessions work)" });
  await press("Import");
  act(() => setLanguage("en"));
  await openTab("Settings");
  expect(resultSettingLines()["Prevent sleep"]).toEqual({ badge: "On" });
  expect(resultSettingLines()["When to prevent sleep"]).toEqual({ badge: "Always" });
});

describe("the default clone directory", () => {
  const cloneBox = () =>
    Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")).find((input) => input.labels?.[0]?.textContent === "Use the file's default clone directory");

  it("is shown against this machine's, with the choice to keep this machine's own", async () => {
    await reviewingSettings();
    // The choice to keep this machine's own is under the line's note.
    expect(settingLines()["Default clone directory"]).toEqual({ badge: "Set to the file's", note: "/Users/ana/Code/clones (now /Users/dev/Projects)Use the file's default clone directory" });
    expect(cloneBox()!.checked).toBe(true);
  });

  it("is applied by default, without a word of it in the request", async () => {
    const mounted = await reviewingSettings();
    await press("Import");
    await openTab("Settings");
    expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: expect.arrayContaining(["git"]) });
    expect(mounted.sent().at(-1)).not.toHaveProperty("apply_clone_dir");
    expect(resultSettingLines()["Default clone directory"]).toEqual({ badge: "Matches the file", note: "/Users/ana/Code/clones" });
  });

  it("is kept at this machine's own when the user unchecks it, which the request says", async () => {
    const mounted = await reviewingSettings();
    await act(async () => cloneBox()!.click());
    // Its own sentence says what is kept and names the file's, not the change.
    expect(settingLines()["Default clone directory"]).toEqual({ badge: "No change", note: "/Users/dev/Projects is kept instead of the file's /Users/ana/Code/clones.Use the file's default clone directory" });
    await press("Import");
    expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: expect.arrayContaining(["git"]), apply_clone_dir: false });
  });

  it("is not offered a choice where it is this machine's own, nor when the file carries none", async () => {
    const same = { ...REVIEW, settings: { ...REVIEW.settings!, default_clone_dir: "/Users/dev/Projects" } };
    await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...same }) : reply(body)) });
    expect(settingLines()["Default clone directory"]).toEqual({ badge: "No change" });
    expect(cloneBox()).toBeUndefined();
  });

  it("is not listed when the file carries none", async () => {
    const none = { ...REVIEW, settings: { ...REVIEW.settings!, default_clone_dir: null } };
    await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...none }) : reply(body)) });
    expect(settingLines()["Default clone directory"]).toBeUndefined();
  });

  it("says a directory written on another operating system is not used here, with no choice to make", async () => {
    const foreign = { ...REVIEW, settings: { ...REVIEW.settings!, default_clone_dir: null, foreign_default_clone_dir: true } };
    await reviewingSettings({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...foreign }) : reply(body)) });
    expect(settingLines()["Default clone directory"]).toEqual({
      badge: "No change",
      note: "The file's default clone directory was written on another operating system, so it cannot be used here. The current default clone directory is kept.",
    });
    expect(cloneBox()).toBeUndefined();
  });

  describe("the targets the review offers", () => {
    // "Search API" is found under this machine's clone directory only, "Dotfiles" under the file's only.
    const inspecting = (body: ClientRequestBody) => {
      if (body.type !== "inspect_configuration") return reply(body);
      const consoles = REVIEW.consoles.map((console) => ({
        ...console,
        projects: console.projects.map((project) =>
          project.name === "Search API"
            ? { ...project, in_document_clone_dir: { offers: { exported_path: null, default_clone_dir: null, clone: null }, default: null } }
            : project.name === "Dotfiles"
              ? { ...project, in_document_clone_dir: { offers: { exported_path: null, default_clone_dir: "/Users/ana/Code/clones/dotfiles", clone: "/Users/ana/Code/clones/dotfiles" }, default: "default_clone_dir" as const } }
              : project,
        ),
      }));
      return Promise.resolve({ type: "configuration_inspected" as const, ...REVIEW, consoles });
    };

    it("are this machine's while the settings are not imported, and the file's once they are, unless the file's directory is kept off", async () => {
      await reviewing({ reply: inspecting });
      // The settings region is unchecked: nothing of the file's directory applies.
      expect(fields("Search API")["Placed in"]).toBe("/Users/dev/Projects/search-api");
      expect(fields("Dotfiles")["Clone into"]).toBe("/Users/dev/Projects/dotfiles");
      await checkSettings();
      // Taking the file's directory: nothing is under it for Search API, which is then left out, and Dotfiles, which
      // the user never touched, follows the file's preselection and is placed there.
      expect(fields("Search API")["Placed in"]).toBeUndefined();
      expect(fields("Dotfiles")["Placed in"]).toBe("/Users/ana/Code/clones/dotfiles");
      await openTab("Settings");
      await act(async () => cloneBox()!.click());
      await openTab("Consoles and projects");
      expect(fields("Search API")["Placed in"]).toBe("/Users/dev/Projects/search-api");
      expect(fields("Dotfiles")["Clone into"]).toBe("/Users/dev/Projects/dotfiles");
    });

    it("send the choices as they stand under the directory the import takes", async () => {
      const mounted = await reviewing({ reply: inspecting });
      await checkSettings();
      await press("Import");
      const { resolutions } = mounted.sent().at(-1) as { resolutions: { console: number; project: number; choice: { kind: string } }[] };
      const dotfiles = resolutions.find((r) => r.console === 1 && r.project === 0);
      expect(dotfiles?.choice.kind).toBe("default_clone_dir");
      expect(resolutions.find((r) => r.console === 0 && r.project === 1)).toBeUndefined();
    });

    it("go back to this machine's when the Git section is unchecked, and the request never refuses the directory then", async () => {
      const mounted = await reviewing({ reply: inspecting });
      await checkSettings();
      await openTab("Settings");
      await act(async () => regionBox("Git").click());
      await openTab("Consoles and projects");
      expect(fields("Search API")["Placed in"]).toBe("/Users/dev/Projects/search-api");
      expect(fields("Dotfiles")["Clone into"]).toBe("/Users/dev/Projects/dotfiles");
      // The box to keep this machine's own directory is unchecked while the section is on; the section off after it leaves
      // nothing for the box to refuse.
      await openTab("Settings");
      await act(async () => regionBox("Git").click());
      await act(async () => cloneBox()!.click());
      await act(async () => regionBox("Git").click());
      await press("Import");
      expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: expect.not.arrayContaining(["git"]) });
      expect(mounted.sent().at(-1)).not.toHaveProperty("apply_clone_dir");
    });
  });

  it("is not sent a refusal while the settings are not imported", async () => {
    const mounted = await reviewing();
    await press("Import");
    expect(mounted.sent().at(-1)).not.toHaveProperty("apply_clone_dir");
  });
});

it("offers the parts of the file as checkboxes above the tabs, the Settings unchecked, and a tab only for a checked part", async () => {
  await reviewing();
  expect(regionBox("Consoles and projects").checked).toBe(true);
  expect(Array.from(document.body.querySelectorAll("input[type=checkbox]")).map((box) => (box as HTMLInputElement).labels?.[0]?.textContent)).toEqual([
    "Consoles and projects", "Settings",
  ]);
  expect(regionBox("Settings").checked).toBe(false);
  // One checked part is one tab, the bar kept.
  expect(tabNames()).toEqual(["Consoles and projects"]);
  // Checking the Settings brings their tab with every carried section checked, a card and a checkbox for each.
  await checkSettings();
  expect(tabNames()).toEqual(["Consoles and projects", "Settings"]);
  await openTab("Settings");
  expect(Array.from(document.body.querySelectorAll("[role=tabpanel] h3")).map((heading) => heading.textContent)).toEqual(SECTIONS);
  for (const label of SECTIONS) expect(regionBox(label).checked, label).toBe(true);
  // An unchecked section collapses to its title, marked as left out, and can be checked again.
  await act(async () => regionBox("Cleanup").click());
  const cleanup = () => Array.from(document.body.querySelectorAll("section")).find((card) => card.querySelector("h3")?.textContent === "Cleanup")!;
  expect(cleanup().querySelector("ul")).toBeNull();
  expect(cleanup().textContent).toContain("Left out");
  await act(async () => regionBox("Cleanup").click());
  expect(cleanup().querySelector("ul")).not.toBeNull();
  expect(cleanup().textContent).not.toContain("Left out");
  // Unchecking the Settings drops their tab; the sections keep their choice for when it is checked again.
  await act(async () => regionBox("Cleanup").click());
  await act(async () => regionBox("Settings").click());
  expect(tabNames()).toEqual(["Consoles and projects"]);
  await checkSettings();
  expect(regionBox("Cleanup").checked).toBe(false);
  await act(async () => regionBox("Consoles and projects").click());
  expect(tabNames()).toEqual(["Settings"]);
  expect(text()).toContain("Automatically pull repositories");
  expect(text()).not.toContain("Set every project that offers it to");
  expect(document.body.textContent).not.toContain("Also apply the application settings");
});

it("starts an accounts region that holds no agent and no account unchecked like the settings, and one that holds an agent checked", async () => {
  const agent_settings = {
    auto_select_account: true,
    current_auto_select_account: false,
    auto_switch_on_usage_limit: false,
    current_auto_switch_on_usage_limit: false,
    auto_start_quota_windows: false,
    current_auto_start_quota_windows: false,
    provider_settings: {},
    current_provider_settings: {},
    agent_tags: {},
    current_agent_tags: {},
  };
  const inspecting = (accounts: NonNullable<typeof REVIEW.accounts>) => (body: ClientRequestBody) =>
    body.type === "inspect_configuration" ? Promise.resolve<Event>({ type: "configuration_inspected", ...REVIEW, accounts }) : reply(body);

  // What a version 1 file with settings and no accounts is upgraded to: the Agent accounts settings alone.
  const mounted = await reviewing({ reply: inspecting({ agents_to_add: [], agents: [], agent_settings }) });
  expect(regionBox("Agent accounts").checked).toBe(false);
  expect(regionBox("Consoles and projects").checked).toBe(true);
  expect(tabNames()).toEqual(["Consoles and projects"]);
  await press("Import");
  expect(mounted.sent().at(-1)).not.toHaveProperty("apply_accounts");
});

describe("the accounts region on opening", () => {
  const listed = { agent: "claude" as const, default_name: null, default_name_applies: false, default_disabled: null, default_disabled_applies: false, accounts: [] };
  const opened = async (accounts: NonNullable<typeof REVIEW.accounts>) => {
    await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve<Event>({ type: "configuration_inspected", ...REVIEW, accounts }) : reply(body)) });
    return regionBox("Agent accounts").checked;
  };
  const rows = [{ name: "Work", directory: "~/.work", foreign_directory: false, proposed_directory: "/Users/dev/.work", status: "new" as const, in_use_by: null, error: null, disabled: false }];

  const switches = {
    auto_select_account: true,
    current_auto_select_account: false,
    auto_switch_on_usage_limit: true,
    current_auto_switch_on_usage_limit: false,
    auto_start_quota_windows: true,
    current_auto_start_quota_windows: false,
    provider_settings: {},
    current_provider_settings: {},
    agent_tags: {},
    current_agent_tags: {},
  };

  it.each([
    ["an agent to add", { agents_to_add: ["codex" as const], agents: [], agent_settings: null }],
    ["an account", { agents_to_add: [], agents: [{ ...listed, accounts: rows }], agent_settings: null }],
    ["a default account's name that applies", { agents_to_add: [], agents: [{ ...listed, default_name: "Main", default_name_applies: true }], agent_settings: null }],
    ["a default account's switch that applies", { agents_to_add: [], agents: [{ ...listed, default_disabled: true, default_disabled_applies: true }], agent_settings: null }],
    ["an agent's provider settings", { agents_to_add: [], agents: [], agent_settings: { ...switches, provider_settings: { codex: { claude_compatibility: true } } } }],
    ["an agent's tags", { agents_to_add: [], agents: [], agent_settings: { ...switches, agent_tags: { codex: ["review"] } } }],
  ])("starts checked when the file brings %s", async (_name, accounts) => {
    expect(await opened(accounts)).toBe(true);
  });

  it("starts unchecked when the agents it lists bring nothing", async () => {
    expect(await opened({ agents_to_add: [], agents: [listed], agent_settings: null })).toBe(false);
  });

  it("starts unchecked when the file brings only the three global switches", async () => {
    expect(await opened({ agents_to_add: [], agents: [], agent_settings: switches })).toBe(false);
  });

  it.each([
    ["provider settings of an agent this daemon does not serve", { provider_settings: { unknown_agent: { claude_compatibility: true } } }],
    ["a setting the provider does not have", { provider_settings: { codex: { not_a_setting: true } } }],
    ["a value the setting does not take", { provider_settings: { codex: { claude_compatibility: "on" } } }],
    ["tags of an agent this daemon does not serve", { agent_tags: { unknown_agent: ["review"] } }],
  ])("starts unchecked when the file brings only %s", async (_name, settings) => {
    const agent_settings = { ...switches, ...settings } as NonNullable<NonNullable<typeof REVIEW.accounts>["agent_settings"]>;
    expect(await opened({ agents_to_add: [], agents: [], agent_settings })).toBe(false);
  });

  it("starts unchecked when the name or switch it lists does not apply", async () => {
    expect(await opened({ agents_to_add: [], agents: [{ ...listed, default_name: "Main", default_disabled: true }], agent_settings: null })).toBe(false);
  });
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

it("shows no checkbox for a file that carries one region, only its tab, and imports that region", async () => {
  const only = { ...REVIEW, consoles: [], accounts: null, settings_sections: ["git" as const] };
  const mounted = await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...only }) : reply(body)) });
  // No region to check: the checkboxes are the Git section's own and the choice about the default clone directory.
  expect(tabNames()).toEqual(["Settings"]);
  expect(Array.from(document.body.querySelectorAll<HTMLInputElement>("input[type=checkbox]"), (input) => input.labels?.[0]?.textContent)).toEqual(["Git", "Use the file's default clone directory"]);
  expect(text()).toContain("Automatically pull repositories");
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: ["git"], resolutions: [] });
  expect(mounted.sent().at(-1)).not.toHaveProperty("apply_consoles");
});

it("keeps Import unavailable while the Settings are the only thing chosen and every section is unchecked", async () => {
  const mounted = await reviewing();
  const importButton = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Import")!;
  await act(async () => regionBox("Consoles and projects").click());
  await checkSettings();
  await openTab("Settings");
  expect(importButton().disabled).toBe(false);
  for (const label of SECTIONS) await act(async () => regionBox(label).click());
  expect(importButton().disabled).toBe(true);
  await act(async () => regionBox("Git").click());
  expect(importButton().disabled).toBe(false);
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: ["git"] });
});

it("sends no sections while the Settings are unchecked, whatever the sections' boxes say", async () => {
  const mounted = await reviewingSettings();
  await act(async () => regionBox("Cleanup").click());
  await openTab("Consoles and projects");
  await act(async () => regionBox("Settings").click());
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: [] });
});

it("imports without the consoles when their region is unchecked: none is asked for and no resolution is sent", async () => {
  const mounted = await reviewing();
  await checkSettings();
  await act(async () => regionBox("Consoles and projects").click());
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", resolutions: [], apply_settings: expect.arrayContaining(["git"]), apply_consoles: false });
});

it("notes what applying each application setting would change with a badge, and what follows only for one it turns on", async () => {
  await reviewingSettings();
  const lines = settingLines();
  expect(lines["Automatically pull repositories"]).toEqual({ badge: "Turns on", note: "Your repositories will then be pulled automatically." });
  // Turning a setting off, or leaving it as it is, has no consequence line.
  expect(lines["Automatically delete archived sessions whose conversation is gone"]).toEqual({ badge: "Turns off" });
  // The switches of the Agent accounts section are in the accounts part, not here.
  expect(lines["Automatically select an account"]).toBeUndefined();
  expect(lines["Start usage windows automatically"]).toBeUndefined();
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
  expect(settingLines()["Ask questions as choices"]).toEqual({ badge: "Turns on", note: "Sessions started afterwards are asked to present decisions as options you can pick." });
});

describe("the git workflow settings", () => {
  const FILE: GitWorkflowSettings = { development: "in_place", squash_before_delivery: false, rebase_before_delivery: true, delivery: "merge", revert_gate: true };
  const inspecting = (git_workflow: GitWorkflowSettings, current: typeof FILE) => {
    const settings = { ...REVIEW.settings!, git_workflow, current_git_workflow: current };
    return (body: ClientRequestBody) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected" as const, ...REVIEW, settings }) : reply(body));
  };

  it("lists the items that applying would change, each with what it is now", async () => {
    await reviewingSettings({ reply: inspecting(FILE, { ...FILE, development: "worktree", rebase_before_delivery: false, delivery: "ask" }) });
    expect(settingLines()["Git workflow"]).toEqual({
      badge: "Set to the file's",
      values: [
        "Where sessions develop: On the current branch (now In a worktree of its own)",
        "Rebase before delivery: On (now Off)",
        "Delivery: Merge locally (now Ask each time)",
      ],
    });
  });

  it("lists nothing when they are the same", async () => {
    await reviewingSettings({ reply: inspecting(FILE, FILE) });
    expect(settingLines()["Git workflow"]).toEqual({ badge: "No change" });
  });

  it("lists every item that differs, whichever it is", async () => {
    await reviewingSettings({ reply: inspecting(FILE, { development: "branch", squash_before_delivery: true, rebase_before_delivery: false, delivery: "pull_request", revert_gate: false }) });
    expect(settingLines()["Git workflow"].values).toEqual([
      "Where sessions develop: On the current branch (now On a branch of its own)",
      "Squash commits before delivery: Off (now On)",
      "Rebase before delivery: On (now Off)",
      "Delivery: Merge locally (now Open a pull request)",
      "Revert gate: On (now Off)",
    ]);
  });
});

describe("the settings of the Agent accounts section", () => {
  const withAgentSettings = (settings: Partial<NonNullable<NonNullable<typeof REVIEW.accounts>["agent_settings"]>>) => (body: ClientRequestBody) =>
    body.type === "inspect_configuration"
      ? Promise.resolve({
        type: "configuration_inspected" as const,
        ...REVIEW,
        settings: null,
        settings_sections: [],
        consoles: [],
        accounts: {
          agents_to_add: [],
          agents: [],
          agent_settings: {
            auto_select_account: false, current_auto_select_account: false,
            auto_switch_on_usage_limit: false, current_auto_switch_on_usage_limit: false,
            auto_start_quota_windows: false, current_auto_start_quota_windows: false,
            provider_settings: {}, current_provider_settings: {}, agent_tags: {}, current_agent_tags: {},
            ...settings,
          },
        },
      })
      : reply(body);
  const accountSettingLines = () => settingLinesOf(document.body.querySelector("[role=tabpanel] ul, section ul")!);
  const agentLines = (agent: string) => agentSettingLinesOf(cardTitled(document.body, agent)!);

  it("note what applying each would change, in the accounts part", async () => {
    await reviewing({ reply: withAgentSettings({ auto_select_account: true, auto_start_quota_windows: true, current_auto_start_quota_windows: false }) });
    const lines = accountSettingLines();
    expect(lines["Automatically select an account"]).toEqual({ badge: "Turns on", note: "A new session then uses the account whose weekly limit resets soonest." });
    expect(lines["Start usage windows automatically"]?.badge).toBe("Turns on");
    expect(lines["Start usage windows automatically"]?.note).toContain("counts a little toward the account's usage limits");
    expect(lines["Automatically switch accounts at the usage limit"]).toEqual({ badge: "No change" });
  });

  it("list, in each agent's card, what applying its provider settings and tags would change, with what it is now", async () => {
    await reviewing({
      reply: withAgentSettings({
        provider_settings: { codex: { claude_compatibility: false }, grok: { claude_compatibility: true } },
        current_provider_settings: { codex: { claude_compatibility: true }, grok: { claude_compatibility: true } },
        agent_tags: { codex: ["review", "ci"], grok: [], claude: ["same"] },
        current_agent_tags: { codex: [], grok: ["old"], claude: ["same"] },
      }),
    });
    const lines = agentLines("Codex");
    expect(lines["Claude compatibility"]).toEqual({ badge: "Set to the file's", values: ["Off (now On)"] });
    expect(lines["Tags"]).toEqual({ badge: "Set to the file's", values: ["review, ci (now No tags)"] });
    const grok = agentLines("Grok Build");
    expect(grok["Claude compatibility"]).toEqual({ badge: "No change" });
    expect(grok["Tags"]).toEqual({ badge: "Set to the file's", values: ["No tags (now old)"] });
    expect(agentLines("Claude Code")["Tags"]).toEqual({ badge: "No change" });
    // Nothing of an agent stands in the card of the switches.
    expect(Object.keys(accountSettingLines())).not.toContain("Tags");
  });

  it("list nothing when they are the same", async () => {
    await reviewing({
      reply: withAgentSettings({
        provider_settings: { codex: { claude_compatibility: true } },
        current_provider_settings: { codex: { claude_compatibility: true } },
        agent_tags: { codex: ["review"] },
        current_agent_tags: { codex: ["review"] },
      }),
    });
    const lines = agentLines("Codex");
    expect(lines["Claude compatibility"]).toEqual({ badge: "No change" });
    expect(lines["Tags"]).toEqual({ badge: "No change" });
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
  expect(await fallbackSentences(resultCard("Work laptop").querySelector("h3")!.parentElement!)).toContain("The Claude Code account Work is not set up here");
  expect(await fallbackSentences(row("Search API"))).toContain("The Codex account Personal is not set up here");
  await openTab("Settings");
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
  // Waiting on the system's picker is not busy: Cancel and the close button stay operable.
  const buttons = Array.from(document.body.querySelectorAll<HTMLButtonElement>("[role=dialog] button"));
  expect(buttons.find((b) => b.textContent === "Cancel")!.disabled).toBe(false);
  expect(buttons.find((b) => b.getAttribute("aria-label") === "Close")!.disabled).toBe(false);
  await act(async () => {
    document.body.querySelector("[role=dialog]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(mounted.closed()).toBe(1);
});

it("announces which choice was applied to how many, so two presses with the same count are told apart", async () => {
  fakeTimers();
  await reviewing();
  await act(async () => void (await vi.advanceTimersByTimeAsync(FIRST_WRITE_DELAY_MS)));
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
  { error: new ConfigurationFileError("too_large"), shown: "The file exceeds the supported size limit.", raw: undefined },
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
  const buttons = Array.from(document.body.querySelectorAll<HTMLButtonElement>("[role=dialog] button"));
  expect(buttons.find((b) => b.textContent === "Cancel")!.disabled).toBe(true);
  expect(buttons.find((b) => b.getAttribute("aria-label") === "Close")!.disabled).toBe(true);
  await act(async () => {
    document.body.querySelector("[role=dialog]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(mounted.closed()).toBe(0);
});

it("takes the window regaining focus, with no file chosen, as the picker dismissed, so the step can be tried again", async () => {
  fakeTimers();
  const mounted = mount({ unanswered: true });
  const choose = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Choose file")!;
  await press("Choose file");
  expect(choose().hasAttribute("data-pending")).toBe(true);
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(FOCUS_GRACE_MS);
  });
  expect(choose().hasAttribute("data-pending")).toBe(false);
  await press("Choose file");
  expect(mounted.open).toHaveBeenCalledTimes(2);
});

it("ignores the answer of a picker it gave up on once a newer one has been opened", async () => {
  fakeTimers();
  const answers: ((file: { name: string; text: string }) => void)[] = [];
  const mounted = mountDialog((onClose) => <ImportConfigurationDialog onClose={onClose} />, reply, {
    save: vi.fn(),
    open: vi.fn(() => new Promise<{ name: string; text: string }>((resolve) => answers.push(resolve))),
  });
  const choose = () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Choose file")!;
  await press("Choose file");
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(FOCUS_GRACE_MS);
  });
  expect(choose().hasAttribute("data-pending")).toBe(false);
  await press("Choose file");
  // The first picker answers late, while the second is the one being waited for.
  await act(async () => answers[0]({ name: "late.json", text: JSON.stringify(DOCUMENT) }));
  expect(mounted.sent()).toEqual([]);
  await act(async () => answers[1]({ name: "mine.json", text: JSON.stringify(DOCUMENT) }));
  expect(mounted.sent()).toEqual([{ type: "inspect_configuration", document: DOCUMENT }]);
});
