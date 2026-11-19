import { consoleOf, minutesAgo, sessionOf, snapshotState } from "../../fixtures/builders";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import type { Ui } from "../../fixtures/interact";
import type { FixtureError } from "../../fixtures/sampleFiles";
import { deletedEvent, entryOf, savedEvent } from "../../fixtures/memoryBuilders";
import type { MemoryEntry } from "../../protocol";
import type { Scenario } from "../scenario";

const GROUP = "Consoles";

const console_ = consoleOf("c-1", "Main");
// A console session, so the console has one; the entries name no session, only the user or an agent.
const hub = sessionOf("s-hub", console_.id, undefined, "Fix search", "idle");
// Memory is shown only while its setting is on.
const state = snapshotState({ consoles: [console_], projects: [], sessions: [hub], settings: { ...DEFAULT_SETTINGS, memory_enabled: true } });

/** An entry of the console's memory with the id `id`, updated `minutes` minutes before the page loaded, by the user unless
 * `extra` says otherwise. */
function entry(id: string, minutes: number, extra: Partial<MemoryEntry> = {}): MemoryEntry {
  return entryOf(id, { created_at: minutesAgo(minutes + 600), updated_at: minutesAgo(minutes), ...extra });
}

const byAgent = { kind: "agent" } as const;

const CHECKLIST = [
  "Before a release, in this order:",
  "",
  "1. Run `pnpm verify` in the release worktree.",
  "2. Read the changelog entry aloud to the user and wait for a go-ahead.",
  "3. Tag from `main` only; never from a feature branch.",
  "",
  "Ask before publishing anything. A published version cannot be taken back.",
].join("\n");

const checklist = entry("c4q8x1ma", 5, {
  summary: "The steps the user wants followed before every release",
  content: CHECKLIST,
  updated_by: byAgent,
});

// The longest summary an entry has (200 characters is the most), which a row clips with a fade and the view shows in full.
const longRelease = entry("r7k2m9qa", 70, {
  summary: "Before any release, follow the user's checklist in order: verify in the release worktree, read the changelog aloud, wait for a go-ahead, and tag from main only; never publish without asking.",
  local: true,
  content: CHECKLIST,
  updated_by: byAgent,
});

// An entry that holds only on this machine, which the edit form shows with its checkbox checked.
const staging = entry("e1v7b4tz", 60 * 30, { summary: "The staging environment is at staging.example.com behind the VPN", local: true });

/** Entries of both writers, the user and an agent; one
 * summary long enough to be clipped, two entries marked as holding only on this machine, and two that share a summary, since it is the only label and is not
 * unique. */
const mixedEntries: MemoryEntry[] = [
  checklist,
  entry("p2j5n8wd", 40, { summary: "Replies in Japanese, in short sentences", updated_by: { kind: "user" } }),
  longRelease,
  entry("f9d3h6ke", 95, { summary: "Do not summarise the diff at the end of a reply", updated_by: byAgent }),
  staging,
  entry("a8s2g5ur", 60 * 24 * 3, { summary: "Where the public API is documented", updated_by: byAgent }),
  entry("u6m3y9cn", 60 * 24 * 12, { summary: "Replies in Japanese, in short sentences", updated_by: byAgent }),
];

/** The most entries a console holds, the most recent first. */
const fullEntries: MemoryEntry[] = Array.from({ length: 200 }, (_, index) =>
  entry(`note${String(index + 1).padStart(4, "0")}`, index + 1, { summary: `Note number ${index + 1}`, local: index % 7 === 0 }),
);

const LISTING_FAILED: FixtureError = {
  code: "store_write_failed",
  params: { cause: "other", detail: "disk I/O error: Error code 10: SQLITE_IOERR" },
  message: "Octoboard could not read or write its own data: disk I/O error: Error code 10: SQLITE_IOERR",
};

const openMemory = [
  (ui: Ui) => ui.press(ui.t("sidebar.console.actions", { name: console_.name })),
  (ui: Ui) => ui.press(ui.t("sidebar.console.memory")),
];
/** Chooses `item` from the actions menu of the entry whose summary is `summary`. */
const fromRowMenu = (summary: string, item: (ui: Ui) => string) => [
  (ui: Ui) => ui.press(ui.t("memory.row.actions", { summary })),
  (ui: Ui) => ui.pressMenuItem(item(ui)),
];
const viewEntry = (summary: string) => fromRowMenu(summary, (ui) => ui.t("memory.view"));
const editEntry = (summary: string) => fromRowMenu(summary, (ui) => ui.t("common.edit"));

export const consoleMemoryScenarios: Scenario[] = [
  {
    id: "console-memory-empty",
    group: GROUP,
    category: "normal",
    title: "Console memory with no entries",
    description: "The empty panel says there is no memory yet; Add entry in the footer is the way to add one. Delete all is unavailable.",
    state,
    steps: openMemory,
  },
  {
    id: "console-memory-list",
    group: GROUP,
    category: "normal",
    title: "Console memory with entries",
    description: "A quiet count at the start of the footer, level with the buttons at its end, counts the entries against the limit (7 of 200 entries). Entries are listed most recently updated first. Each row is its summary on one line, which fades where it is clipped (the long one at the third place), then a \"This machine only\" tag where the entry is marked so (the long one and the staging one), then when and by whom it was updated: by you or by an agent. The last two entries share a summary, which is not unique.",
    state,
    memory: { entries: mixedEntries },
    steps: openMemory,
  },
  {
    id: "console-memory-full",
    group: GROUP,
    category: "normal",
    title: "Console memory holding the most entries",
    description: "With 200 entries the count at the start of the footer reads 200 of 200 entries, with no styling of its own, the list scrolls inside the dialog, and Add entry is unavailable; hover it for the reason.",
    state,
    memory: { entries: fullEntries },
    steps: openMemory,
  },
  {
    id: "console-memory-loading",
    group: GROUP,
    category: "normal",
    title: "Console memory loading",
    description: "The list is still being asked for: a loading line sits under the introduction, Add entry is unavailable and Delete all is too.",
    state,
    memory: { listing: "pending" },
    steps: openMemory,
  },
  {
    id: "console-memory-view",
    group: GROUP,
    category: "normal",
    title: "A memory entry in full",
    description: "The entry's summary in full, however long (choose a narrow width to see it wrap over several lines beside its \"This machine only\" tag, which this entry carries), then when and by whom it was last updated, then its Markdown content as written, in a block that scrolls when it is long. Close and Edit are in the footer.",
    state,
    memory: { entries: mixedEntries },
    steps: [...openMemory, ...viewEntry(longRelease.summary)],
  },
  {
    id: "console-memory-edit",
    group: GROUP,
    category: "normal",
    title: "Edit a memory entry",
    description: "Summary and Content hold the entry's values, with the cursor in Summary, and the \"This machine only\" checkbox under them is checked, with its note about configuration exports. There is no name or type field: the summary is the entry's only label.",
    state,
    memory: { entries: mixedEntries },
    steps: [...openMemory, ...editEntry(staging.summary)],
  },
  {
    id: "console-memory-delete",
    group: GROUP,
    category: "normal",
    title: "Delete a memory entry",
    description: "A confirmation names the entry by its summary, says console sessions will no longer see it, and offers Delete as a destructive button.",
    state,
    memory: { entries: mixedEntries },
    steps: [...openMemory, ...fromRowMenu(checklist.summary, (ui) => ui.t("common.delete"))],
  },
  {
    id: "console-memory-delete-all",
    group: GROUP,
    category: "normal",
    title: "Delete every memory entry",
    description: "A confirmation names the console and warns that it cannot be undone. The destructive Delete all stays unavailable until DELETE is typed.",
    state,
    memory: { entries: mixedEntries },
    steps: [...openMemory, (ui) => ui.press(ui.t("memory.deleteAll"))],
  },
  {
    id: "console-memory-load-failed",
    group: GROUP,
    category: "exceptional",
    title: "Console memory that could not be loaded",
    description: "In place of the list, a danger panel titled \"Could not load the memory\" says the app could not read or write its own data, with the raw database error in a copyable block under it, and Try again, which asks again and fails again here. Add entry and Delete all are unavailable.",
    state,
    memory: { listing: LISTING_FAILED },
    steps: openMemory,
  },
  {
    id: "console-memory-add-full",
    group: GROUP,
    category: "exceptional",
    title: "Add a memory entry once the memory has filled",
    description: "The form was opened with 199 entries and a session saved the 200th meanwhile, so pressing Add is refused before anything is sent: a danger alert at the top of the form, above Summary, says the memory already holds 200 entries and to delete one first. No field is marked.",
    state,
    memory: { entries: fullEntries.slice(1) },
    steps: [
      ...openMemory,
      (ui) => ui.press(ui.t("memory.add")),
      (ui) => ui.daemonEvents(savedEvent(fullEntries[0]!)),
      (ui) => ui.focus(ui.t("memory.field.summary")),
      (ui) => ui.type("Where the release notes are kept"),
      (ui) => ui.focus(ui.t("memory.field.content")),
      (ui) => ui.type("In docs/releases, one file per version."),
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "console-memory-add-invalid",
    group: GROUP,
    category: "exceptional",
    title: "Add a memory entry with nothing filled in",
    description: "Pressing Add on the empty form marks Summary and Content as required, each with its message under the field; the \"This machine only\" checkbox under them is unchecked.",
    state,
    memory: { entries: mixedEntries },
    steps: [
      ...openMemory,
      (ui) => ui.press(ui.t("memory.add")),
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "console-memory-view-deleted",
    group: GROUP,
    category: "exceptional",
    title: "A memory entry deleted while it is open",
    description: "The entry's view has been replaced by the line saying it was deleted while it was open, and Edit is unavailable. The row is gone from the list behind it.",
    state,
    memory: { entries: mixedEntries },
    steps: [
      ...openMemory,
      ...viewEntry(checklist.summary),
      (ui) => ui.daemonEvents(deletedEvent([checklist.id])),
    ],
  },
  {
    id: "console-memory-edit-replaced",
    group: GROUP,
    category: "exceptional",
    title: "A memory entry changed by a session while it is edited",
    description: "The Summary was edited here and keeps that edit; a session saved new Content meanwhile, so the field shows the session's text without disturbing the edit.",
    state,
    memory: { entries: mixedEntries },
    steps: [
      ...openMemory,
      ...editEntry(checklist.summary),
      (ui) => ui.focus(ui.t("memory.field.summary")),
      (ui) => ui.type("The steps to follow before every release, in order"),
      (ui) => ui.daemonEvents(savedEvent({
        ...checklist,
        content: `${CHECKLIST}\n\nAlways attach the changelog to the release.`,
        updated_at: Date.now(),
        version: checklist.version + 1,
      })),
    ],
  },
  {
    id: "console-memory-edit-refused",
    group: GROUP,
    category: "exceptional",
    title: "A memory entry saved from an out-of-date form",
    description: "A session saved the entry after this window last heard of it, so Save is refused: an alert titled Could not save the entry says it was changed since it was read. The form stays open and reads the entry again, so Content now shows the session's text, ending with the line about the changelog.",
    state,
    memory: {
      entries: mixedEntries,
      unheard: [{ ...checklist, content: `${CHECKLIST}\n\nAlways attach the changelog to the release.`, updated_at: minutesAgo(1), version: checklist.version + 1 }],
    },
    steps: [
      ...openMemory,
      ...editEntry(checklist.summary),
      (ui) => ui.confirm(),
      (ui) => ui.waitForText(ui.t("memory.saveFailedTitle")),
    ],
  },
];
