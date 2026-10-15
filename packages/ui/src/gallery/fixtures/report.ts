import type { Scenario } from "../scenario";
import { consoleOf, pageOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Report panel";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
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

// Four console sessions keep empty, old, historical and interactive reports independently.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "olive" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "idle", { colour: "jade" });
const emptyHub = sessionOf("s-hub-empty", console_.id, undefined, "No reports", "idle", { colour: "azure" });
const oldHub = sessionOf("s-hub-old", console_.id, undefined, "Old report", "idle", { colour: "violet" });
const twoSessions = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [hub1, hub2, emptyHub, oldHub],
  pages: {
    [emptyHub.id]: [],
    [oldHub.id]: [pageOf("old-1", oldHub.id, summary("A day old"), 60 * 30)],
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
    id: "report-two-console-sessions",
    group: GROUP,
    category: "normal",
    title: "Latest interactive report",
    state: twoSessions,
    description: "The latest report is an interactive form; pagination belongs to the selected console session.",
    steps: [
      (ui) => ui.press(ui.session("Hub 2")),
    ],
  },
  {
    id: "report-history",
    group: GROUP,
    category: "normal",
    title: "Historical report",
    state: twoSessions,
    description: "The first page is read-only and its controls are disabled.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.t("report.previous")),
    ],
  },
  {
    id: "report-no-pages",
    group: GROUP,
    category: "normal",
    title: "No reports",
    state: twoSessions,
    steps: [
      (ui) => ui.press(ui.session("No reports")),
    ],
  },
  {
    id: "report-one-old-page",
    group: GROUP,
    category: "normal",
    title: "One old report",
    state: twoSessions,
    steps: [
      (ui) => ui.press(ui.session("Old report")),
    ],
  },
];
