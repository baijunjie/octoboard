import type { Page } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, pageOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Report panel";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const consoleSession = sessionOf("s-console", console_.id, undefined, "Hub", "idle", { colour: "teal" });
const sessions = [
  consoleSession,
  sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working"),
];

const summary = (title: string) => `<!doctype html><html><head><title>${title}</title></head><body>
<h1>${title}</h1>
<p>Two sessions finished since the last report. Nothing needs your attention.</p>
<table><thead><tr><th>Session</th><th>Result</th></tr></thead>
<tbody><tr><td>Add idempotency keys</td><td>Merged</td></tr><tr><td>Audit the retry path</td><td>Needs review</td></tr></tbody></table>
<ul><li>Tests: 214 passed</li><li>Coverage: 91%</li></ul>
</body></html>`;

const form = `<!doctype html><html><head><title>Choose a release</title></head><body>
<h1>Which release should the console session cut?</h1>
<form>
  <p><label>Version <input type="text" name="version" value="1.4.0"></label></p>
  <p><label><input type="radio" name="channel" value="stable" checked> Stable</label>
     <label><input type="radio" name="channel" value="beta"> Beta</label></p>
  <p><label>Notes<br><textarea name="notes" rows="3" cols="30">Ship it.</textarea></label></p>
  <p><button type="submit">Submit</button></p>
</form>
</body></html>`;

const pages: Page[] = [
  pageOf("pg-1", consoleSession.id, summary("Morning summary"), 60 * 20),
  pageOf("pg-2", consoleSession.id, summary("Midday summary"), 60 * 8),
  pageOf("pg-3", consoleSession.id, form, 12),
];

const withPages = (list: Page[]) =>
  snapshotState({ consoles: [console_], projects: [project], sessions, pages: { [consoleSession.id]: list } });

// Two console sessions in one console, each with pages of its own.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "olive" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "idle", { colour: "jade" });
const twoSessions = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [hub1, hub2],
  pages: {
    [hub1.id]: [pageOf("h1-1", hub1.id, summary("Hub, first"), 60 * 6), pageOf("h1-2", hub1.id, summary("Hub, second"), 30)],
    [hub2.id]: [
      pageOf("h2-1", hub2.id, summary("Hub 2, first"), 60 * 5),
      pageOf("h2-2", hub2.id, summary("Hub 2, second"), 60 * 2),
      pageOf("h2-3", hub2.id, form, 10),
    ],
  },
});

export const reportScenarios: Scenario[] = [
  {
    id: "report-no-pages",
    group: GROUP,
    title: "No pages yet",
    state: withPages([]),
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "report-pages",
    group: GROUP,
    title: "Several pages, a form on the newest",
    description: "The newest page is a form, which can be submitted (the fixture accepts it).",
    state: withPages(pages),
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.wait(500)],
  },
  {
    id: "report-history",
    group: GROUP,
    title: "A history page, read-only",
    description: "Paged back one page: the read-only badge, with the page's controls disabled.",
    state: withPages(pages),
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.press(ui.t("report.previous")), (ui) => ui.wait(500)],
  },
  {
    id: "report-one-old-page",
    group: GROUP,
    title: "One page, a day old",
    state: withPages([{ ...pages[0], created_at: minutesAgo(60 * 30) }]),
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.wait(500)],
  },
  {
    id: "report-two-console-sessions",
    group: GROUP,
    title: "Two console sessions, each with its own pages",
    description:
      "Hub is paged back to its first page (read-only, 1 / 2). Selecting Hub 2 shows only its own pages, " +
      "on its newest (3 / 3, a form). Selecting Hub again shows its newest page, not where it was left.",
    state: twoSessions,
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.t("report.previous")),
      (ui) => ui.press(ui.session("Hub 2")),
      (ui) => ui.wait(500),
    ],
  },
];
