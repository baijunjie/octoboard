// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { DaemonRequestError } from "../daemon-client";
import { MIXED_RESULT, REVIEW } from "../gallery/fixtures/configuration";
import { ConfigurationFileError } from "../platform";
import type { Event, RequestBody } from "../protocol";
import { ImportConfigurationDialog } from "./ImportConfigurationDialog";
import { mountDialog, press } from "./testSupport";

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

it("shows a row for every project with where it was, and the directory its choice would use", async () => {
  await reviewing();
  expect(row("Website").textContent).toContain("/Users/ana/code/website");
  expect(row("Website").textContent).toContain("Exported path, placed there: /Users/ana/code/website");
  expect(row("Design System").textContent).toContain("Cloned from https://github.com/acme/design-system.git into /Users/dev/Projects/design-system");
  expect(row("Notes").textContent).toContain("Not found on the machine running Octoboard");
  // Where each was found is said on its path's line, whatever the choice made since.
  expect(row("Search API").textContent).toContain("Found in the default clone directory.");
  expect(row("Design System").textContent).toContain("Not found; can be cloned from its remote.");
  // Leaving a project out is said by its choice; only a project found nowhere adds a note.
  expect(row("Notes").textContent).not.toContain("Placed in");
});

it("keeps the dialog on the file step, with the daemon's reason, for a document it refuses", async () => {
  const refuse = (body: RequestBody) =>
    body.type === "inspect_configuration"
      ? Promise.reject(new DaemonRequestError("This is not a configuration file.", "configuration_not_recognised", {}))
      : reply(body);
  await reviewing({ reply: refuse });
  expect(text()).toContain("This is not a configuration file.");
  expect(text()).toContain("Choose a file exported from");
});

it("refuses a file that is not JSON without asking the daemon", async () => {
  const mounted = await reviewing({ text: "not json" });
  expect(text()).toContain("mine.json is not a JSON file.");
  expect(document.body.querySelector('[role="alertdialog"] code')?.textContent).toBe("mine.json");
  expect(mounted.sent()).toEqual([]);
});

it("does nothing when the open dialog is dismissed", async () => {
  const mounted = await reviewing({ text: undefined });
  expect(mounted.sent()).toEqual([]);
  expect(text()).toContain("Choose a file exported from");
});

it("changes only the row whose choice was changed", async () => {
  await reviewing();
  await choose("Website", "Default clone directory");
  expect(row("Website").textContent).toContain("Placed in /Users/dev/Projects/website");
  expect(row("Search API").textContent).toContain("Placed in /Users/dev/Projects/search-api");
  expect(row("Design System").textContent).toContain("Cloned from");
});

it("applies one choice to every project it is available for and leaves the others as they were", async () => {
  await reviewing();
  await choose("Website", "Default clone directory");
  await press("Use exported paths (1)");
  expect(row("Website").textContent).toContain("Exported path, placed there: /Users/ana/code/website");
  // Search API offers no exported path, so it keeps the choice it had.
  expect(row("Search API").textContent).toContain("Placed in /Users/dev/Projects/search-api");
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
  await act(async () => document.body.querySelector<HTMLInputElement>("input[type=checkbox]")!.click());
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration", apply_settings: true });
});

it("notes what applying each application setting would change", async () => {
  await reviewing();
  expect(text()).toContain("Turns on Automatically pull repositories");
  expect(text()).toContain("Turns off Automatically delete archived sessions whose conversation is gone");
});

it("leaves out the setting a file from before it existed does not carry", async () => {
  const older = { ...REVIEW, settings: { ...REVIEW.settings!, auto_delete_unresumable_archives: null } };
  await reviewing({ reply: (body) => (body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...older }) : reply(body)) });
  expect(text()).toContain("Turns on Automatically pull repositories");
  expect(text()).not.toContain("archived sessions whose conversation is gone");
});

it("stops before any request when a project is to go to a directory that is missing or not a path", async () => {
  const mounted = await reviewing();
  await choose("Notes", "Another directory");
  await press("Import");
  expect(text()).toContain("Enter a directory, or leave the project out.");
  type(row("Notes").querySelector("input")!, "relative/dir");
  expect(text()).toContain("Enter an absolute path, or one starting with ~/.");
  expect(mounted.sent().filter((body) => body.type === "import_configuration")).toEqual([]);
  type(row("Notes").querySelector("input")!, "~/notes");
  await press("Import");
  expect(mounted.sent().at(-1)).toMatchObject({
    resolutions: expect.arrayContaining([{ console: 0, project: 3, choice: { kind: "directory", path: "~/notes" } }]),
  });
});

it("tells each project's result in a table row, with the reason beside a failure, and the accounts that fell back apart", async () => {
  await reviewing();
  await press("Import");
  const line = (project: string) =>
    Array.from(document.body.querySelectorAll("tbody tr")).find((tr) => tr.querySelectorAll("td")[1]?.textContent?.endsWith(project))?.textContent ?? "";
  expect(line("Search API")).toContain("Work laptop");
  expect(line("Search API")).toContain("Created");
  expect(line("Search API")).toContain("In the default clone directory");
  expect(line("Website")).toContain("Failed");
  expect(line("Website")).toContain("/Users/ana/code/website does not exist.");
  expect(line("Notes")).toContain("Left out");
  expect(line("Design System")).toContain("Failed");
  expect(line("Design System")).toContain("git clone failed:");
  expect(line("The console itself")).toContain("the data directory cannot be written");
  expect(line("Dotfiles")).toContain("Not attempted");
  expect(line("Dotfiles")).toContain("Its console could not be created.");
  const section = (title: string) => Array.from(document.body.querySelectorAll("section")).find((s) => s.querySelector(":scope > h3")?.textContent === title)?.textContent ?? "";
  expect(section("Accounts that fell back to the default account")).toContain("Claude Code account Work, named by the console Work laptop");
  expect(section("Accounts that fell back to the default account")).toContain("Codex account Personal, named by the project Search API in Work laptop");
  expect(text()).toContain("Automatically pull repositories is now on.");
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
  expect(document.activeElement).toBe(document.querySelector('[role="alertdialog"]'));
  act(() => errorAlerts.acknowledge());
  expect(dialog.querySelector("h2")?.textContent).toBe("Import finished");
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
  await press("Use default clone directory (2)");
  const first = announced();
  await press("Clone from remotes (2)");
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
  { error: new ConfigurationFileError("too_large"), shown: "The file is too large to be a configuration file." },
  { error: new ConfigurationFileError("not_text"), shown: "The file is not a text file." },
  { error: new ConfigurationFileError("io", "Permission denied"), shown: "Could not read the file: Permission denied" },
])("words a file that could not be read from its kind: $shown", async ({ error, shown }) => {
  const mounted = mountDialog((onClose) => <ImportConfigurationDialog onClose={onClose} />, reply, {
    save: vi.fn(),
    open: vi.fn(async () => Promise.reject(error)),
  });
  await press("Choose file");
  expect(text()).toContain(shown);
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
