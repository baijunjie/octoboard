import type { Agent } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Archive view";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const AGENTS: Agent[] = ["claude", "codex", "grok"];

const archived = Array.from({ length: 70 }, (_, i) =>
  sessionOf(`s-a${i}`, console_.id, project.id, i % 7 === 3 ? `Audit the retry path ${i}` : i % 11 === 5 ? `A very long archived session title that does not fit one row ${i}` : `Archived session ${i}`, "archived", {
    agent: AGENTS[i % 3],
    ended_at: minutesAgo(20 + i * 400),
  }),
);
const consoleSessions = Array.from({ length: 8 }, (_, i) =>
  sessionOf(`s-c${i}`, console_.id, undefined, `Hub ${i + 1}`, "archived", { ended_at: minutesAgo(60 * (i + 1)) }),
);

const state = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [sessionOf("s-live", console_.id, project.id, "Live session", "idle"), ...archived, ...consoleSessions],
});

export const archiveScenarios: Scenario[] = [
  {
    id: "archive-many",
    group: GROUP,
    title: "A project's archive, many entries",
    description: "Seventy archived sessions, listed thirty at a time as the end scrolls into view. The empty view is not reachable: the menu only offers View all when something is archived.",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll", { count: archived.length })),
    ],
  },
  {
    id: "archive-console-sessions",
    group: GROUP,
    title: "A console's archived console sessions",
    state,
    steps: [
      (ui) => ui.press(ui.t("sidebar.consoleSession.actions", { name: console_.name })),
      (ui) => ui.press(ui.t("sidebar.archive.consoleSessions")),
      (ui) => ui.press(ui.t("sidebar.archive.viewAll", { count: consoleSessions.length })),
    ],
  },
];
