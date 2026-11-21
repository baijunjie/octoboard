import { consoleOf, minutesAgo, sessionOf, snapshotState } from "../../fixtures/builders";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import type { Ui } from "../../fixtures/interact";
import { byAgent, entryOf, PROJECT, PROJECT_SCOPE } from "../../fixtures/memoryBuilders";
import type { MemoryEntry } from "../../protocol";
import type { Scenario } from "../scenario";

const console_ = consoleOf("c-1", "Main");
const worker = sessionOf("s-app", console_.id, PROJECT.id, "Speed up the build", "idle");
// Memory is shown only while its setting is on.
const state = snapshotState({ consoles: [console_], projects: [PROJECT], sessions: [worker], settings: { ...DEFAULT_SETTINGS, memory_enabled: true } });

/** An entry of the memory of the project's place, updated `minutes` minutes before the page loaded, by the user unless
 * `extra` says otherwise. Entries of a place are never marked as holding only on one machine, since they all hold only on the machine the project is on. */
function entry(id: string, minutes: number, extra: Partial<MemoryEntry> = {}): MemoryEntry {
  return entryOf(id, { scope: PROJECT_SCOPE, created_at: minutesAgo(minutes + 600), updated_at: minutesAgo(minutes), ...extra });
}

const entries: MemoryEntry[] = [
  entry("b3n7q2xk", 12, { summary: "The dev server listens on port 4100, not the default", updated_by: byAgent("codex") }),
  entry("h5w8d1zt", 60 * 5, { summary: "Run the integration tests with the local Postgres from docker compose up" }),
  entry("m9c4v6jr", 60 * 26, { summary: "The checkout of the sibling repository is at /work/shared-ui", updated_by: byAgent("claude") }),
];

const openProjectMemory = [
  (ui: Ui) => ui.press(ui.t("sidebar.project.actions", { name: PROJECT.name })),
  (ui: Ui) => ui.press(ui.t("sidebar.project.memory")),
];

export const projectMemoryScenarios: Scenario[] = [
  {
    id: "project-memory-list",
    group: "Projects",
    category: "normal",
    title: "Project memory with entries",
    description: "The project's own memory, opened from the project row's menu: the introduction says it holds what agents working in the project need to know about the machine the project is on, such as local paths, ports, tools and account setup, shared by every agent working in the project. Rows have no tag, since every entry of a project's memory is machine-only, and show when and by whom each was updated: by the agent whose session saved it, named (Codex, Claude Code), or by you. Console memory has the same list with its \"This machine only\" tag on marked entries.",
    state,
    memory: { entries },
    steps: openProjectMemory,
  },
  {
    id: "project-memory-empty",
    group: "Projects",
    category: "normal",
    title: "Project memory with no entries",
    description: "The empty panel says there is no memory yet and that agents working in the project save here what they need to know about the machine the project is on, such as local paths and ports. Delete all is unavailable.",
    state,
    steps: openProjectMemory,
  },
  {
    id: "project-memory-add",
    group: "Projects",
    category: "normal",
    title: "Add a project memory entry",
    description: "The form has Summary and Content only: unlike a console's form, there is no \"This machine only\" checkbox under them.",
    state,
    memory: { entries },
    steps: [...openProjectMemory, (ui) => ui.press(ui.t("memory.add"))],
  },
  {
    id: "dialog-remove-project-memory",
    group: "Projects",
    category: "normal",
    title: "Remove a project that has memory",
    description: "Under the message, a \"Project memory\" section holds the unchecked option \"Also delete this project's memory\", its hint counting the 3 entries kept for the folder on the machine the project is on and saying they can be deleted later in Settings > Memory, the two steps as chips with a chevron between them. The section is absent for a project without memory, or one whose folder another project shares.",
    state,
    memory: { entries },
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: PROJECT.name })),
      (ui) => ui.press(ui.t("sidebar.project.remove")),
      (ui) => ui.waitForText(ui.t("dialog.removeProject.memory")),
    ],
  },
];
